//! The agents' raw terminals (vault designs/yaaihh-2026-11-07.md, "Terminals: a holder per agent"). Each attached
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
    /// The cells he dragged over, and what they said then (N-753): copied as he saw it, highlighted only while the
    /// screen still shows it there.
    sel: Option<Selection>,
}

/// A drag over the full view's cells: from A to B (row, column; 0-based, on the screen as it showed then), and the text.
#[derive(Clone, Debug, PartialEq)]
struct Selection {
    a: (u16, u16),
    b: (u16, u16),
    text: String,
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
        sel: None,
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
        PtyEvent::Output { data, .. } => {
            term.parser.process(&data);
            crate::keystamp::taken(&name);
        }
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
            // the page shows it, or its Terminal drawer (drawer.rs)
            if s.term.pages.contains(&(p, name.clone())) || crate::drawer::is_open(s, p, name, crate::drawer::Drawer::Terminal) {
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
    // a row is built again only when its cells or its size changed (Rune's memo): new output moves a few rows, and
    // building every row of every terminal shown for each chunk (each run's text measured again) was most of a
    // working agent's cost to the window (11 Nov). A row's cells with their looks are its formatted bytes.
    let cells: Vec<String> = screen.rows_formatted(0, cols).collect();
    let row = |r: u16| {
        let mut v = crate::version::Version::default();
        v.add((cells.get(r as usize), cols, style.size.to_bits(), cw.to_bits(), ch.to_bits(), k.dark));
        rune::memo::memo_with_hash(format!("term-{project}-{agent}-{focused}-r{r}"), v.finish(), || row_view(screen, r, cols, style, (cw, ch), k, bg0))
    };
    let mut grid = el().absolute(PAD, PAD).size(cols as f32 * cw, rows as f32 * ch).children((0..rows).map(row).collect::<Vec<_>>());
    let back = screen.scrollback();
    if !screen.hide_cursor() && t.exited.is_none() && back == 0 {
        let (r, c) = screen.cursor_position();
        let ink = if focused { k.c.text_base.alpha(0.5) } else { k.c.text_base.alpha(0.25) };
        grid = grid.child(el().key("cursor").absolute(c as f32 * cw, r as f32 * ch).size(cw, ch).fill(ink));
    }
    // the selection, where the screen still shows what was selected
    if let Some(sel) = t.sel.as_ref().filter(|x| focused && selected_text(screen, x.a, x.b) == x.text) {
        let (a, b) = if sel.a <= sel.b { (sel.a, sel.b) } else { (sel.b, sel.a) };
        for r in a.0..=b.0 {
            let from = if r == a.0 { a.1 } else { 0 };
            let to = if r == b.0 { b.1 } else { cols - 1 };
            grid = grid.child(el().absolute(from as f32 * cw, r as f32 * ch).size((to + 1 - from) as f32 * cw, ch).fill(k.selection()));
        }
    }
    let mut v = frame.child(grid);
    let scrolled = (back > 0).then(|| format!("{back} lines back · Enter returns"));
    // a program on the alternate screen gets the wheel only as mouse reports, if it asked for them (wheel()); say so
    // when it did not, or the view looks stuck (8 Nov)
    let full = (screen.alternate_screen() && screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None)
        .then(|| "PgUp/PgDn scroll · Esc or Ctrl+O leaves".to_string());
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
    let drag_who = agent.to_string();
    let menu_who = agent.to_string();
    v.focusable(true)
        .focus_ring(2.0, 0.0, k.c.border_focus)
        .on_drag(move |s: &mut App, d, _| dragged(s, project, &drag_who, d, (cw, ch)))
        .on_secondary_press(move |s: &mut App, _, tx| right_click(s, project, &menu_who, tx))
        .on_key(move |s: &mut App, ev, tx| key(s, project, &who, ev, tx))
        .on_wheel(move |s: &mut App, e, _| !e.mods.ctrl && wheel(s, project, &wheel_who, e.dy, cell_at(e.x, e.y, cw, ch), ch))
}

/// The 0-based cell (row, column) under X, Y in the view's points, inside a screen of ROWS by COLS.
fn cell_of(x: f32, y: f32, cw: f32, ch: f32, rows: u16, cols: u16) -> (u16, u16) {
    let at = |p: f32, size: f32, n: u16| (((p - PAD) / size.max(1.0)).floor().max(0.0) as u16).min(n.saturating_sub(1));
    (at(y, ch, rows), at(x, cw, cols))
}

/// The text of SCREEN's cells from A to B (row, column; either way round), as it is copied: the cells of each line in
/// order, a line's trailing blanks dropped, a line the terminal wrapped joined to the next, the others ended by "\n".
fn selected_text(screen: &vt100::Screen, a: (u16, u16), b: (u16, u16)) -> String {
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    let cols = screen.size().1.get();
    let mut out = String::new();
    for r in a.0..=b.0 {
        let from = if r == a.0 { a.1 } else { 0 };
        let to = if r == b.0 { b.1 } else { cols - 1 };
        let mut line = String::new();
        for c in from..=to.min(cols - 1) {
            match screen.cell(r, c) {
                Some(cell) if cell.is_wide_continuation() => {}
                Some(cell) if cell.has_contents() => line.push_str(cell.contents()),
                _ => line.push(' '),
            }
        }
        let wrapped = r < b.0 && screen.row_wrapped(r);
        if wrapped {
            out.push_str(&line);
        } else {
            out.push_str(line.trim_end());
            if r < b.0 {
                out.push('\n');
            }
        }
    }
    out
}

/// His drag over the full view (D in the view's points): the cells from the press to the pointer selected; a press
/// that does not move clears the selection.
fn dragged(s: &mut App, project: usize, agent: &str, d: &rune::Drag, (cw, ch): (f32, f32)) {
    let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) else { return };
    let screen = t.parser.screen();
    let (rows, cols) = screen.size();
    let (rows, cols) = (rows.get(), cols.get());
    let a = cell_of(d.start_x, d.start_y, cw, ch, rows, cols);
    let b = cell_of(d.x, d.y, cw, ch, rows, cols);
    let moved = d.dx.abs() > cw / 2.0 || d.dy.abs() > ch / 2.0;
    t.sel = match d.phase {
        rune::Phase::Down => None,
        _ if !moved => None,
        _ => Some(Selection { a, b, text: selected_text(screen, a, b) }),
    };
}

/// The selection's text, while there is one and the screen still shows it: what Ctrl+Shift+C and a right click copy.
/// A selection whose highlight went (the screen moved on) is gone: nothing he no longer sees marked is copied
/// (consult-4, N-589.p26; paper-research p28).
fn selection(s: &App, project: usize, agent: &str) -> Option<String> {
    let t = s.term.open.get(&(project, agent.to_string()))?;
    let x = t.sel.as_ref()?;
    (!x.text.is_empty() && selected_text(t.parser.screen(), x.a, x.b) == x.text).then(|| x.text.clone())
}

/// A right click on the full view: the selection copied (and let go), else the clipboard pasted.
fn right_click(s: &mut App, project: usize, agent: &str, tx: &mut Tx) {
    match selection(s, project, agent) {
        Some(text) => {
            tx.copy(text);
            if let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) {
                t.sel = None;
            }
        }
        None => {
            let who = agent.to_string();
            tx.read_clipboard(move |s: &mut App, content, _| paste(s, project, &who, content.text()));
        }
    }
}

/// The 1-based cell (column, row) under a pointer at X, Y in the view's points, for mouse reports.
fn cell_at(x: f32, y: f32, cw: f32, ch: f32) -> (u16, u16) {
    let at = |p: f32, size: f32| (((p - PAD) / size.max(1.0)).floor().max(0.0) as u16).saturating_add(1);
    (at(x, cw), at(y, ch))
}

/// The wheel over the full view: back through the history; or, while the program has the alternate screen, wheel
/// reports at cell AT if it turned mouse reporting on (Claude Code's fullscreen renderer, htop), and nothing if it did
/// not. Never Up and Down keys: in Claude Code's prompt those recall earlier prompts, and the owner's wheel resent them
/// (10 Nov). DY points, positive moving the content down (back in time); CH a line's height.
fn wheel(s: &mut App, project: usize, agent: &str, dy: f32, at: (u16, u16), ch: f32) -> bool {
    let Some(t) = s.term.open.get_mut(&(project, agent.to_string())) else { return false };
    t.wheel += dy / ch.max(1.0);
    let lines = t.wheel.trunc();
    if lines == 0.0 {
        return true;
    }
    t.wheel -= lines;
    let screen = t.parser.screen_mut();
    if screen.alternate_screen() {
        if screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
            return true;
        }
        let button: u16 = if lines > 0.0 { 64 } else { 65 };
        let (col, row) = at;
        let one = match screen.mouse_protocol_encoding() {
            vt100::MouseProtocolEncoding::Sgr => format!("\x1b[<{button};{col};{row}M").into_bytes(),
            // the X10 form: each value plus 32 in one byte (coordinates past 223 are clamped there)
            _ => vec![0x1b, b'[', b'M', (32 + button) as u8, (32 + col.min(223)) as u8, (32 + row.min(223)) as u8],
        };
        let data = one.repeat(lines.abs() as usize);
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

/// A key while the full view has focus: as an xterm sends it, a paste (Ctrl+Shift+V, ⌘V), or a copy of the selection
/// (Ctrl+Shift+C, ⌘C): the copy chord is never sent on, selection or not (as Ctrl+C it interrupted the agent: N-753).
fn key(s: &mut App, project: usize, agent: &str, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if is_copy(ev) {
        if let Some(text) = selection(s, project, agent) {
            tx.copy(text);
        }
        return true;
    }
    let Some(t) = s.term.open.get(&(project, agent.to_string())) else { return false };
    if matches!(ev.key, Key::Command('v')) && (ev.mods.shift || !ev.mods.ctrl) {
        let who = agent.to_string();
        tx.read_clipboard(move |s: &mut App, content, _| paste(s, project, &who, content.text()));
        return true;
    }
    let Some(bytes) = encode(ev, tx.key_base(), t.parser.screen().application_cursor()) else { return false };
    // typing while scrolled up keeps the view where he reads, so he can type and read at once ("cannto tyepe at the
    // smae time", N-788); the line sent (Enter) or an interrupt goes back to the end, to see what comes of it
    if returns_to_end(&bytes)
        && let Some(t) = s.term.open.get_mut(&(project, agent.to_string()))
    {
        t.parser.screen_mut().set_scrollback(0);
    }
    crate::keystamp::down(agent);
    let sent = ask(s, project, agent, Request::Keys { name: agent.into(), data: bytes });
    crate::keystamp::written(sent);
    true
}

/// Whether keys BYTES bring a view scrolled up back to the end: Return (a line sent) or Ctrl+C (an interrupt); any
/// other typing leaves it where he reads.
fn returns_to_end(bytes: &[u8]) -> bool {
    bytes.iter().any(|b| matches!(b, b'\r' | 3))
}

/// Ctrl+Shift+C, or ⌘C: the copy chord (plain Ctrl+C stays the terminal's interrupt).
fn is_copy(ev: &KeyEvent) -> bool {
    matches!(ev.key, Key::Command('c')) && (ev.mods.shift || !ev.mods.ctrl)
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

#[cfg(test)]
mod before_snapshot_tests {
    /// The bridge answers an attach before coordd's first snapshot (a2h-speed's a2h-bridge-wait, N-518.p29): its screen
    /// and output reach a project that holds no snapshot yet, and the terminal shows them.
    #[test]
    fn a_terminals_screen_is_shown_before_the_first_snapshot() {
        let (_held, mut s) = crate::view_cost::test_app();
        let cfg = crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false };
        s.projects = vec![crate::Proj::new(&cfg, Vec::new())];
        s.cur = 0;
        assert!(s.projects[0].live.tickets.is_empty() && s.projects[0].live.agents.is_empty(), "no snapshot yet");
        super::attach(&mut s, 0, "a");
        let screen = coord_types::PtyEvent::Screen { name: "a".into(), rows: 4, cols: 20, data: b"hello".to_vec() };
        let output = coord_types::PtyEvent::Output { name: "a".into(), data: b" there".to_vec() };
        s.projects[0].inbox.extend([crate::link::Incoming::Pty(screen), crate::link::Incoming::Pty(output)]);
        crate::take_terminals(&mut s);
        let t = &s.term.open[&(0, "a".to_string())];
        assert!(t.parser.screen().contents().starts_with("hello there"), "{:?}", t.parser.screen().contents());
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;

    fn screen(rows: u16, cols: u16, bytes: &str) -> vt100::Parser {
        let mut p = vt100::Parser::new(nz(rows), nz(cols), 100);
        p.process(bytes.as_bytes());
        p
    }

    /// What a drag copies (N-753): one long command the terminal wrapped comes out as one line, two output lines as
    /// two, a line's trailing blanks dropped, the same whichever way he dragged.
    #[test]
    fn a_selection_copies_wrapped_lines_as_one_and_drops_trailing_blanks() {
        // 10 columns: "cargo test --all" wraps after "cargo test"
        let p = screen(4, 10, "cargo test --all\r\nok   \r\ndone");
        let s = p.screen();
        assert!(s.row_wrapped(0) && !s.row_wrapped(1));
        assert_eq!(selected_text(s, (0, 0), (3, 3)), "cargo test --all\nok\ndone");
        // backwards gives the same
        assert_eq!(selected_text(s, (3, 3), (0, 0)), "cargo test --all\nok\ndone");
        // from the middle of a line to the middle of another
        assert_eq!(selected_text(s, (0, 6), (2, 1)), "test --all\nok");
        // one line's blanks alone
        assert_eq!(selected_text(s, (2, 2), (2, 9)), "");
    }

    /// The cell under the pointer: past the padding, inside the screen even when dragged outside it.
    #[test]
    fn the_cell_under_the_pointer_is_kept_inside_the_screen() {
        assert_eq!(cell_of(PAD, PAD, 8.0, 16.0, 24, 80), (0, 0));
        assert_eq!(cell_of(PAD + 8.0 * 3.5, PAD + 16.0 * 2.2, 8.0, 16.0, 24, 80), (2, 3));
        assert_eq!(cell_of(-50.0, 5000.0, 8.0, 16.0, 24, 80), (23, 0));
        assert_eq!(cell_of(9000.0, -3.0, 8.0, 16.0, 24, 80), (0, 79));
    }

    /// Ctrl+Shift+C is never sent to the agent as Ctrl+C (it interrupts it), with a selection or without one; Ctrl+C
    /// alone still is.
    #[test]
    fn ctrl_shift_c_copies_and_never_interrupts_the_agent() {
        let (_held, mut s) = crate::view_cost::test_app();
        let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        s.projects = vec![p];
        s.cur = 0;
        let mut t = Term { parser: screen(4, 10, "hello\r\nworld"), exited: None, note: None, want: None, fit: [0.0, 0.0], live: true, wheel: 0.0, sel: None };
        t.sel = Some(Selection { a: (0, 0), b: (1, 4), text: selected_text(t.parser.screen(), (0, 0), (1, 4)) });
        s.term.open.insert((0, "a".into()), t);
        assert_eq!(selection(&s, 0, "a").as_deref(), Some("hello\nworld"));
        // the screen moves on: the highlight goes, and so does what would be copied
        s.term.open.get_mut(&(0, "a".into())).unwrap().parser.process(b"\x1b[2J\x1b[Hother");
        assert_eq!(selection(&s, 0, "a"), None);
        s.term.open.get_mut(&(0, "a".into())).unwrap().sel = Some(Selection { a: (0, 0), b: (1, 4), text: "hello\nworld".into() });
        s.term.open.get_mut(&(0, "a".into())).unwrap().parser = screen(4, 10, "hello\r\nworld");
        let shift = rune::Mods { ctrl: true, cmd: true, shift: true, ..Default::default() };
        let ev = KeyEvent { key: Key::Command('c'), mods: shift };
        let mut tx = Tx::default();
        assert!(key(&mut s, 0, "a", &ev, &mut tx), "taken, not sent on");
        // without a selection too
        s.term.open.get_mut(&(0, "a".into())).unwrap().sel = None;
        assert!(key(&mut s, 0, "a", &ev, &mut tx));
        // plain Ctrl+C still goes to the terminal as 0x03
        let ctrl = rune::Mods { ctrl: true, cmd: true, ..Default::default() };
        assert_eq!(encode(&KeyEvent { key: Key::Command('c'), mods: ctrl }, None, false), Some(vec![3]));
    }

    /// Typing while scrolled up leaves the view where he reads; Enter, the line sent, goes back to the end (N-788).
    #[test]
    fn typing_while_scrolled_up_stays_and_enter_returns_to_the_end() {
        let (_held, mut s) = crate::view_cost::test_app();
        let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "w".into(), id: "w".into(), boxed: false }, Vec::new());
        s.projects = vec![p];
        s.cur = 0;
        let mut t = Term { parser: screen(3, 10, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7"), exited: None, note: None, want: None, fit: [0.0, 0.0], live: true, wheel: 0.0, sel: None };
        t.parser.screen_mut().set_scrollback(2);
        assert_eq!(t.parser.screen().scrollback(), 2);
        s.term.open.insert((0, "a".into()), t);
        let back = |s: &App| s.term.open[&(0, "a".to_string())].parser.screen().scrollback();
        let mut tx = Tx::default();
        let none = rune::Mods::default();
        for (name, ev) in [("a letter", KeyEvent { key: Key::Text("x".into()), mods: none }), ("Backspace", KeyEvent { key: Key::Backspace, mods: none }), ("Left", KeyEvent { key: Key::Left, mods: none })] {
            key(&mut s, 0, "a", &ev, &mut tx);
            assert_eq!(back(&s), 2, "{name} keeps the view");
        }
        key(&mut s, 0, "a", &KeyEvent { key: Key::Enter, mods: none }, &mut tx);
        assert_eq!(back(&s), 0, "Enter returns to the end");
        assert!(returns_to_end(&[3]) && !returns_to_end(b"abc\x7f"));
    }
}

#[cfg(test)]
mod copy_key_tests {
    use super::*;

    /// Ctrl+Shift+C reaches no byte of the terminal's input (as Ctrl+C, 0x03, it interrupted the agent: N-753); plain
    /// Ctrl+C still does.
    #[test]
    fn ctrl_shift_c_never_reaches_the_terminal() {
        let shift = rune::Mods { ctrl: true, cmd: true, shift: true, ..Default::default() };
        let ctrl = rune::Mods { ctrl: true, cmd: true, ..Default::default() };
        let mac = rune::Mods { cmd: true, logo: true, ..Default::default() };
        assert!(is_copy(&KeyEvent { key: Key::Command('c'), mods: shift }));
        assert!(is_copy(&KeyEvent { key: Key::Command('c'), mods: mac }));
        assert!(!is_copy(&KeyEvent { key: Key::Command('c'), mods: ctrl }));
        // key() takes the chord before anything is encoded or asked, terminal attached or not
        let (_held, mut s) = crate::view_cost::test_app();
        let mut tx = Tx::default();
        assert!(key(&mut s, 0, "a", &KeyEvent { key: Key::Command('c'), mods: shift }, &mut tx));
        assert_eq!(encode(&KeyEvent { key: Key::Command('c'), mods: ctrl }, None, false), Some(vec![3]));
    }
}
