//! The agents' raw terminals. Each attached
//! agent has its own emulator (vt100, as in its holder, `coord-pty`), fed by the bridge's `{"pty": …}` lines; the view
//! draws its cells at any size and, focused, takes keys and makes its own size the terminal's.
//!
//! For a page, a split pane or a grid tile, on project PROJECT:
//!
//!   term::attach(s, project, agent)      relay AGENT's terminal (again, after the pipe was down); idempotent
//!   term::view(s, k, project, agent, focused)
//!                                        its cells, filling the column it is put in: focused, at full size, taking
//!                                        keys (once clicked; on the agent page also while nothing else has focus)
//!                                        and sending its size in cells as the terminal's;
//!                                        else scaled down to fit, keys and size left alone
//!
//! The state is `App::term`; `App::take` feeds it (`take`, `refused`, `down`, `relink`), and `poll` keeps the agent page's own
//! terminal attached while the page shows it (`follow_page`).

use crate::{App, Pending, Sel};
use coord_types::{PtyEvent, Request};
use rune::{Color, El, Key, KeyEvent, Span, TextStyle, Tx, col, el, text};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU16;

/// Space around the cells, in points.
const PAD: f32 = 8.0;
/// Lines of history kept above the screen, for the wheel to scroll back through.
const HISTORY: usize = 5000;
/// A cell's height, in font sizes.
const LINE: f32 = 1.3;

/// Every attached terminal, by project and agent, and the agent page's choice.
#[derive(Default)]
pub struct Terms {
    open: HashMap<(usize, String), Term>,
    /// The agents whose page shows the terminal rather than the chat ("Chat | Terminal"), by project and agent.
    pub pages: HashSet<(usize, String)>,
    /// The grid's tiles switched to Terminal, by project and agent.
    pub tiles: HashSet<(usize, String)>,
    /// The terminals shown now (the agent page's, the grid's tiles), attached for being shown.
    shown: HashSet<(usize, String)>,
}

/// One attached terminal.
struct Term {
    parser: vt100::Parser,
    /// How the program ended, once it has.
    exited: Option<String>,
    /// Why there is nothing to show (the attach was refused).
    note: Option<String>,
    /// The size the full view wants, in cells: asked for when it changes and again with each new screen (a new
    /// attach), so the bridge's connection carries it for the holder's "latest".
    want: Option<(u16, u16)>,
    /// Room for the cells in the view's last layout, in points.
    fit: [f32; 2],
    /// Whether the bridge relays it now (not after the pipe went down, until it is attached again).
    live: bool,
    /// Wheel points not yet a whole line.
    wheel: f32,
}

fn nz(v: u16) -> NonZeroU16 {
    NonZeroU16::new(v).unwrap_or(NonZeroU16::MIN)
}

/// Ask the bridge of PROJECT; the reply is looked at only when it refuses (`refused`).
fn ask(s: &mut App, project: usize, agent: &str, req: Request) -> bool {
    let Some(p) = s.projects.get_mut(project) else { return false };
    match p.link.as_mut().and_then(|l| l.ask(&req)) {
        Some(id) => {
            p.pending.insert(id, Pending::Term { agent: agent.into() });
            true
        }
        None => false,
    }
}

pub fn attach(s: &mut App, project: usize, agent: &str) {
    let key = (project, agent.to_string());
    if s.term.open.get(&key).is_some_and(|t| t.live) {
        return;
    }
    let live = ask(s, project, agent, Request::Attach { name: agent.into(), rows: 0, cols: 0 });
    let t = s.term.open.entry(key).or_insert_with(|| Term {
        parser: vt100::Parser::default(),
        exited: None,
        note: None,
        want: None,
        fit: [0.0, 0.0],
        live,
        wheel: 0.0,
    });
    t.live = live;
}

fn detach_in(s: &mut App, project: usize, agent: &str) {
    if s.term.open.remove(&(project, agent.to_string())).is_some_and(|t| t.live) {
        ask(s, project, agent, Request::Detach { name: agent.into() });
    }
}

/// What the bridge of PROJECT relayed.
pub fn take(s: &mut App, project: usize, ev: PtyEvent) {
    let name = ev.name().to_string();
    let Some(term) = s.term.open.get_mut(&(project, name.clone())) else { return }; // detached meanwhile
    match ev {
        PtyEvent::Screen { rows, cols, data, .. } => {
            term.parser = vt100::Parser::new(nz(rows), nz(cols), HISTORY);
            term.parser.process(&data);
            (term.exited, term.note) = (None, None);
            if let Some((rows, cols)) = term.want {
                ask(s, project, &name, Request::Resize { name: name.clone(), rows, cols });
            }
        }
        PtyEvent::Output { data, .. } => term.parser.process(&data),
        PtyEvent::Size { rows, cols, .. } => term.parser.set_size(nz(rows), nz(cols)),
        PtyEvent::Exit { status, .. } => term.exited = Some(status),
    }
}

/// The bridge refused a request for AGENT (no terminal, say): shown in its view.
pub fn refused(t: &mut Terms, project: usize, agent: &str, why: String) {
    if let Some(term) = t.open.get_mut(&(project, agent.to_string())) {
        term.note = Some(why);
    }
}

/// PROJECT's pipe went down: its terminals are attached again with the next one.
pub fn down(t: &mut Terms, project: usize) {
    t.open.iter_mut().filter(|((p, _), _)| *p == project).for_each(|(_, term)| term.live = false);
}

/// PROJECT's pipe is up again: attach what was attached.
pub fn relink(s: &mut App, project: usize) {
    let names: Vec<String> = s.term.open.iter().filter(|((p, _), t)| *p == project && !t.live).map(|((_, n), _)| n.clone()).collect();
    for name in names {
        let live = ask(s, project, &name, Request::Attach { name: name.clone(), rows: 0, cols: 0 });
        if let Some(t) = s.term.open.get_mut(&(project, name)) {
            t.live = live;
        }
    }
}

/// The terminals on screen (the agent page's when it shows Terminal, the grid's tiles switched to it) are attached;
/// one no longer shown is detached. Whether that changed.
pub fn follow_page(s: &mut App) -> bool {
    let mut want: HashSet<(usize, String)> = match &s.sel {
        Sel::Agents if s.agents_tab == 0 => s.term.tiles.iter().filter(|(p, _)| *p == s.cur).cloned().collect(),
        _ => HashSet::new(),
    };
    // every pane on an agent whose page shows its terminal (the focused one and the split's others)
    let others = s.panes.iter().enumerate().filter(|(j, _)| *j != s.focus).map(|(_, (p, sel))| (*p, sel));
    for (p, sel) in std::iter::once((s.cur, &s.sel)).chain(others) {
        if let Sel::Agent(name) = sel {
            if s.term.pages.contains(&(p, name.clone())) {
                want.insert((p, name.clone()));
            }
        }
    }
    if want == s.term.shown {
        return false;
    }
    for (p, name) in s.term.shown.difference(&want).cloned().collect::<Vec<_>>() {
        detach_in(s, p, &name);
    }
    for (p, name) in want.difference(&s.term.shown).cloned().collect::<Vec<_>>() {
        attach(s, p, &name);
    }
    s.term.shown = want;
    true
}

/// The mono style at SIZE, its line a cell high.
fn sized(base: TextStyle, size: f32) -> TextStyle {
    TextStyle { size, line_height: size * LINE, ..base }
}

/// A cell's width and height in STYLE.
fn cell(style: TextStyle) -> (f32, f32) {
    (rune::text::advance("MMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMM", style) / 32.0, style.line_px())
}

/// The 16 colours of the ANSI palette, for a dark and a light page.
const DARK: [u32; 16] = [
    0x2e3436, 0xe0605a, 0x7fc35a, 0xd9b44a, 0x5c9ce6, 0xc678dd, 0x4fc1c9, 0xd3d7cf, 0x6c7176, 0xff7b72, 0xa5e075, 0xf0d36a,
    0x82b8ff, 0xe09cf0, 0x7ee0e8, 0xf5f7f2,
];
const LIGHT: [u32; 16] = [
    0x2e3436, 0xc4302b, 0x3a8a1f, 0x9a6b00, 0x2a63c4, 0x9c3fb5, 0x1d8a93, 0x8a8f8a, 0x555a5f, 0xe0453f, 0x4ea62c, 0xb88400,
    0x3f7be0, 0xb65ad0, 0x28a6b0, 0xb8bcb6,
];

fn rgb(r: u8, g: u8, b: u8) -> Color {
    let lin = |v: u8| rune::color::srgb_decode(v as f32 / 255.0);
    Color { r: lin(r), g: lin(g), b: lin(b), a: 1.0 }
}

/// A cell colour: the 16 of the palette, the 6×6×6 cube and the grey ramp of xterm's 256, or true colour; None is the
/// default.
fn colour(c: vt100::Color, dark: bool) -> Option<Color> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Rgb(r, g, b) => Some(rgb(r, g, b)),
        vt100::Color::Idx(i @ 0..16) => {
            let v = (if dark { DARK } else { LIGHT })[i as usize];
            Some(rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
        }
        vt100::Color::Idx(i @ 16..232) => {
            let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
            let i = i - 16;
            Some(rgb(level(i / 36), level(i / 6 % 6), level(i % 6)))
        }
        vt100::Color::Idx(i) => {
            let v = 8 + 10 * (i - 232);
            Some(rgb(v, v, v))
        }
    }
}

/// How a run of cells looks.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    fg: Color,
    bg: Option<Color>,
    bold: bool,
    italic: bool,
    underline: bool,
}

fn look(c: &vt100::Cell, k: &Kit, bg0: Color) -> Look {
    let mut fg = colour(c.fgcolor(), k.dark).unwrap_or(k.c.text_base);
    let mut bg = colour(c.bgcolor(), k.dark);
    if c.inverse() {
        (fg, bg) = (bg.unwrap_or(bg0), Some(fg));
    }
    if c.dim() {
        fg = fg.mix(bg.unwrap_or(bg0), 0.4);
    }
    Look { fg, bg, bold: c.bold(), italic: c.italic(), underline: c.underline() }
}

/// One row: runs of cells that look the same, each at its own column and fitted to its cells exactly (letter spacing
/// takes up what the font's advance misses, so a fallback glyph never shifts the columns after it).
fn row_view(screen: &vt100::Screen, row: u16, cols: u16, style: TextStyle, (cw, ch): (f32, f32), k: &Kit, bg0: Color) -> El {
    let mut runs: Vec<(u16, u16, String, Look)> = Vec::new(); // first column, cells, text, look
    for c in 0..cols {
        let Some(cell) = screen.cell(row, c) else { continue };
        if cell.is_wide_continuation() {
            continue;
        }
        let (ch_text, w) = (if cell.has_contents() { cell.contents() } else { " " }, if cell.is_wide() { 2 } else { 1 });
        let l = look(cell, k, bg0);
        match runs.last_mut() {
            Some((_, n, t, prev)) if *prev == l && !cell.is_wide() => {
                *n += w;
                t.push_str(ch_text);
            }
            _ => runs.push((c, w, ch_text.to_string(), l)),
        }
        if cell.is_wide() {
            runs.push((c + 2, 0, String::new(), l)); // a wide cell is a run of its own
        }
    }
    let els = runs.into_iter().filter(|(_, n, _, _)| *n > 0).filter_map(|(c, n, mut t, l)| {
        // blank cells at the end of a run show nothing unless they have a background or a line
        let blank = if l.bg.is_none() && !l.underline { t.len() - t.trim_end_matches(' ').len() } else { 0 };
        t.truncate(t.len() - blank);
        if t.is_empty() {
            return None;
        }
        let cells = (n as usize - blank) as f32;
        let mut st = TextStyle { weight: if l.bold { 700.0 } else { style.weight }, ..style };
        st.italic = l.italic;
        st.tracking += (cells * cw - rune::text::advance(&t, st)) / t.chars().count() as f32;
        let mut label = text(t.clone()).font(st).color(l.fg);
        if l.underline {
            label = label.spans(vec![Span::new(0, t.len()).underline(l.fg)]);
        }
        let mut run = el().absolute(c as f32 * cw, 0.0).size(n as f32 * cw, ch).child(label);
        if let Some(bg) = l.bg {
            run = run.fill(bg);
        }
        Some(run)
    });
    el().key(format!("r{row}")).absolute(0.0, row as f32 * ch).size(cols as f32 * cw, ch).children(els.collect::<Vec<_>>())
}

pub fn view(s: &App, k: &Kit, project: usize, agent: &str, focused: bool) -> El {
    let bg0 = k.c.bg_deep;
    // a terminal's cells land where they are at once: a scroll or new output moves text, it never springs there
    let frame = col().key(format!("term-{project}-{agent}")).w_full().grow(1.0).min_h(0.0).fill(bg0).radius(k.radius.sm).clip_rounded(k.radius.sm).instant_changes();
    let Some(t) = s.term.open.get(&(project, agent.to_string())) else {
        return frame.padding(PAD).child(text(format!("{agent}'s terminal is not attached")).font(k.ty.small).color(k.c.text_faint));
    };
    let screen = t.parser.screen();
    let (rows, cols) = screen.size();
    let (rows, cols) = (rows.get(), cols.get());
    let base = k.ty.mono;
    let (bw, bh) = cell(sized(base, base.size));
    // a tile shows the whole terminal, as small as its room makes it
    let scale = if focused || t.fit[0] <= 0.0 { 1.0 } else { (t.fit[0] / (cols as f32 * bw)).min(t.fit[1] / (rows as f32 * bh)).min(1.0) };
    let style = sized(base, base.size * scale);
    let (cw, ch) = cell(style);
    let mut grid = el().absolute(PAD, PAD).size(cols as f32 * cw, rows as f32 * ch).children((0..rows).map(|r| row_view(screen, r, cols, style, (cw, ch), k, bg0)).collect::<Vec<_>>());
    let back = screen.scrollback();
    if !screen.hide_cursor() && t.exited.is_none() && back == 0 {
        let (r, c) = screen.cursor_position();
        let ink = if focused { k.c.text_base.alpha(0.5) } else { k.c.text_base.alpha(0.25) };
        grid = grid.child(el().key("cursor").absolute(c as f32 * cw, r as f32 * ch).size(cw, ch).fill(ink));
    }
    let mut v = frame.child(grid);
    let scrolled = (back > 0).then(|| format!("{back} lines back · typing returns to the end"));
    // a program on the alternate screen gets the wheel as Up and Down (wheel()); say so, or it looks stuck
    let full = screen.alternate_screen().then(|| "a full-screen view: the wheel sends ↑ ↓ to it · in Claude Code, Esc or Ctrl+O leaves it".to_string());
    if let Some(why) = t.note.clone().or(t.exited.as_ref().map(|e| format!("ended: {e}"))).or(scrolled).or(full) {
        v = v.child(row_note(why, k));
    }
    let who = agent.to_string();
    v = v.on_layout(move |s: &mut App, r| fitted(s, project, &who, r, focused, sized(base, base.size)));
    if !focused {
        return v;
    }
    let who = agent.to_string();
    let wheel_who = agent.to_string();
    v.focusable(true)
        .focus_ring(2.0, 0.0, k.c.border_focus)
        .on_key(move |s: &mut App, ev, tx| key(s, project, &who, ev, tx))
        .on_wheel(move |s: &mut App, e, _| !e.mods.ctrl && wheel(s, project, &wheel_who, e.dy, ch))
}

/// The wheel over the full view: back through the history, or, while the program has the alternate screen (a
/// full-screen program: htop, an editor), Up and Down to it, as terminals do. DY points, positive moving the content
/// down (back in time); CH a line's height.
fn wheel(s: &mut App, project: usize, agent: &str, dy: f32, ch: f32) -> bool {
    let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) else { return false };
    t.wheel += dy / ch.max(1.0);
    let lines = t.wheel.trunc();
    if lines == 0.0 {
        return true;
    }
    t.wheel -= lines;
    let screen = t.parser.screen_mut();
    if screen.alternate_screen() {
        let up = if screen.application_cursor() { "\x1bOA" } else { "\x1b[A" };
        let down = if screen.application_cursor() { "\x1bOB" } else { "\x1b[B" };
        let data = (if lines > 0.0 { up } else { down }).repeat(lines.abs() as usize).into_bytes();
        ask(s, project, agent, Request::Keys { name: agent.into(), data });
        return true;
    }
    let back = (screen.scrollback() as f32 + lines).max(0.0) as usize;
    screen.set_scrollback(back);
    true
}

/// A note over the terminal's top right corner (why it shows nothing, or that its program ended).
fn row_note(why: String, k: &Kit) -> El {
    el().pin(None, Some(PAD), Some(PAD), None)
        .px(8.0)
        .py(3.0)
        .radius(k.radius.sm)
        .fill(k.c.surface_raised)
        .border(1.0, k.c.border_weak)
        .child(text(why).font(k.ty.small).color(k.c.text_muted))
}

/// The view was laid out at R: the room for the cells; and, focused, the terminal's size in cells at BASE.
fn fitted(s: &mut App, project: usize, agent: &str, r: [f32; 4], focused: bool, base: TextStyle) {
    let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) else { return };
    t.fit = [(r[2] - 2.0 * PAD).max(0.0), (r[3] - 2.0 * PAD).max(0.0)];
    if !focused {
        return;
    }
    let (cw, ch) = cell(base);
    let size = ((t.fit[1] / ch).floor().max(1.0) as u16, (t.fit[0] / cw).floor().max(1.0) as u16);
    if t.want == Some(size) {
        return;
    }
    t.want = Some(size);
    if t.live {
        ask(s, project, agent, Request::Resize { name: agent.into(), rows: size.0, cols: size.1 });
    }
}

/// A key while the full view has focus: as an xterm sends it, or a paste (Ctrl+Shift+V, ⌘V).
fn key(s: &mut App, project: usize, agent: &str, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(t) = s.term.open.get(&(project, agent.to_string())) else { return false };
    if matches!(ev.key, Key::Command('v')) && (ev.mods.shift || !ev.mods.ctrl) {
        let who = agent.to_string();
        tx.read_clipboard(move |s: &mut App, content, _| paste(s, project, &who, content.text()));
        return true;
    }
    let Some(bytes) = encode(ev, tx.key_base(), t.parser.screen().application_cursor()) else { return false };
    // typing returns to the end, as a terminal does
    if let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) {
        t.parser.screen_mut().set_scrollback(0);
    }
    ask(s, project, agent, Request::Keys { name: agent.into(), data: bytes });
    true
}

/// A key while nothing has focus, on the agent page that shows the terminal: the terminal's.
pub fn page_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    match s.sel.clone() {
        Sel::Agent(name) if s.term.pages.contains(&(s.cur, name.clone())) => key(s, s.cur, &name, ev, tx),
        _ => false,
    }
}

/// TEXT typed at once, inside bracketed paste when the program asked for it; newlines as Return.
fn paste(s: &mut App, project: usize, agent: &str, text: &str) {
    let Some(t) = s.term.open.get(&(project, agent.to_string())) else { return };
    let text = text.replace("\r\n", "\r").replace('\n', "\r");
    let data = if t.parser.screen().bracketed_paste() {
        // no ESC of its own inside, so the text cannot end the paste early
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
    } else {
        text.into_bytes()
    };
    ask(s, project, agent, Request::Keys { name: agent.into(), data });
}

/// The control character Ctrl+C types.
fn control(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        c @ 'a'..='z' => c as u8 - b'a' + 1,
        ' ' | '@' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '/' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// The bytes an xterm sends for EV (BASE: the key's character without modifiers; APP_CURSOR: the program asked for
/// application cursor keys). None leaves the key to yaaihh (a ⌘ chord, say).
fn encode(ev: &KeyEvent, base: Option<char>, app_cursor: bool) -> Option<Vec<u8>> {
    let m = ev.mods;
    // Alt sends ESC first (Meta); xterm's modifier parameter: 1 + Shift 1 + Alt 2 + Ctrl 4
    let meta = |b: Vec<u8>| if m.alt { [vec![0x1b], b].concat() } else { b };
    let param = 1 + m.shift as u8 + 2 * m.alt as u8 + 4 * m.ctrl as u8;
    let cursor = |fin: char| match (param, app_cursor) {
        (1, true) => format!("\x1bO{fin}"),
        (1, false) => format!("\x1b[{fin}"),
        _ => format!("\x1b[1;{param}{fin}"),
    };
    let tilde = |n: u8| if param == 1 { format!("\x1b[{n}~") } else { format!("\x1b[{n};{param}~") };
    let typed = |t: &str| if m.alt { base.map(String::from).unwrap_or_else(|| t.to_string()) } else { t.to_string() };
    Some(match &ev.key {
        Key::Text(t) if m.ctrl => meta(vec![control(t.chars().next()?)?]),
        Key::Text(t) => meta(typed(t).into_bytes()),
        Key::Command(c) if m.ctrl => meta(vec![control(*c)?]),
        Key::Command(_) => return None,
        Key::Enter => meta(b"\r".to_vec()),
        Key::Backspace => meta(vec![if m.ctrl { 0x08 } else { 0x7f }]),
        Key::Tab if m.shift => b"\x1b[Z".to_vec(),
        Key::Tab => meta(b"\t".to_vec()),
        Key::Escape => meta(vec![0x1b]),
        Key::Up => cursor('A').into_bytes(),
        Key::Down => cursor('B').into_bytes(),
        Key::Right => cursor('C').into_bytes(),
        Key::Left => cursor('D').into_bytes(),
        Key::Home => cursor('H').into_bytes(),
        Key::End => cursor('F').into_bytes(),
        Key::Insert => tilde(2).into_bytes(),
        Key::Delete => tilde(3).into_bytes(),
        Key::PageUp => tilde(5).into_bytes(),
        Key::PageDown => tilde(6).into_bytes(),
        Key::Function(n @ 1..=4) => {
            let fin = ['P', 'Q', 'R', 'S'][*n as usize - 1];
            (if param == 1 { format!("\x1bO{fin}") } else { format!("\x1b[1;{param}{fin}") }).into_bytes()
        }
        Key::Function(n @ 5..=12) => tilde([15, 17, 18, 19, 20, 21, 23, 24][*n as usize - 5]).into_bytes(),
        _ => return None,
    })
}
