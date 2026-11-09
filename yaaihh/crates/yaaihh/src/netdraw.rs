//! The drawing of a net beside its text on the Clerk page: the net as written, read-only, top to
//! bottom. coordd parses the text (`Request::NetGraph`) into nested blocks; `coord_types::netlayout` places them (a
//! sequence on a shared axis, a branch's arms side by side or stacked on a rail, a loop framed) with this page's text
//! sizes, and this module draws what it placed at those positions, so every line drawn is one the layout's
//! connectivity test walked. Each kind of step has its own shape: the event a pill, a model call a card marked with
//! its session (or "fresh"), a decision an amber card, a test a plain outline, an effect a green pill (solid when
//! turned on, outlined when only shown), a fault red; what leaves the net (an event raised, a wake) is an arrow out of
//! its step. A click on a step selects its lines in the text; the caret's line marks its step. While the text does not
//! parse, the last good drawing stays with the error above it. With a run picked or tried, the lines and steps it went
//! through are marked and each one's output shows on hover.

use crate::App;
use coord_types::netlayout::{self, Font, Placed, What};
use coord_types::{FlowNode, NetFlow, NetGraph, NetRun};
use rune::{El, col, el, row, text};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};

/// What the drawing marks: the file and net it is of (a click selects in the net's lines), the caret's step (its lines), and a run's
/// outputs by source line (none: no run shown).
pub struct Marks<'a> {
    pub path: &'a str,
    pub name: &'a str,
    pub caret: Option<(usize, usize)>,
    pub run: Option<HashMap<usize, Vec<String>>>,
    pub live: &'a dyn Fn(&str) -> bool,
}

/// RUN's outputs by source line: each call's answer, each effect taken or not.
pub fn outputs(run: &NetRun) -> HashMap<usize, Vec<String>> {
    let mut by: HashMap<usize, Vec<String>> = HashMap::new();
    for st in &run.steps {
        let out = st.out.as_ref().map(|o| o.as_str().map(String::from).unwrap_or_else(|| o.to_string())).unwrap_or_else(|| "no answer".into());
        by.entry(st.line).or_default().push(format!("{} → {}", st.what, short(&out, 400)));
    }
    for e in &run.effects {
        let done = match (e.live, e.done.as_deref()) {
            (false, _) => "would".to_string(),
            (true, Some(d)) => format!("did → {d}"),
            (true, None) => "did".into(),
        };
        by.entry(e.line).or_default().push(format!("{} ({done}): {}", e.effect, short(&e.args.to_string(), 300)));
    }
    by
}

fn short(t: &str, n: usize) -> String {
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}

/// The step the caret is on: the narrowest whose lines hold LINE (1-based).
pub fn caret_step(g: &NetGraph, line: usize) -> Option<(usize, usize)> {
    fn walk(f: &NetFlow, line: usize, best: &mut Option<(usize, usize)>) {
        let mut take = |n: &FlowNode| {
            if n.lines.0 <= line && line <= n.lines.1 && best.is_none_or(|b| n.lines.1 - n.lines.0 <= b.1 - b.0) {
                *best = Some(n.lines);
            }
        };
        match f {
            NetFlow::Seq(xs) => xs.iter().for_each(|x| walk(x, line, best)),
            NetFlow::Node(n) => take(n),
            NetFlow::Branch { head, arms } => {
                // a branch head's lines cover its arms: its own line is what marks it
                if head.lines.0 == line {
                    *best = Some((line, line));
                }
                arms.iter().for_each(|a| walk(&a.body, line, best));
            }
            NetFlow::Each { head, body } => {
                if head.lines.0 == line {
                    *best = Some((line, line));
                }
                walk(body, line, best);
            }
        }
    }
    let mut best = None;
    if g.start.lines.0 <= line && line <= g.start.lines.1 {
        best = Some(g.start.lines);
    }
    walk(&g.body, line, &mut best);
    best
}

/// Whether the caret marks node N (a head is marked from its own line only).
fn marked(n: &FlowNode, m: &Marks, head: bool) -> bool {
    m.caret.is_some_and(|c| if head { c == (n.lines.0, n.lines.0) } else { c == n.lines })
}

const LINE: f32 = 1.5;

/// The drawing of net G: a legend of the shapes, then the net as `netlayout` places it.
pub fn draw(g: &NetGraph, m: &Marks, k: &Kit) -> El {
    let c_ = &k.c;
    let key = |fill: rune::Color, border: rune::Color, radius: f32, word: &str| {
        row().gap(4.0).center_y().child(el().w(14.0).h(10.0).radius(radius).fill(fill).border(1.0, border)).child(text(word.to_string()).font(k.ty.small).color(c_.text_faint))
    };
    let legend = row()
        .max_w(netlayout::ROOM)
        .gaps(4.0, 12.0)
        .flex_wrap()
        .pb(10.0)
        .child(key(c_.layer_02, c_.border_base, 2.0, "model call (blue bar: in a session)"))
        .child(key(c_.warning.bg, c_.warning.border, 2.0, "decision"))
        .child(key(rune::Color::TRANSPARENT, c_.border_strong, 1.0, "test"))
        .child(key(c_.success.bg, c_.success.border, 5.0, "effect turned on"))
        .child(key(rune::Color::TRANSPARENT, c_.success.border, 5.0, "only shown"))
        .child(key(c_.danger.bg, c_.danger.border, 5.0, "fault"));
    let style = |f: Font| if f == Font::Label { k.ty.small_medium } else { k.ty.small };
    let measure = |t: &str, f: Font, max: f32| rune::text::measure(t, &[], style(f), Some(max));
    let went: Option<HashSet<usize>> = m.run.as_ref().map(|r| r.keys().copied().collect());
    let l = netlayout::layout(g, &measure, netlayout::ROOM, went.as_ref());
    let mut canvas = col().w(l.w).h(l.h);
    // frames first (behind), then the lines, then the boxes over their ends
    for p in l.items.iter().filter(|p| p.what == What::Frame) {
        canvas = canvas.child(el().absolute(p.x, p.y).w(p.w).h(p.h).radius(10.0).border(1.0, c_.border_base));
    }
    for sg in &l.segs {
        let (x, y) = (sg.x0.min(sg.x1) - LINE / 2.0, sg.y0.min(sg.y1) - LINE / 2.0);
        let (w, h) = ((sg.x1 - sg.x0).abs() + LINE, (sg.y1 - sg.y0).abs() + LINE);
        canvas = canvas.child(el().absolute(x, y).w(w).h(h).fill(if sg.lit { c_.success.icon } else { c_.icon_faint }));
    }
    for p in &l.items {
        canvas = match &p.what {
            What::Frame => canvas,
            What::Step { .. } => step(canvas, p, m, k),
            What::Arm(label) => canvas.child(
                row()
                    .absolute(p.x, p.y)
                    .w(p.w)
                    .h(p.h)
                    .px(netlayout::CHIP_X)
                    .py(netlayout::CHIP_Y)
                    .radius(999.0)
                    .fill(if p.lit { c_.success.bg } else { c_.layer_02 })
                    .child(text(label.clone()).font(k.ty.small).color(if p.lit { c_.success.fg } else { c_.text_muted }).max_w(netlayout::TEXT_W).wrap()),
            ),
            What::Done => canvas.child(
                row()
                    .absolute(p.x, p.y)
                    .w(p.w)
                    .h(p.h)
                    .px(netlayout::PAD_X)
                    .py(netlayout::PAD_Y)
                    .radius(999.0)
                    .fill(c_.layer_01)
                    .border(1.0, if p.lit { c_.success.icon } else { c_.border_strong })
                    .child(text("done").font(k.ty.small).color(c_.text_muted)),
            ),
        };
    }
    col().padding(12.0).child(legend).child(canvas)
}

/// CANVAS with step P drawn on it in its kind's shape, its arcs out to its right.
fn step(canvas: El, p: &Placed, m: &Marks, k: &Kit) -> El {
    let What::Step { node: n, head, shape_w, detail, outs, .. } = &p.what else { return canvas };
    let c = &k.c;
    let live = matches!(n.kind.as_str(), "effect" | "raise") && (m.live)(n.label.split('(').next().unwrap_or(""));
    let (fill, border, ink, radius) = match n.kind.as_str() {
        "start" => (c.info.bg, c.info.border, c.info.fg, 999.0),
        "call" => (c.layer_02, c.border_base, c.text_base, 6.0),
        "decide" => (c.warning.bg, c.warning.border, c.warning.fg, 6.0),
        "test" => (c.layer_01, c.border_strong, c.text_base, 2.0),
        "each" => (c.layer_03, c.border_base, c.text_base, 6.0),
        "data" => (c.layer_01, c.border_weak, c.text_faint, 4.0),
        "effect" if live => (c.success.bg, c.success.border, c.success.fg, 999.0),
        "effect" => (c.layer_01, c.success.border, c.text_muted, 999.0),
        "fault" => (c.danger.bg, c.danger.border, c.danger.fg, 999.0),
        "raise" => (c.info.bg, c.info.border, c.info.fg, 999.0),
        _ => (c.layer_02, c.border_base, c.text_base, 6.0),
    };
    let font = if netlayout::label_font(n) == Font::Label { k.ty.small_medium } else { k.ty.small };
    let bar = if n.kind == "call" && n.session.is_some() { netlayout::BAR } else { 0.0 };
    let mut body = col().gap(2.0).child(text(n.label.clone()).font(font).color(ink).max_w(netlayout::TEXT_W).wrap());
    if detail.is_some() {
        body = body.child(text(netlayout::detail(n)).font(k.ty.small).color(c.text_faint).max_w(netlayout::TEXT_W).wrap());
    }
    let caret = marked(n, m, *head);
    let outs_run = m.run.as_ref().and_then(|r| r.get(&n.lines.0));
    let (bw, bc) = if caret {
        (2.0, c.border_focus)
    } else if p.lit && outs_run.is_some() {
        (2.0, c.success.icon)
    } else {
        (1.0, border)
    };
    let mut shape = col().w(*shape_w).h(p.h).pl(netlayout::PAD_X + bar).pr(netlayout::PAD_X).pt(netlayout::PAD_Y).radius(radius).fill(fill).border(bw, bc);
    // a call in a session: a bar at its left edge (fresh calls have none)
    if bar > 0.0 {
        shape = shape.child(el().absolute(netlayout::PAD_X - 1.0, netlayout::PAD_Y).w(3.0).h(p.h - 2.0 * netlayout::PAD_Y).radius(1.5).fill(c.icon_accent));
    }
    shape = shape.child(body);
    // steps without an output of their own (the event, a test, a loop head, plain data) are never dimmed
    if m.run.is_some() && outs_run.is_none() && !matches!(n.kind.as_str(), "start" | "test" | "data" | "each") {
        shape = shape.opacity(0.45);
    }
    let help = match outs_run {
        Some(o) => o.join("\n"),
        None => format!("lines {}–{}: click to select them in the text", n.lines.0, n.lines.1),
    };
    let (path, name, lines) = (m.path.to_string(), m.name.to_string(), if *head { (n.lines.0, n.lines.0) } else { n.lines });
    let mut canvas = canvas.child(col().absolute(p.x, p.y).child(k.interactive(shape).help(help).on_press(move |s: &mut App| select_lines(s, &path, &name, lines))));
    let mut x = p.x + shape_w;
    let mid = p.y + p.h / 2.0;
    for (o, (w, h)) in n.out.iter().zip(outs) {
        canvas = canvas.child(el().absolute(x, mid - LINE / 2.0).w(netlayout::OUT_LINE).h(LINE).fill(c.info.border));
        x += netlayout::OUT_LINE;
        canvas = canvas.child(
            row()
                .absolute(x, mid - h / 2.0)
                .w(*w)
                .h(*h)
                .px(6.0)
                .py(netlayout::CHIP_Y)
                .radius(4.0)
                .border(1.0, c.info.border)
                .child(text(format!("→ {o}")).font(k.ty.small).color(c.info.fg).max_w(netlayout::OUT_W).wrap()),
        );
        x += w;
    }
    canvas
}

/// Select lines A..=B (1-based, of the file) of net NAME's lines in file PATH in the editor.
fn select_lines(s: &mut App, path: &str, name: &str, (a, b): (usize, usize)) {
    let piece = crate::clerk::piece_mut(s, path, name);
    let t = &mut piece.edit;
    let n = t.text.lines().count().max(1);
    let (a, b) = (a.saturating_sub(piece.line).clamp(1, n) - 1, b.saturating_sub(piece.line).clamp(1, n) - 1);
    let (from, to) = (t.line_range(a).start, t.line_range(b).end);
    t.set_selection(from, to);
    t.focused = true;
}

const KEYWORDS: &[&str] = &[
    "net", "on", "session", "per", "if", "else", "match", "let", "for", "in", "where", "fresh", "type", "call", "none", "new", "true", "false",
];

/// Net or call text T coloured, all of it in mono: keywords, strings, numbers, comments dimmed, type names,
/// and the words the drawing gives a shape in that shape's colour: a call (one of CALLS, before its `(`) in the session
/// bar's blue, `decide` and its option in the decision's amber, an effect in the effect's green, a fault in red, a
/// raise in the event's blue.
pub fn highlight(t: &str, calls: &HashSet<String>, k: &Kit) -> Vec<rune::Span> {
    use rune_kit::syntax::{Syntax, SyntaxTheme};
    let theme = SyntaxTheme::from_kit(k);
    let c = &k.c;
    let b = t.as_bytes();
    let mut out: Vec<rune::Span> = Vec::new();
    let mut put = |from: usize, to: usize, color: Option<rune::Color>, bold: bool| {
        let mut sp = rune::Span::new(from, to);
        sp.mono = true;
        sp.color = color;
        if bold {
            sp.weight = Some(600.0);
        }
        out.push(sp);
    };
    let word = |ch: u8| ch.is_ascii_alphanumeric() || ch == b'_';
    let next_is_paren = |mut j: usize| {
        while j < b.len() && b[j] == b' ' {
            j += 1;
        }
        j < b.len() && b[j] == b'('
    };
    // a call's template is prose for the model: no words of the language in it (lines indented past its `template`)
    let mut prose: Vec<std::ops::Range<usize>> = Vec::new();
    let mut at = 0;
    let mut under: Option<usize> = None;
    for l in t.split_inclusive('\n') {
        let indent = l.len() - l.trim_start().len();
        match under {
            Some(n) if l.trim().is_empty() || indent > n => prose.push(at..at + l.len()),
            _ => under = (l.trim() == "template").then_some(indent),
        }
        at += l.len();
    }
    let (mut i, mut plain) = (0, 0);
    let mut after_decide = false;
    while i < b.len() {
        let start = i;
        if let Some(r) = prose.iter().find(|r| r.contains(&i)) {
            if plain < start {
                put(plain, start, None, false);
            }
            put(start, r.end, Some(c.text_muted), false);
            (i, plain) = (r.end, r.end);
            continue;
        }
        let (color, bold) = if b[i..].starts_with(b"//") {
            i = t[i..].find('\n').map_or(b.len(), |n| i + n);
            (Some(c.text_faint), false)
        } else if b[i] == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                i += if b[i] == b'\\' { 2 } else { 1 };
            }
            i = (i + 1).min(b.len());
            (Some(theme.color(Syntax::String)), false)
        } else if after_decide && (word(b[i]) || b[i] == b'-') {
            while i < b.len() && (word(b[i]) || b[i] == b'-') {
                i += 1;
            }
            after_decide = false;
            (Some(c.warning.fg), true)
        } else if b[i].is_ascii_digit() {
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            (Some(theme.color(Syntax::Number)), false)
        } else if word(b[i]) {
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            let w = &t[start..i];
            let called = next_is_paren(i);
            match w {
                "decide" => {
                    after_decide = true;
                    (Some(c.warning.fg), true)
                }
                "fault" => (Some(c.danger.fg), true),
                "raise" => (Some(c.info.fg), true),
                _ if called && coord_types::NET_EFFECTS.contains(&w) => (Some(c.success.fg), true),
                _ if called && calls.contains(w) => (Some(c.icon_accent), true),
                _ if KEYWORDS.contains(&w) => (Some(theme.color(Syntax::Keyword)), false),
                _ if w.starts_with(|ch: char| ch.is_ascii_uppercase()) => (Some(theme.color(Syntax::Type)), false),
                _ => {
                    continue;
                }
            }
        } else {
            i += t[i..].chars().next().map_or(1, char::len_utf8);
            if after_decide && !b[start].is_ascii_whitespace() {
                after_decide = false;
            }
            continue;
        };
        if plain < start {
            put(plain, start, None, false);
        }
        put(start, i, color, bold);
        plain = i;
    }
    if plain < b.len() {
        put(plain, b.len(), None, false);
    }
    out
}
