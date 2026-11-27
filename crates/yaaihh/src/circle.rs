//! The context circle (the owner, N-481 p12: "show as a circle for the amx size so 1m in most cases with a red line
//! where the ocnext is a nd a visual hwo full it is"; N-541 post 32: "see that small one at the top that should have
//! the red mark and be dragged . uifi  click it like something else shows up that is not right . that element there
//! should be then editable"). One circle, in the chat's header, and it is the control: the ring is the model's whole
//! window, the arc how full the context is (the view's `context.used`), the red mark where its harness compacts it
//! (`context.window`: Claude Code's auto-compact window, Codex's `model_auto_compact_token_limit`; coordd keeps the
//! point per agent, roster gives it).
//!
//! N-804.p16, the owner: "instead of circle i prefer red line there to drag and then when i click say the circle
//! itself give the context"; "i dont want that text there polluting the ui at all". The mark is a red line across the
//! ring, dragged round it; nothing is written beside the circle, at rest or after a drag: while the line is dragged the
//! number shows at the pointer, and goes on release; its hover says the two figures (used of the window, where it
//! compacts). A click opens the context in a small panel under the circle: the bar by kind with its numbers, and the
//! two figures; a click elsewhere or Escape closes it. A drag applies at once: coordd types `/autocompact N` into a
//! running Claude Code session (once no turn runs and its input box is empty) and takes the session's answer from
//! its record; meanwhile the line stands at the new place. Only a session started with a window that stands above
//! the command (before E6) needs a restart, which its hover says; a Codex agent takes the point at its next start.

use crate::App;
use coord_types::AgentView;
use rune::{El, Icon, col, icon, row, text};
use rune_kit::Kit;
use std::sync::Mutex;

/// The model's whole window while the view does not give it (`context.max`): every Claude model we run has a native 1M
/// window (accounts-3, N-490.p2), and Claude Code caps a compaction point there.
pub const WHOLE: u64 = 1_000_000;
/// The lowest point the mark can be dragged to (Claude Code's bound for `autoCompactWindow`, N-490.p2).
pub const LEAST: u64 = 100_000;
/// The mark moves in steps of this many tokens.
pub const STEP: u64 = 10_000;

/// The mark being dragged on agent (project, name): the tokens the pointer is at now (`on`), from where the pointer
/// went down (`from`: a press that does not move is no drag but a click, which opens the context), and the pointer
/// in window points (`pointer`), where the number shows while dragged. `open`: the agent whose context panel is open.
#[derive(Default)]
pub struct Drag {
    pub on: Option<(usize, String, u64)>,
    pub from: Option<(f32, f32)>,
    pub pointer: Option<[f32; 2]>,
    pub open: Option<(usize, String)>,
    /// The agent whose circle the pointer is over: its mark drawn held (`drawn`).
    pub hot: Option<(usize, String)>,
}

/// The pointer has left where it went down by more than this many pixels: a drag.
const SLACK: f32 = 3.0;

/// Tokens as he reads them: "143k", "1M".
pub fn tokens(n: u64) -> String {
    if n >= 1_000_000 && n % 1_000_000 == 0 {
        format!("{}M", n / 1_000_000)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1000 {
        format!("{}k", (n + 500) / 1000)
    } else {
        n.to_string()
    }
}

/// The whole window of A: the view's, else 1M.
pub(crate) fn whole(a: &AgentView) -> u64 {
    a.context.as_ref().and_then(|c| c.max).filter(|m| *m > 0).unwrap_or(WHOLE)
}

/// The point the mark shows: the one kept for its next start, else the one it runs with.
pub fn point(a: &AgentView) -> Option<u64> {
    let c = a.context.as_ref()?;
    c.next.or(c.window).filter(|w| *w > 0)
}

/// The circle's parts as fractions of the whole: used, and the mark (none when the point is not known).
pub fn parts(a: &AgentView) -> Option<(f32, Option<f32>)> {
    let c = a.context.as_ref()?;
    let w = whole(a);
    let f = |n: u64| (n as f32 / w as f32).clamp(0.0, 1.0);
    Some((f(c.used), point(a).map(f)))
}

/// The tokens at (X, Y) on a circle W by H: its angle from the top, clockwise, in steps of STEP between LEAST and
/// the whole window.
pub fn tokens_at(x: f32, y: f32, w: f32, h: f32, whole: u64) -> u64 {
    let a = (x - w / 2.0).atan2(h / 2.0 - y).rem_euclid(std::f32::consts::TAU);
    let t = (a / std::f32::consts::TAU) as f64 * whole as f64;
    ((t / STEP as f64).round() as u64 * STEP).clamp(LEAST, whole)
}

const RING: Icon = Icon {
    name: "context-ring",
    view: 100.0,
    body: r#"<circle cx="50" cy="50" r="42" stroke="currentColor" stroke-width="8" fill="none"/>"#,
};

/// A static icon per step of 1% (icons are static; each step is made once).
fn cached(cache: &'static Mutex<Vec<Option<&'static Icon>>>, step: usize, make: impl FnOnce() -> String) -> &'static Icon {
    let mut c = cache.lock().unwrap_or_else(|e| e.into_inner());
    if c.len() < 101 {
        c.resize(101, None);
    }
    if let Some(i) = c[step] {
        return i;
    }
    let body: &'static str = Box::leak(make().into_boxed_str());
    let i: &'static Icon = Box::leak(Box::new(Icon { name: "context-part", view: 100.0, body }));
    c[step] = Some(i);
    i
}

fn at(f: f32, r: f32) -> (f32, f32) {
    let a = f * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    (50.0 + r * a.cos(), 50.0 + r * a.sin())
}

/// The arc from the top clockwise to F.
fn arc(f: f32) -> &'static Icon {
    static CACHE: Mutex<Vec<Option<&'static Icon>>> = Mutex::new(Vec::new());
    let step = (f.clamp(0.0, 1.0) * 100.0).round() as usize;
    cached(&CACHE, step, || match step {
        0 => String::new(),
        100 => RING.body.to_string(),
        _ => {
            let (x, y) = at(step as f32 / 100.0, 42.0);
            format!(r#"<path d="M50 8A42 42 0 {} 1 {x:.2} {y:.2}" stroke="currentColor" stroke-width="8" fill="none"/>"#, (step > 50) as u8)
        }
    })
}

/// The mark at F as the project's look draws it (coord_types::look, N-866; the built-in look is the owner's red line
/// across the ring, N-804.p16: "red line there to drag"): a line from `from` to `to` (from the circle's centre, its
/// box 100), a dot `width` across on the ring, or a thin tick (never nothing: N-866.p6, 1). Built here from the look's numbers; the look
/// carries no drawing of its own. A press anywhere on the circle takes it, so it need not be caught.
fn mark(f: f32, m: &coord_types::look::Mark) -> Option<&'static Icon> {
    use coord_types::look::Shape;
    type Key = (usize, u8, u32, u32, u32);
    static CACHE: Mutex<Vec<(Key, &'static Icon)>> = Mutex::new(Vec::new());
    let step = (f.clamp(0.0, 1.0) * 100.0).round() as usize;
    let key: Key = (step, m.shape as u8, m.from.to_bits(), m.to.to_bits(), m.width.to_bits());
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, i)) = c.iter().find(|(k, _)| *k == key) {
        return Some(i);
    }
    let f = step as f32 / 100.0;
    let body = match m.shape {
        Shape::Line | Shape::Tick => {
            let ((x0, y0), (x1, y1)) = (at(f, m.from), at(f, m.to));
            let w = if m.shape == Shape::Tick { (m.width / 2.0).max(2.0) } else { m.width };
            format!(r#"<path d="M{x0:.2} {y0:.2}L{x1:.2} {y1:.2}" stroke="currentColor" stroke-width="{w:.1}" stroke-linecap="round" fill="none"/>"#)
        }
        Shape::Dot => {
            let (x, y) = at(f, 42.0);
            format!(r#"<circle cx="{x:.2}" cy="{y:.2}" r="{:.1}" fill="currentColor"/>"#, m.width / 2.0)
        }
    };
    let i: &'static Icon = Box::leak(Box::new(Icon { name: "context-mark", view: 100.0, body: Box::leak(body.into_boxed_str()) }));
    c.push((key, i));
    Some(i)
}

/// The circle's side in the header.
pub const SIZE: f32 = 30.0;

/// The circle at SIZE: the ring faint, how full in the text's colour, the mark as LOOK draws it (the built-in: a red
/// line across the ring), a little smaller at rest; HELD (hovered or dragged), the mark alone answers, a little longer,
/// wider and brighter, and back on release (the owner, N-2003: no box or handle over the control).
fn drawn(used: f32, point: Option<f32>, size: f32, look: &coord_types::look::Mark, held: bool, k: &Kit) -> El {
    let mut c = rune::el().size(size, size);
    c = c
        .child(icon(&RING, size).absolute(0.0, 0.0).color(k.c.text_faint).opacity(0.35))
        .child(icon(arc(used), size).absolute(0.0, 0.0).color(k.c.text_base).opacity(0.8));
    if let Some(i) = point.and_then(|p| mark(p, &sized(look, held))) {
        c = c.child(icon(i, size).absolute(0.0, 0.0).color(mark_colour(look, k)).opacity(if held { 1.0 } else { 0.85 }).key(if held { "context-mark-held" } else { "context-mark" }));
    }
    c
}

/// LOOK's mark as drawn: at rest a little shorter and thinner than the look gives it; held, longer toward the centre
/// (its outer end at most on the ring's edge) and wider.
fn sized(look: &coord_types::look::Mark, held: bool) -> coord_types::look::Mark {
    let (grow, wide) = if held { (2.0, 1.15) } else { (-2.0, 0.8) };
    let (from, to) = if look.from <= look.to { (look.from - grow, look.to + grow) } else { (look.from + grow, look.to - grow) };
    coord_types::look::Mark { from: from.clamp(0.0, 50.0), to: to.clamp(0.0, 50.0), width: look.width * wide, ..look.clone() }
}

/// The circle as MARK draws it, twice its size in the header, two fifths used and the mark at seven tenths: the look's
/// editor draws it beside the text as it stands (look.rs `preview`; N-866.p6, 3).
pub fn sample(mark: &coord_types::look::Mark, k: &Kit) -> El {
    drawn(0.4, Some(0.7), SIZE * 2.0, mark, false, k)
}

/// The mark's colour in K, the frame's Kit with the look in it (look.rs `apply`): the look's one accent, the text's,
/// or the faint one, as the mark says.
fn mark_colour(m: &coord_types::look::Mark, k: &Kit) -> rune::Color {
    use coord_types::look::Colour;
    match m.colour {
        Colour::Attention => k.c.danger.icon,
        Colour::Text => k.c.text_base,
        Colour::Faint => k.c.text_faint,
    }
}

/// What the circle says of where A compacts, for its hover and its panel: where its session compacts now and, while
/// the point kept differs, how that point reaches it (`context.next_by`, N-804.p16).
pub fn hover(a: &AgentView) -> Option<String> {
    let c = a.context.as_ref()?;
    Some(match (c.window.filter(|w| *w > 0), c.next) {
        (Some(w), Some(n)) if n != w => match c.next_by.as_deref() {
            Some("typing") => format!("compacts at {} when idle; {} until then", tokens(n), tokens(w)),
            Some("restart") => format!("compacts at {}; {} after a restart", tokens(w), tokens(n)),
            _ => format!("compacts at {}; {} from next start", tokens(w), tokens(n)),
        },
        (Some(w), _) => format!("compacts at {}", tokens(w)),
        (None, Some(n)) => format!("compacts at {} from its next start", tokens(n)),
        // a Codex agent with no point of its own compacts where Codex's default for its model says, a number its
        // session does not give: no mark, and a drag on the ring sets one (the owner, N-541.p28)
        (None, None) if a.kind == "codex" => "compacts by Codex's own default".to_string(),
        // an opencode agent compacts where opencode itself decides (its `compaction` config); coordd sets no point
        (None, None) if a.kind == "opencode" => "compacts by opencode's own default".to_string(),
        // an agy agent, where agy itself decides (agy's own session, or a t3code thread on an account t3code signed in)
        (None, None) if a.kind == "agy" => "compacts by agy's own default".to_string(),
        (None, None) => return None,
    })
}

/// The key of A's circle in project PI, which its panel hangs under.
fn ring_key(pi: usize, name: &str) -> String {
    format!("context-circle-{pi}-{name}")
}

/// The circle in the chat's header, the control itself, with nothing beside it. The line is dragged around the ring,
/// the pointer's angle from the circle's middle giving the tokens (it may leave the circle: further out is finer);
/// while dragged the number shows at the pointer ([`layer`]). On release the point is sent (`Request::Compact`). A
/// press that does not move is a click: it opens the context under the circle.
pub fn control(s: &App, pi: usize, a: &AgentView, k: &Kit) -> Option<El> {
    let c = a.context.as_ref()?;
    let (used, kept) = parts(a)?;
    let all = whole(a);
    let dragged = s.circle.on.as_ref().filter(|(p, n, _)| *p == pi && *n == a.name).map(|(_, _, t)| *t);
    let shown = dragged.map(|t| t as f32 / all as f32).or(kept);
    let name = a.name.clone();
    // the window's one look, as its colours are (look.rs `of`; N-866.p6, 6), not this project's own
    let look = crate::look::of(s).map(|l| l.context.clone()).unwrap_or_default();
    let opens = look.click == coord_types::look::Opens::Context;
    let me = (pi, a.name.clone());
    let held = dragged.is_some() || s.circle.hot.as_ref() == Some(&me);
    let hover = move |s: &mut App, inside: bool, _: &mut rune::Tx| {
        if inside {
            s.circle.hot = Some(me.clone());
        } else if s.circle.hot.as_ref() == Some(&me) {
            s.circle.hot = None;
        }
    };
    let ring = drawn(used, shown, SIZE, &look.mark, held, k).key(ring_key(pi, &a.name)).on_hover_change(hover).on_drag(move |s: &mut App, d: &mut rune::Drag, _| {
        let Some(a) = s.projects.get(pi).and_then(|p| p.live.agents.iter().find(|a| a.name == name)) else { return };
        let (all, now) = (whole(a), point(a));
        let at = tokens_at(d.x, d.y, d.w, d.h, all);
        match d.phase {
            rune::Phase::Down => (s.circle.on, s.circle.from, s.circle.pointer) = (None, Some((d.x, d.y)), None),
            rune::Phase::Move => {
                let moved = s.circle.from.is_none_or(|(x, y)| (d.x - x).hypot(d.y - y) > SLACK);
                if moved || s.circle.on.is_some() {
                    s.circle.on = Some((pi, name.clone(), at));
                    s.circle.pointer = Some(d.window_point());
                }
            }
            rune::Phase::Up => {
                let was = s.circle.on.take().is_some();
                (s.circle.from, s.circle.pointer) = (None, None);
                if d.cancelled() {
                    return;
                }
                if !was && opens {
                    // a click: the context, under the circle
                    let me = Some((pi, name.clone()));
                    s.circle.open = if s.circle.open == me { None } else { me };
                } else if Some(at) != now {
                    send(s, pi, &name, at);
                }
            }
        }
    });
    Some(ring.help(figures(a, c, all)))
}

/// The two figures, for the hover and the panel: "243k of 1M used · compacts at 350k".
pub(crate) fn figures(a: &AgentView, c: &coord_types::AgentContext, all: u64) -> String {
    let used = format!("{} of {} used", tokens(c.used), tokens(all));
    match hover(a) {
        Some(h) => format!("{used} · {h}"),
        None => used,
    }
}

/// The circle's overlays, into LAYER: the number at the pointer while the line is dragged, and the context panel
/// under the circle while opened (a click elsewhere or Escape closes it).
pub fn layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if let (Some((_, _, t)), Some([x, y])) = (&s.circle.on, s.circle.pointer) {
        let tip = row().px(8.0).py(3.0).radius(k.radius.sm).fill(k.c.surface_raised).border(1.0, k.c.border_weak).child(text(tokens(*t)).font(k.ty.small).color(k.c.text_base));
        layer.push(tip.key("context-circle-number").pin(Some(x + 14.0), Some(y - 28.0), None, None));
    }
    let Some((pi, name)) = &s.circle.open else { return };
    let Some(a) = s.projects.get(*pi).and_then(|p| p.live.agents.iter().find(|a| a.name == *name)) else { return };
    let Some(c) = a.context.as_ref() else { return };
    let mut body = col()
        .key("context-panel")
        .padding(14.0)
        .gap(10.0)
        .w(420.0)
        .child(text("Context").font(k.ty.ui_medium).color(k.c.text_strong))
        .child(text(figures(a, c, whole(a))).font(k.ty.small).color(k.c.text_base).wrap());
    if !c.parts.is_empty() {
        body = body.child(parts_bar(&c.parts, k));
    }
    let panel = rune_kit::popover::raised(k, body, k.radius.md).modal_tx(true, |s: &mut App, _| s.circle.open = None);
    layer.push(panel.anchored(ring_key(*pi, name), rune::Side::Bottom, rune::Align::Start, 6.0));
}

/// The parts of a context in plain words (`AgentContext.parts`, about).
fn kind_words(kind: &str) -> &str {
    match kind {
        "start" => "its start: system prompt, tools, CLAUDE.md, memory",
        "messages" => "messages to it",
        "summary" => "the summary of before its compaction",
        "words" => "its own words",
        "thinking" => "thinking",
        "tool_calls" => "its tool calls",
        "tool_results" => "tool results",
        other => other,
    }
}

/// A part as the legend says it: "tool results 131k".
fn legend_words(p: &coord_types::ContextPart) -> String {
    format!("{} {}", kind_words(&p.kind), tokens(p.tokens))
}

/// One stacked bar of the context by kind, largest first, each part's name and number in the legend and on its hover,
/// "about" beside it (N-804.p7: the owner's "a breakdown of the context by type").
fn parts_bar(parts: &[coord_types::ContextPart], k: &Kit) -> El {
    let all: u64 = parts.iter().map(|p| p.tokens).sum::<u64>().max(1);
    let mut sorted: Vec<&coord_types::ContextPart> = parts.iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.tokens));
    // one ink, a step lighter for each smaller part: no colour of its own (an accent says he is needed)
    let steps = [1.0, 0.72, 0.52, 0.38, 0.27, 0.18, 0.12];
    let width = 340.0;
    let mut bar = row().w(width).h(8.0).clip_rounded(4.0);
    let mut legend = row().gap(12.0).flex_wrap().w(392.0);
    for (i, p) in sorted.iter().enumerate() {
        let (ink, opacity) = (k.c.text_base, steps[i.min(steps.len() - 1)]);
        let words = format!("{} · about {}", kind_words(&p.kind), tokens(p.tokens));
        bar = bar.child(rune::el().h_full().w(width * p.tokens as f32 / all as f32).fill(ink).opacity(opacity).help(words.clone()));
        legend = legend.child(
            // its number in the legend, so shades one cannot tell apart need no hover (N-804.p11)
            row()
                .gap(4.0)
                .center_y()
                .child(rune::el().size(8.0, 8.0).radius(4.0).fill(ink).opacity(opacity))
                .child(text(legend_words(p)).font(k.ty.small).color(k.c.text_muted))
                .help(words),
        );
    }
    col().gap(6.0).child(row().gap(8.0).center_y().child(bar).child(text("about").font(k.ty.small).color(k.c.text_faint))).child(legend)
}

/// Send agent NAME's new point to its coordd, if that coordd keeps one. Its answer is not shown beside the circle (the
/// owner, N-804.p16): a refusal shows under the session's settings, as effort's does; the view's point is the news.
fn send(s: &mut App, pi: usize, name: &str, tokens: u64) {
    if s.projects.get(pi).is_some_and(|p| p.protocol < coord_types::COMPACT_POINT_PROTOCOL) {
        s.streams.said.insert((pi, name.to_string()), "Needs a newer engine".into());
        return;
    }
    s.streams.said.remove(&(pi, name.to_string()));
    s.request_in(pi, coord_types::Request::Compact { name: name.to_string(), tokens }, crate::Pending::Compact { agent: name.to_string() });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The project's look draws the mark (N-866): the built-in look is the red line; a look that checks changes it as
    /// soon as it arrives, with no new window; one that does not check leaves what was drawn and is listed with the
    /// refusals; a look taken away gives the built-in back.
    #[test]
    fn the_mark_is_drawn_as_the_projects_look_says_and_a_look_that_does_not_check_changes_nothing() {
        use serde_json::json;
        // a look bumps version::LOOK, which every memo's version holds: not while a view_cost test counts builds
        let _one = crate::view_cost::one();
        let mut live = crate::live::Live::default();
        let body = |live: &crate::live::Live| mark(0.35, &live.look.context.mark).map(|i| i.body);
        assert!(body(&live).is_some_and(|b| b.starts_with("<path") && b.contains(r#"stroke-width="10.0""#)), "built in: the line");
        live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(json!({"context": {"mark": {"shape": "dot", "width": 12}}})) });
        assert!(body(&live).is_some_and(|b| b.starts_with("<circle") && b.contains(r#"r="6.0""#)), "the look's dot");
        live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Look(json!({"context": {"mark": {"shape": "star"}}})) });
        assert!(body(&live).is_some_and(|b| b.starts_with("<circle")), "a look that does not check leaves the dot");
        assert!(live.refused().any(|r| r.contains("not applied") && r.contains("`context.mark.shape`")), "{:?}", live.look_refused);
        // a look says how a thing is drawn, never whether it is there (N-866.p6, 1)
        live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Look(json!({"context": {"mark": {"shape": "none"}, "click": {"opens": "nothing"}}})) });
        assert!(body(&live).is_some_and(|b| b.starts_with("<circle")) && live.refused().count() == 1, "{:?}", live.look_refused);
        live.apply(coord_types::Event { seq: 4, body: coord_types::Body::Look(serde_json::Value::Null) });
        assert_eq!(live.look, coord_types::look::Look::default(), "no look: the built-in");
    }

    /// A look that comes on the link as `Body::Look` draws the mark in its colour in the next frame, with no new window
    /// (N-1104 p113, head 1): the accent the look gives, and the text's colour when its mark says `text`.
    #[test]
    fn a_look_that_arrives_draws_the_mark_in_its_colour() {
        use serde_json::json;
        let _one = crate::view_cost::one();
        let mut live = crate::live::Live::default();
        let kit = |live: &crate::live::Live| {
            let mut k = Kit::neutral(true);
            crate::look::apply(&mut k, &live.look);
            k
        };
        let hex = |h: &str| rune::color::hex(h).unwrap();
        let built = mark_colour(&live.look.context.mark, &kit(&live));
        let palette = json!({"page": "#101114", "read": "#e8e8ea", "support": "#a0a1a6", "quiet": "#6c6d72", "layer": "#1a1b1f", "border": "#2a2b30", "attention": "#336699"});
        live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(json!({"colour": {"dark": palette}})) });
        assert_eq!(mark_colour(&live.look.context.mark, &kit(&live)), hex("#336699"), "the look's accent");
        assert_ne!(built, hex("#336699"), "and not the built-in one");
        live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Look(json!({"colour": {"dark": palette}, "context": {"mark": {"colour": "text"}}})) });
        assert_eq!(mark_colour(&live.look.context.mark, &kit(&live)), hex("#e8e8ea"), "the text's");
    }

    #[test]
    fn the_circle_is_the_whole_window_with_the_point_on_it() {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "", "state": "", "since": "",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        assert_eq!(parts(&a), None);
        a.context = Some(coord_types::AgentContext { used: 143_400, window: Some(350_000), ..Default::default() });
        let (used, at) = parts(&a).unwrap();
        assert!((used - 0.1434).abs() < 1e-4 && (at.unwrap() - 0.35).abs() < 1e-6);
        assert_eq!((tokens(143_400), tokens(1_000_000), tokens(350_000)), ("143k".into(), "1M".into(), "350k".into()));
        assert_eq!(tokens(1_250_000), "1.2M");
        // the pointer's angle from the top, clockwise, in steps, between the bounds
        assert_eq!(tokens_at(100.0, 50.0, 100.0, 100.0, WHOLE), 250_000, "right: a quarter");
        assert_eq!(tokens_at(50.0, 100.0, 100.0, 100.0, WHOLE), 500_000, "bottom: a half");
        assert_eq!(tokens_at(0.0, 50.0, 100.0, 100.0, WHOLE), 750_000, "left");
        assert_eq!(tokens_at(51.0, 0.0, 100.0, 100.0, WHOLE), LEAST, "just right of the top: the least");
        assert_eq!(tokens_at(49.9, 0.0, 100.0, 100.0, WHOLE), WHOLE, "just left of the top: the whole");
        a.context = Some(coord_types::AgentContext { used: 143_400, window: Some(350_000), next: Some(600_000), ..Default::default() });
        assert_eq!(point(&a), Some(600_000), "the mark shows the point kept for its next start");
        assert_eq!(hover(&a).as_deref(), Some("compacts at 350k; 600k from next start"));
        // how the kept point reaches the running session (N-804.p16)
        a.context = Some(coord_types::AgentContext { used: 143_400, window: Some(350_000), next: Some(600_000), next_by: Some("typing".into()), ..Default::default() });
        assert_eq!(point(&a), Some(600_000), "the line stands at the new place meanwhile");
        assert_eq!(hover(&a).as_deref(), Some("compacts at 600k when idle; 350k until then"));
        a.context = Some(coord_types::AgentContext { used: 143_400, window: Some(350_000), next: Some(600_000), next_by: Some("restart".into()), ..Default::default() });
        assert_eq!(hover(&a).as_deref(), Some("compacts at 350k; 600k after a restart"));
        let c = a.context.clone().unwrap();
        assert_eq!(figures(&a, &c, WHOLE), "143k of 1M used · compacts at 350k; 600k after a restart");
        a.context = Some(coord_types::AgentContext { used: 90_000, max: Some(272_000), ..Default::default() });
        assert_eq!(hover(&a), None);
        a.kind = "codex".into();
        assert_eq!(hover(&a).as_deref(), Some("compacts by Codex's own default"));
        a.kind = "opencode".into();
        assert_eq!(hover(&a).as_deref(), Some("compacts by opencode's own default"));
    }

    #[test]
    fn the_legend_says_each_parts_number() {
        assert_eq!(legend_words(&coord_types::ContextPart { kind: "tool_results".into(), tokens: 131_000 }), "tool results 131k");
    }

    /// A made-up agent with its context and a thread on it, the lock of whole-App tests held.
    fn scene() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "AGT-004", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "claude-fable-5-2:high",
            "state": "idle", "since": "2026-11-19T07:30:00Z", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        let part = |kind: &str, tokens: u64| coord_types::ContextPart { kind: kind.into(), tokens };
        let parts = vec![part("tool_results", 131_000), part("start", 31_400), part("words", 38_000), part("tool_calls", 22_000), part("messages", 14_000), part("thinking", 7_000)];
        a.context = Some(coord_types::AgentContext { used: 243_400, window: Some(350_000), max: Some(1_000_000), next: None, parts, next_by: None });
        p.live.agents = vec![a];
        let at = |m: u32| format!("2026-11-19T07:{m:02}:00Z");
        let v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-541", "n": 541, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "With you empty", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": [
            {"n": 1, "at": at(4), "by": "owner", "text": "the red bar for the compaction mark and being able to drag it", "names": ["paper-research"]},
            {"n": 2, "at": at(9), "by": "paper-research", "text": "The circle in the header is the control: drag the red mark on it.", "told": ["owner"]}]})).unwrap();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agent("paper-research".into());
        (held, s)
    }

    /// The owner's N-2003 (2): the red line answers alone, with no box, handle or grab hand over the control. A
    /// little smaller at rest; over the circle and while dragged a little longer, wider and brighter; back down on
    /// release.
    #[test]
    fn the_red_line_alone_answers_a_hover_and_a_drag_and_comes_back_down() {
        let (_held, s) = scene();
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let key = ring_key(0, "paper-research");
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).map(|e| e.rect).expect("the circle");
        let shown = |rt: &rune::runtime::Runtime<App>| -> Vec<String> {
            rt.elements().iter().filter(|e| !e.exiting).filter_map(|e| e.key.clone()).filter(|k| k.starts_with("context-mark")).collect()
        };
        assert_eq!(shown(&rt), ["context-mark"], "at rest: the line alone");
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert_eq!((shown(&rt), rt.cursor()), (vec!["context-mark-held".to_string()], rune::Cursor::Default), "over it: the line held, no handle, no grab hand");
        rt.pointer_down(x, y);
        rt.pointer_move(x + 12.0, y - 12.0);
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert_eq!((shown(&rt), rt.cursor(), rt.state.circle.on.is_some()), (vec!["context-mark-held".to_string()], rune::Cursor::Default, true), "dragged");
        rt.pointer_up(x + 12.0, y - 12.0);
        rt.pointer_move(5.0, 5.0);
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert_eq!((shown(&rt), rt.state.circle.hot.is_none()), (vec!["context-mark".to_string()], true), "released and away: back down");
        // the circle keeps no fill of its own, held or not
        assert!(rt.elements().iter().filter(|e| e.key.as_deref() == Some(key.as_str())).all(|e| e.fill.a == 0.0), "no box behind it");
    }

    /// The line's sizes: smaller than the look's at rest, larger held, its outer end never past the ring's edge.
    #[test]
    fn the_line_is_smaller_at_rest_and_larger_held_than_the_look_gives_it() {
        let look = coord_types::look::Look::default().context.mark;
        let (rest, held) = (sized(&look, false), sized(&look, true));
        let long = |m: &coord_types::look::Mark| (m.to - m.from).abs();
        assert!(rest.width < look.width && held.width > look.width, "{rest:?} {held:?}");
        assert!(long(&rest) < long(&look) && long(&held) > long(&look), "{rest:?} {held:?}");
        assert!([rest.from, rest.to, held.from, held.to].iter().all(|r| (0.0..=50.0).contains(r)), "inside the ring's box");
    }

    /// The stills of the header's circle (the owner, N-541 posts 32 and 35; N-804.p16): `STILL_OUT=DIR cargo test
    /// circle_stills -- --ignored`. Made-up agent and thread: at rest with the red line and nothing beside it; the line
    /// while it is dragged to 600k, the number at the pointer; the context panel opened by a click; the working line
    /// while its context is compacted; the session's settings.
    #[test]
    #[ignore]
    fn circle_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> App {
            scene().1
        }
        fn dragged() -> App {
            let mut s = init();
            s.circle.on = Some((0, "paper-research".into(), 600_000));
            let at: Vec<f32> = std::env::var("STILL_POINTER").unwrap_or_else(|_| "700,60".into()).split(',').filter_map(|v| v.parse().ok()).collect();
            s.circle.pointer = Some([at[0], at[1]]);
            s
        }
        fn hovered() -> App {
            let mut s = init();
            s.circle.hot = Some((0, "paper-research".into()));
            s
        }
        fn opened() -> App {
            let mut s = init();
            s.circle.open = Some((0, "paper-research".into()));
            s
        }
        fn compacting() -> App {
            let mut s = init();
            let a = &mut s.projects[0].live.agents[0];
            a.state = "working".into();
            a.compacting = Some((chrono::Utc::now() - chrono::Duration::seconds(84)).to_rfc3339());
            s
        }
        // the session's settings opened, an effort picked and not applied: Apply says when it takes effect
        fn session() -> App {
            let mut s = init();
            s.streams.open.insert("session|0|paper-research".into());
            s.streams.staged.insert((0, "paper-research".into()), (None, Some("xhigh".into())));
            s
        }
        // the same, nothing picked: Restart in Apply's place
        fn restart() -> App {
            let mut s = init();
            s.streams.open.insert("session|0|paper-research".into());
            s
        }
        let shots: [(&str, fn() -> App); 7] = [
            ("circle-rest.png", init),
            ("circle-hover.png", hovered),
            ("circle-dragged.png", dragged),
            ("circle-context.png", opened),
            ("circle-compacting.png", compacting),
            ("session-apply.png", session),
            ("session-restart.png", restart),
        ];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
