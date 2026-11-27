//! A kind's program as a picture (paper-research N-96, the owner's order of 15 Nov 2026): a flow drawn from the
//! checker's parse of the kind (docket-machine's `syntax`), not from its text matched by hand. One drawing, two uses:
//! the definition is the picture with nothing lit (the Kinds page, beside the kind's text); the execution is the same
//! picture lit from a ticket's run tree (beside its thread): what is done, where it is now, the path taken, and the
//! values that came back on the arrows, a halt of slow mode on the box where it waits. The text is the source: nobody
//! edits by drawing.
//!
//! ```text
//!   dev                         a step: its worker, its call (a name the program bound shows its value once known)
//!   implement(part, …)
//!     │ commit f995bbe          the value that came back, beside the arrow
//!   ◇ match r                   a match: its arms side by side, a bus across them, a drop into each with its case
//!   ├──────────────┐
//!   ▼ Cannot(why)  ▼ Commit(c)  lit: the way taken solid, an arm not taken folded to "not taken" (a press opens it)
//!   raise NoPart   …
//!   ┌ iterate · up to 3 · round 2 of 3 ─┐   an iterate: a frame with an arrow back, its bound and its round
//!   ━ par: at once               a par: a bar, a drop into each lane side by side, a join under them
//! ```
//!
//! The wires are drawn on rune's canvas (picture/wire.rs), down a block's spine; dashed is what is to come.
//!
//! The run tree mirrors the parse statement for statement (docket-machine's `view::Tree` walks the same `Prog`), so
//! each statement is lit by the run node in its place: the same index in its block, of the same kind.

use crate::{App, Proj};
use coord_types::TicketView;
use docket_machine::syntax::{self, Binding, Every, Expr, Kind, Pat, Prog, When, Worker};
use docket_machine::view::RunNode;
use docket_machine::{Type, ViewHalt, ViewRan, ViewStatus};
use rune::{El, col, el, row, text};
use rune_kit::Kit;
use serde_json::Value;
use std::collections::HashSet;
use crate::threads::Page;
use crate::texts::words;

pub(crate) mod docket;
pub(crate) mod graph;
mod stages;
pub(crate) use docket::{mark as docket_mark, shown_kind};
pub(crate) use graph::measure;
pub use stages::{Labels, read_labels, reached, shown_key, shown_stage};
pub(crate) mod wire;

// ---- the parse -----------------------------------------------------------------------------------------------------

/// The name in an error about a type the parse does not know (`` `Part` is not a type ``, ``no type `Part` ``).
fn unknown_type(e: &str) -> Option<String> {
    if !(e.contains("is not a type") || e.contains("no type")) {
        return None;
    }
    let (_, rest) = e.split_once('`')?;
    let (name, _) = rest.split_once('`')?;
    Some(name.to_string())
}

/// PARSE run with the named types it asks for standing as Any: the project's own types (`type Part {…}`) are not in a
/// kind's text, and the picture needs only the program's shape.
fn with_named<T>(parse: impl Fn(&syntax::Named) -> Result<T, String>) -> Result<T, String> {
    let mut named = syntax::Named::new();
    loop {
        match parse(&named) {
            Ok(x) => return Ok(x),
            Err(e) => match unknown_type(&e) {
                Some(n) if !named.contains_key(&n) && named.len() < 64 => {
                    named.insert(n, Type::Any);
                }
                _ => return Err(e),
            },
        }
    }
}

/// A kind's text (as a ticket keeps it, or as the Kinds page reads it) parsed as the checker parses it.
pub fn parse_kind(text: &str) -> Result<Kind, String> {
    let start = text.lines().position(|l| l.starts_with("kind ")).unwrap_or(0);
    let body: String = text.lines().skip(start).collect::<Vec<_>>().join("\n");
    with_named(|n| syntax::parse_kind("kind", &body, start + 1, n))
}

/// The kind a ticket's program TEXT draws as: a kind's text; else, for a handler's run (`TicketView::ran`), whose
/// program is its `when`'s body as it ran (docket-machine `handler_program`), a kind named KIND (its event) with that
/// body as its pattern (0.3 W3: a run opened from its thread's picture, N-1446 p16).
pub fn program_kind(text: &str, kind: &str) -> Result<Kind, String> {
    let e = match parse_kind(text) {
        Ok(kd) => return Ok(kd),
        Err(e) => e,
    };
    if text.lines().any(|l| l.starts_with("kind ")) {
        return Err(e);
    }
    let pattern = syntax::parse_prog(text, 1).map_err(|_| e)?;
    Ok(Kind { name: kind.to_string(), file: String::new(), line: 1, letter: None, width: 0, form: Vec::new(), result: Type::Any, raises: Vec::new(), goals: Vec::new(), pattern, text: text.to_string(), base: None, handlers: Vec::new(), doc: Default::default(), drawn: None })
}

/// A kinds file: its kinds, and the bindings in it (`on`, `every`, `when D is o`), each parsed as the checker does.
pub fn parse_file(path: &str, text: &str) -> Result<(Vec<Kind>, Vec<Binding>), String> {
    let kinds = with_named(|n| syntax::parse_kinds(path, text, n))?;
    let bindings = syntax::parse_bindings(path, text)?.into_iter().filter_map(Result::ok).collect();
    Ok((kinds, bindings))
}

/// P's statements in order: a `<-` is its first part (bound to its name) then the rest, as the run tree has them.
fn flat(p: &Prog) -> Vec<(&Prog, Option<String>)> {
    fn go<'a>(p: &'a Prog, bind: Option<String>, out: &mut Vec<(&'a Prog, Option<String>)>) {
        match p {
            Prog::Bind { x, first, rest } => {
                let b = docket_machine::print::bound_of(x);
                go(first, (b != "_").then_some(b), out);
                go(rest, None, out);
            }
            _ => out.push((p, bind)),
        }
    }
    let mut out = Vec::new();
    go(p, None, &mut out);
    out
}

/// How much room an arm's body wants beside the others: its statements, a match's or a par's by its arms, 1 to 4.
fn weight(p: &Prog) -> f32 {
    fn n(p: &Prog) -> usize {
        flat(p)
            .iter()
            .map(|(s, _)| match s {
                Prog::Match { arms, .. } => arms.iter().map(|(_, b)| n(b)).sum::<usize>().max(1),
                Prog::Par { branches, .. } | Prog::Any { branches, .. } => branches.iter().map(n).sum::<usize>().max(1),
                _ => 1,
            })
            .sum()
    }
    n(p).clamp(1, 4) as f32
}

// ---- what lights it ------------------------------------------------------------------------------------------------

/// What the picture is drawn with: the kinds it may open (by name, their text), the tickets that light it, the opens
/// shown open, a text at a place read.
pub struct Pic<'a> {
    pub k: &'a Kit,
    pub p: Option<&'a Proj>,
    pub kinds: Vec<(String, String)>,
    pub tickets: Vec<&'a TicketView>,
    pub open: &'a HashSet<String>,
    /// What it marks of the program's live edits: what is being edited now, and when each kind and call last was
    /// (edit.rs, p294).
    pub marks: crate::edit::Marks,
    pub words: &'a dyn Fn(&Value) -> String,
    /// A step clicked opens its panel (edit.rs): on ticket N's page (Some), its once-panel; on the Kinds page (None),
    /// its call's template.
    pub pick: Option<u64>,
    /// The step whose panel is open, by key: drawn picked.
    pub picked: Option<&'a str>,
}

/// One ticket lighting its picture: its view, and the steps a halt of its waits at.
struct Lit<'a> {
    v: &'a TicketView,
    halted: Vec<&'a ViewHalt>,
}

/// Where a statement is drawn: its key's prefix, the names bound so far with their values, the exceptions a `try`
/// around it handles, the agents a `let agent` made.
#[derive(Clone, Default)]
struct At {
    key: String,
    env: crate::ran::Env,
    handled: Vec<String>,
    made: Vec<String>,
    /// Of those, the forks (N-145): each with the agent it is a copy of, as written.
    forks: Vec<(String, String)>,
    depth: usize,
    /// Inside a par's lane: narrow, so an open there shows its ticket and call without "a ticket of its own".
    in_lane: bool,
    /// Its kind has no handler on `hole`, its own or its base's (`Pic::minded`): a rest there is no minder's.
    unminded: bool,
    /// The kind whose text it is written in (a base's handler: the base's), whose lines a press lights (`TEXT`).
    kind: String,
}

/// The key in `pic_open` of the lines of a kind's text a press in its picture lit (the owner, N-1104 p558: every thing
/// drawn opens what it stands for): `docket-text:KIND:LINE`, and `:arm` after it for a case, whose arm it is.
pub(crate) const TEXT: &str = "docket-text:";

/// The lines a press lit in KIND's TEXT, first and last (1-based): the statement at LINE and the lines under it
/// indented deeper, a match's or a try's closing brace with them; for a case, from its pattern's line.
pub(crate) fn lit_lines(open: &HashSet<String>, kind: &str, text: &str) -> Option<(usize, usize)> {
    let rest = open.iter().find_map(|k| k.strip_prefix(TEXT)?.strip_prefix(kind)?.strip_prefix(':'))?;
    let (line, arm) = match rest.strip_suffix(":arm") {
        Some(l) => (l, true),
        None => (rest, false),
    };
    let line: usize = line.parse().ok()?;
    let ls: Vec<&str> = text.lines().collect();
    if line == 0 || line > ls.len() {
        return None;
    }
    let mut from = line;
    // a case whose body starts under it: its pattern's line ends with `=>`
    if arm && !ls[from - 1].contains("=>") {
        if let Some(j) = (1..from).rev().find(|&j| !ls[j - 1].trim().is_empty()).filter(|&j| ls[j - 1].trim_end().ends_with("=>")) {
            from = j;
        }
    }
    let indent = |l: &str| l.len() - l.trim_start().len();
    let base = indent(ls[from - 1]);
    let mut to = from;
    for (i, l) in ls.iter().enumerate().skip(from) {
        if l.trim().is_empty() {
            continue;
        }
        if indent(l) > base {
            to = i + 1;
            continue;
        }
        // a block's closing brace, `};` too where the statement binds its value (`a <- try { … } handle { … };`)
        if indent(l) == base && (l.trim() == "}" || l.trim() == "};") {
            to = i + 1;
        }
        break;
    }
    Some((from, to))
}

/// The lines lit in KIND's TEXT (`lit_lines`) as a span over them, for the text drawn as TEXT is.
pub(crate) fn lit_spans(open: &HashSet<String>, kind: &str, text: &str, k: &Kit) -> Vec<rune::Span> {
    let Some((a, b)) = lit_lines(open, kind, text) else { return Vec::new() };
    let (mut at, mut from, mut to) = (0, None, 0);
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 == a {
            from = Some(at);
        }
        if i + 1 == b {
            to = at + l.trim_end_matches('\n').len();
        }
        at += l.len();
    }
    let Some(from) = from else { return Vec::new() };
    let mut sp = rune::Span::new(from, to.min(text.trim_end().len()));
    sp.bg = Some(k.c.info.bg);
    sp.bg_pad = 2.0;
    vec![sp]
}

/// A press in a picture: the lines KEY names lit, and KIND's card open, its text beside its picture; pressed again,
/// none.
fn light(s: &mut App, key: &str, kind: &str) {
    let was = s.tickets.pic_open.remove(key);
    s.tickets.pic_open.retain(|k| !k.starts_with(TEXT));
    if !was {
        s.tickets.pic_open.insert(key.to_string());
        s.kinds_open.insert(kind.to_string());
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    /// The definition: nothing lit.
    Plain,
    Done,
    Now,
    ToCome,
    NotTaken,
}

fn state_of(n: Option<&RunNode>) -> State {
    match n.map(|n| n.state.as_str()) {
        None => State::Plain,
        Some("done") => State::Done,
        Some("now") => State::Now,
        Some("not_taken") => State::NotTaken,
        Some(_) => State::ToCome,
    }
}

/// How deep an open's picture opens inside another.
const DEEP: usize = 3;

impl<'a> Pic<'a> {
    /// Kind TEXT's picture, lit by V when given; KEY names it on the page (and its opens' toggles).
    pub fn kind(&self, text: &str, v: Option<&TicketView>, key: &str) -> El {
        self.kind_into(text, v, &[], key)
    }

    /// As `kind`, with INTO, the bindings that open it, on its edge as `file` draws them (the Kinds page's card, a
    /// ticket's page with the binding that opened it; 0.2 line 26).
    pub fn kind_into(&self, text: &str, v: Option<&TicketView>, into: &[Binding], key: &str) -> El {
        let k = self.k;
        let kind = match program_kind(text, v.map_or("", |v| v.kind.as_str())) {
            Ok(x) => x,
            Err(e) => return text_el(format!("Its program could not be drawn: {e}"), k).color(k.c.danger.fg),
        };
        let lit = v.map(|v| Lit { v, halted: v.halts.iter().filter(|h| !matches!(h, ViewHalt::Deliver { .. })).collect() });
        let into: Vec<&Binding> = into.iter().filter(|b| opens_kind(b, &kind.name)).collect();
        self.kind_of(&kind, &into, lit.as_ref(), key, 0)
    }

    /// A kinds file's picture (its kinds and the bindings into them), nothing lit: a draft not installed.
    pub fn file(&self, kinds: &[Kind], bindings: &[Binding], key: &str) -> El {
        let mut c = col().w_full().gap(28.0);
        for kd in kinds {
            let into: Vec<&Binding> = bindings.iter().filter(|b| opens_kind(b, &kd.name)).collect();
            c = c.child(self.kind_of(kd, &into, None, &format!("{key}-{}", kd.name), 0));
        }
        c
    }

    /// Bindings into kinds not drawn here: each an arrow from outside, the kind it opens named.
    pub fn bindings(&self, bs: &[Binding], key: &str) -> El {
        let k = self.k;
        let mut c = col().key(format!("pic-{key}")).w_full().gap(6.0).child(text("bindings in the file, into kinds not in it".to_string()).font(k.ty.small_medium).color(k.c.text_muted));
        for b in bs {
            c = c.child(self.binding_chip(b));
        }
        c
    }

    /// Binding B on a kind's edge: an arrow from outside, what starts it and what it opens; a clock with when it fires
    /// next and when it last did, and whose own program it is (the snapshot's clocks, 0.2 line 26). Quiet: the one
    /// accent is what waits on him, and a binding does not.
    fn binding_chip(&self, b: &Binding) -> El {
        let k = self.k;
        let mut r = row().gap(6.0).center_y().flex_wrap().child(text("⟶").font(k.ty.ui).color(k.c.text_muted)).child(chip(binding_words(b), k.c.text_base, k.c.layer_02, k));
        if let Some(c) = self.p.and_then(|p| p.live.clocks.iter().find(|c| c.binding == b.text)) {
            let mut said = clock_words(c);
            if let Some(by) = &c.by {
                said.insert(0, format!("{by}'s own"));
            }
            if !said.is_empty() {
                r = r.child(text(said.join(" · ")).font(k.ty.small).color(k.c.text_muted));
            }
        }
        r
    }

    /// Whether KD's rest is the minder's: a handler on `hole`, KD's own or one it takes from its base (its text among
    /// the Pic's KINDS), writes what follows there. The engine's own rule for the minder (docket-machine
    /// machine.rs:2998, `minder`); base.kind's `hole` is the minder's `shape`.
    fn minded(&self, kd: &Kind) -> bool {
        let hole = |k: &Kind| k.handlers.iter().any(|h| h.ev == "hole");
        hole(kd) || kd.base.as_ref().and_then(|b| self.kinds.iter().find(|(n, _)| n == b)).and_then(|(_, t)| parse_kind(t).ok()).is_some_and(|b| hole(&b))
    }

    fn kind_of(&self, kd: &Kind, into: &[&Binding], lit: Option<&Lit>, key: &str, depth: usize) -> El {
        let k = self.k;
        let mut c = col().key(format!("pic-{key}")).w_full().gap(6.0);
        let mut head = row().w_full().gap(8.0).center_y().child(text(format!("kind {}", kd.name)).font(k.ty.ui_medium).color(k.c.text_base));
        if let Some(v) = lit.map(|l| l.v) {
            let n = v.n;
            head = head.child(k.interactive(text(format!("{} · {}", v.id, ticket_state(v))).font(k.ty.small).color(k.c.text_muted).key(format!("pic-{key}-ticket"))).on_press(move |s: &mut App| crate::threads::show(s, n)));
        }
        // its head lights the whole kind in its text, its foot the `result` line (p223: no dead press)
        let mine = At { kind: kd.name.clone(), ..At::default() };
        let result_line = kd.text.lines().position(|l| l.trim_start().starts_with("result")).filter(|_| kd.text.starts_with("kind ")).map_or(kd.line, |i| kd.line + i);
        c = c.child(self.lights(head.key(format!("pic-{key}-kind")), kd.line, &mine, false, true));
        // the bindings that open it: an arrow from outside into the kind; on the Kinds page, pressed, its runs under the
        // board, as the board's start opens them (docket.rs `show_panel`, `start:KIND:I`)
        for (i, b) in into.iter().enumerate() {
            let chip = self.binding_chip(b);
            c = c.child(match self.pick {
                None if self.p.is_some() => {
                    let what = format!("start:{}:{i}", kd.name);
                    k.interactive(chip.key(format!("pic-{key}-start-{i}"))).on_press(move |s: &mut App| docket::show_panel(s, &what))
                }
                _ => chip,
            });
        }
        // its entries on the edge: what runs on a ticket event, one under another. Not a wrapping row: taffy sizes a
        // wrapping row by its content as one line (0.15.1 flexbox.rs:1240), so in a picture that scrolls, its entries
        // ran past it (grown from nothing) or never wrapped (from their own width)
        let unminded = !self.minded(kd);
        let at = At { key: key.to_string(), env: lit.map(|l| form_env(l.v)).unwrap_or_default(), depth, unminded, kind: kd.name.clone(), ..At::default() };
        if !kd.handlers.is_empty() {
            let mut e = col().w_full().gap(8.0);
            for w in &kd.handlers {
                e = e.child(self.entry(w, lit, &format!("{key}-when-{}", w.ev), &At { made: Vec::new(), ..at.clone() }));
            }
            c = c.child(e);
        }
        c = c.child(self.block(&kd.pattern, lit.map(|l| l.v.run.as_slice()), lit, &at));
        let result = if kd.raises.is_empty() { format!("ends with {}", kd.result) } else { format!("ends with {} · or raises {}", kd.result, kd.raises.join(", ")) };
        c.child(self.lights(row().w_full().child(text(result).font(k.ty.small).color(k.c.text_faint).wrap()).key(format!("pic-{key}-ends")), result_line, &mine, false, true))
    }

    /// A `when` of the kind: an entry on its picture's edge, its body under it. AROUND: where it is drawn (its depth, the
    /// agents a `let agent` around it made, a `during`'s watch inside them, whether its kind's rest is the minder's);
    /// its head lights the handler's lines in the text of the kind it was written in.
    fn entry(&self, w: &When, lit: Option<&Lit>, key: &str, around: &At) -> El {
        let k = self.k;
        let kind = if w.from.is_empty() { around.kind.clone() } else { w.from.clone() };
        let at = At { key: key.to_string(), depth: around.depth, made: around.made.clone(), unminded: around.unminded, kind, ..At::default() };
        let head = format!("⇢ when {}({}){}", w.ev, w.x, w.cond.as_ref().map(|c| format!(" where {}", docket_machine::print::expr(c))).unwrap_or_default());
        let head = self.lights(row().w_full().child(text(head).font(k.ty.small_medium).color(k.c.text_muted).wrap()).key(format!("pic-{key}-head")), w.line, &at, false, true);
        col()
            .key(format!("pic-{key}"))
            .gap(6.0)
            .padding(8.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_weak)
            .border_dash(wire::DASH[0], wire::DASH[1])
            .child(head)
            .child(self.block(&w.body, None, lit.filter(|_| false), &at))
    }

    /// Ticket V's handlers as rows (0.3 W3, his zoom from a thread into its program: N-1104 p593–p630; N-1446 p16):
    /// every `when` of its kind and those it takes from its base, as the engine takes them (check.rs `inherit`), each
    /// with the runs it made under V (`TicketView::ran`, docket-edge5's): how many and when last, the latest each a
    /// `label · id` that opens its program (docket.rs `open_run`), the ones before them behind a press. A handler that
    /// never ran is faint; with no run listed at all (a coordd before `ran`) the rows say nothing of runs. A run in no
    /// row (a delivery's desk run, a stop, a `when` since taken out) stands in a row of its event's. A row's head
    /// pressed draws its handler's body under it, as the whole program's entry does. None: V's program is no kind's
    /// (a run's own), or its kind has no handler and nothing ran.
    pub fn handler_rows(&self, v: &TicketView, key: &str) -> Option<El> {
        let k = self.k;
        let mut kd = parse_kind(&v.program).ok()?;
        let names: std::collections::BTreeMap<String, ()> = self.kinds.iter().map(|(n, _)| (n.clone(), ())).collect();
        let base = docket_machine::check::base_of(&kd, &names).filter(|b| *b != kd.name);
        if let Some(b) = base.and_then(|b| self.kinds.iter().find(|(n, _)| *n == b)).and_then(|(_, t)| parse_kind(t).ok()) {
            docket_machine::check::inherit(&mut kd, &b);
        }
        let ws = &kd.handlers;
        // a run's row: its `when`'s kind and event; of several `when`s on one event, the one at its line, else the
        // first (a base's lines are its file's and a kind's own its text's, so a line only tells those apart)
        let row_of = |r: &ViewRan| -> Option<usize> {
            let same: Vec<usize> = (0..ws.len()).filter(|&i| ws[i].ev == r.event && r.from.as_deref() == Some(ws[i].from.as_str())).collect();
            same.iter().copied().find(|&i| r.line == Some(ws[i].line as u32)).or_else(|| same.first().copied())
        };
        let mut runs: Vec<Vec<&ViewRan>> = vec![Vec::new(); ws.len()];
        let mut other: Vec<(String, Vec<&ViewRan>)> = Vec::new();
        for r in &v.ran {
            match row_of(r) {
                Some(i) => runs[i].push(r),
                None => match other.iter_mut().find(|(e, _)| *e == r.event) {
                    Some((_, l)) => l.push(r),
                    None => other.push((r.event.clone(), vec![r])),
                },
            }
        }
        if ws.is_empty() && other.is_empty() {
            return None;
        }
        let known = !v.ran.is_empty();
        let mut c = col().key(format!("pic-{key}-on")).w_full().gap(8.0).child(text("When something happens".to_string()).font(k.ty.small_medium).color(k.c.text_muted));
        for (i, w) in ws.iter().enumerate() {
            let from = if w.from.is_empty() { kd.name.clone() } else { w.from.clone() };
            let whose = (from != kd.name).then(|| format!("{from}'s"));
            let at = At { key: format!("{key}-on-{i}"), kind: from, ..At::default() };
            c = c.child(self.handler_row(&docket::event_words(&w.ev), whose, Some((w, &at)), &runs[i], known, &format!("{key}-on-{i}")));
        }
        for (j, (ev, rs)) in other.iter().enumerate() {
            let why = if rs.iter().all(|r| r.from.is_none()) { "run by the engine itself" } else { "no longer written in its kind" };
            c = c.child(self.handler_row(&docket::event_words(ev), Some(why.to_string()), None, rs, known, &format!("{key}-on-x{j}")));
        }
        Some(c)
    }

    /// One row of `handler_rows`: LEAD, its event in words; WHOSE, the kind it is taken from or why it stands apart;
    /// its `when` (W, drawn at AT) when it has one; RUNS, its runs oldest first; KNOWN: the view lists runs at all.
    fn handler_row(&self, lead: &str, whose: Option<String>, w: Option<(&When, &At)>, runs: &[&ViewRan], known: bool, key: &str) -> El {
        let k = self.k;
        const LATEST: usize = 5;
        let ran = !runs.is_empty();
        let mut head = row().w_full().gap(8.0).center_y().child(text(lead.to_string()).font(k.ty.small_medium).color(if ran || !known { k.c.text_base } else { k.c.text_faint }));
        if let Some(x) = whose {
            head = head.child(text(x).font(k.ty.small).color(k.c.text_faint));
        }
        head = head.child(row().grow(1.0));
        if known {
            let said = runs.last().map_or_else(|| "never ran".to_string(), |r| format!("ran {} · last {}", runs.len(), crate::when::at(&r.opened_at)));
            head = head.child(text(said).font(k.ty.small).color(if ran { k.c.text_muted } else { k.c.text_faint }).key(format!("pic-{key}-ran")));
        }
        let body = format!("{key}-body");
        let shown = self.open.contains(&body);
        let head = match w {
            Some(_) => k.interactive(head.key(format!("pic-{key}"))).on_press(move |s: &mut App| {
                if !s.tickets.pic_open.remove(&body) {
                    s.tickets.pic_open.insert(body.clone());
                }
            }),
            None => head.key(format!("pic-{key}")),
        };
        let mut c = col().w_full().gap(4.0).padding(8.0).radius(k.radius.md).border(1.0, k.c.border_weak).child(head);
        let all = format!("{key}-all");
        let from = if self.open.contains(&all) { 0 } else { runs.len().saturating_sub(LATEST) };
        if from > 0 {
            let before = text(format!("› the {from} before them")).font(k.ty.small).color(k.c.text_muted);
            c = c.child(k.interactive(row().child(before).key(format!("pic-{key}-all"))).on_press(move |s: &mut App| {
                s.tickets.pic_open.insert(all.clone());
            }));
        }
        for r in &runs[from..] {
            let label = r.post.map_or_else(|| crate::when::at(&r.opened_at), |n| format!("#{n}"));
            let mut line = row().w_full().gap(8.0).center_y().child(text(format!("{label} · {}", r.id)).font(k.ty.small).color(k.c.text_base));
            line = line.child(text(r.said.clone().unwrap_or_default()).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink());
            let when = if r.closed_at.is_none() { "running".to_string() } else if r.post.is_some() { crate::when::at(&r.opened_at) } else { String::new() };
            line = line.child(text(when).font(k.ty.small).color(k.c.text_faint));
            let id = r.id.clone();
            c = c.child(k.interactive(line.key(format!("pic-{key}-run-{}", r.id))).on_press(move |s: &mut App| docket::open_run(s, &id)));
        }
        if let Some((w, at)) = w.filter(|_| shown) {
            c = c.child(self.block(&w.body, None, None, at));
        }
        c
    }

    /// EL pressed: the lines of the statement at LINE (a case's arm: ARM) lit in the text of AT's kind (`TEXT`);
    /// HOVER: lit under the pointer too (a box; a frame's inside takes its own presses and is not lit whole).
    fn lights(&self, el: El, line: usize, at: &At, arm: bool, hover: bool) -> El {
        let key = format!("{TEXT}{}:{line}{}", at.kind, if arm { ":arm" } else { "" });
        let kind = at.kind.clone();
        // its lines being edited in place (p294): marked as the panel they are edited in is
        let el = if self.marks.editing(&at.kind, line) { el.border(2.0, self.k.c.border_focus) } else { el };
        let el = if hover { self.k.interactive(el) } else { el };
        el.on_press(move |s: &mut App| light(s, &key, &kind))
    }

    /// A block's statements down the page, an arrow between each two, the value that came back on it.
    fn block(&self, p: &Prog, run: Option<&[RunNode]>, lit: Option<&Lit>, at: &At) -> El {
        let mut c = col().w_full().gap(0.0);
        let mut at = at.clone();
        let items = flat(p);
        let n = items.len();
        for (i, (s, bind)) in items.into_iter().enumerate() {
            let (what, _) = docket_machine::print::node(s);
            let node = run.and_then(|r| r.get(i)).filter(|r| r.what == what);
            let key = format!("{}.{i}", at.key);
            let state = state_of(node);
            c = c.child(self.statement(s, node, lit, &At { key, ..at.clone() }));
            // what it bound, for the statements after it
            if let (Some(b), Some(x)) = (&bind, node.and_then(|n| n.value.clone())) {
                at.env.push((b.clone(), x, node.and_then(|n| n.ty.clone())));
            }
            if i + 1 < n {
                let value = node.filter(|_| matches!(what, "step" | "open")).and_then(|n| n.value.as_ref().map(|v| (v.clone(), n.ty.clone())));
                let next = run.and_then(|r| r.get(i + 1)).map(|r| r.state.as_str());
                let on = state == State::Done && matches!(next, Some("done" | "now"));
                // lit and not gone along yet (to come, or in an arm not taken): dashed
                c = c.child(self.arrow(value, on, run.is_some() && !on));
            }
        }
        c
    }

    /// The arrow from one statement to the next, a wire down the spine (wire.rs): lit when the path went along it,
    /// dashed when AHEAD (lit, and not gone along yet), the value that came back on it beside it.
    fn arrow(&self, value: Option<(Value, Option<Type>)>, on: bool, ahead: bool) -> El {
        let k = self.k;
        let ink = if on { k.c.success.fg } else { k.c.text_faint };
        let mut r = row().gap(4.0).center_y().child(wire::down(ink, ahead, 22.0));
        if let Some((v, t)) = value {
            let shown = value_words(&v, t.as_ref(), self.words);
            if !shown.is_empty() {
                r = r.child(chip(shown, k.c.success.fg, k.c.success.bg, k));
            }
        }
        r
    }

    fn statement(&self, s: &Prog, node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let el = match s {
            Prog::Step { w, call, args, inline, at: line, .. } => self.step(w, call, args, inline.as_ref().map(|(t, _)| t.as_str()), *line, node, lit, at),
            Prog::Open { kind, args, .. } => self.open(kind, args, node, at),
            Prog::Return { e, .. } => {
                let said = match node.and_then(|n| n.value.as_ref()) {
                    Some(v) if state == State::Done => format!("returns {}", value_words(v, None, self.words)),
                    _ => format!("return {}", shown_expr(e, &at.env, self.words)),
                };
                terminal(said, if state == State::Done { k.c.success.fg } else { k.c.text_base }, if state == State::Done { k.c.success.bg } else { k.c.layer_02 }, k)
            }
            Prog::Raise { exc, .. } => {
                let to = if at.handled.contains(exc) { format!("→ to its handler {exc}") } else { "→ out of the kind".to_string() };
                row().w_full().gap(8.0).center_y().flex_wrap().child(row().child(terminal(format!("raise {exc}"), k.c.danger.fg, k.c.danger.bg, k))).child(text(to).font(k.ty.small).color(k.c.danger.fg))
            }
            Prog::Hole { .. } => {
                // the minder writes what follows only where a `hole` handler is (consult-3, N-1396 p192): delivery's
                // rest waits for the next event its handlers, drawn above, take
                let words = if at.unminded { "rests here; each event is handled above" } else { "waits for the minder" };
                node_box(text_el(words.into(), k), state, false, k).border_dash(wire::DASH[0], wire::DASH[1])
            }
            Prog::Match { e, arms, guards, .. } => self.fork(e, arms, guards, node, lit, at),
            Prog::Par { branches, each, .. } => self.lanes("par", branches, each.as_ref(), node, lit, at),
            Prog::Any { n, branches, each, .. } => self.lanes(&format!("any {n} of"), branches, each.as_ref(), node, lit, at),
            Prog::Iterate { x, init, n, body, .. } => {
                // no `up to` (N-147): as many rounds as it takes, each round's steps bounded
                let head = if *n == docket_machine::syntax::UNBOUNDED {
                    format!("iterate {x} = {} · no count", shown_expr(init, &at.env, self.words))
                } else {
                    format!("iterate {x} = {} · up to {n}", shown_expr(init, &at.env, self.words))
                };
                self.round_frame(head, *n, body, node, lit, at)
            }
            Prog::For { x, e, n, body, .. } => {
                let head = format!("for {x} in {} · up to {n}", shown_expr(e, &at.env, self.words));
                self.each_frame(head, *n, body, node, lit, at)
            }
            Prog::Layer { on, body, .. } => {
                let head = match on {
                    // a string names a branch; anything else (commits, one per repository) is said as written
                    Some(e @ Expr::Str(_)) => format!("layer on {}: a workspace of its own, on top of that branch", shown_expr(e, &at.env, self.words)),
                    Some(e) => {
                        let on = shown_expr(e, &at.env, self.words);
                        format!("layer on {on}: a workspace of its own, on top of {on}")
                    }
                    None => "layer: a workspace of its own".to_string(),
                };
                self.frame(head, k.c.text_accent, body, node, lit, at, true)
            }
            Prog::Try { body, arms, .. } => {
                let mut inner = at.clone();
                inner.handled.extend(arms.iter().map(|h| h.exc.clone()));
                let run = node.map(|n| n.nodes.as_slice());
                // the body's statements, then one `handle` node per handler
                let nb = flat(body).len();
                let mut c = col()
                    .w_full()
                    .gap(6.0)
                    .padding(8.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_base)
                    .child(text("try".to_string()).font(k.ty.small_medium).color(k.c.text_muted))
                    .child(self.block(body, run.map(|r| &r[..nb.min(r.len())]), lit, &At { key: format!("{}.t", at.key), ..inner }));
                let mut exits = col().w_full().gap(6.0);
                let label = |h: &docket_machine::syntax::Handler| format!("⚠ {}{} exits here", h.exc, h.x.as_ref().map(|x| format!("({x})")).unwrap_or_default());
                // every label of one try as wide as the widest, so their bodies all go beside or all go under
                let label_w = arms.iter().map(|h| label(h).chars().count()).max().unwrap_or(0) as f32 * 7.5 + 20.0;
                for (i, h) in arms.iter().enumerate() {
                    let hn = run.and_then(|r| r.get(nb + i)).filter(|n| n.what == "handle");
                    let taken = hn.is_some_and(|n| n.state != "not_taken");
                    // its body beside the label, or under it when the row has no room (at any depth)
                    let arm = row()
                        .w_full()
                        .gap(10.0)
                        .flex_wrap()
                        .child(col().w(label_w).pt(2.0).child(row().child(chip(label(h), k.c.danger.fg, k.c.danger.bg, k))))
                        .child(col().grow(1.0).w(0.0).min_w(240.0).child(self.block(&h.body, hn.map(|n| n.nodes.as_slice()), lit, &At { key: format!("{}.h{i}", at.key), ..at.clone() })));
                    exits = exits.child(if lit.is_some() && !taken { arm.opacity(0.45) } else { arm });
                }
                c = c.child(exits);
                c
            }
            Prog::With { f, e, body, .. } => self.frame(format!("with {f} = {}", shown_expr(e, &at.env, self.words)), k.c.text_muted, body, node, lit, at, false),
            Prog::LetAgent { x, from, body, .. } => {
                // no frame of its own: the agent's name stands on the boxes it does
                let mut inner = at.clone();
                inner.made.push(x.clone());
                if let Some(a) = from {
                    inner.forks.push((x.clone(), docket_machine::print::expr(a)));
                }
                col()
                    .w_full()
                    .gap(4.0)
                    .child(
                        text(match from {
                            // a fork (N-101): a copy of that agent's conversation as it stood then
                            Some(a) => format!("let agent {x} = fork {}: a copy of that agent's conversation, for what follows", shown_expr(a, &at.env, self.words)),
                            None => format!("let agent {x}: a new agent, for what follows"),
                        })
                        .font(k.ty.small)
                        .color(k.c.text_muted),
                    )
                    .child(self.block(body, node.map(|n| n.nodes.as_slice()), lit, &inner))
            }
            Prog::Bind { .. } => col(),
            // a watch while its body runs: the body in a frame, the `when` a dashed entry on the frame's side, each
            // look in a hidden ticket of its own
            Prog::During { body, whens, .. } => {
                let frame = col()
                    .w_full()
                    .gap(6.0)
                    .padding(8.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_base)
                    .child(text("during: while this runs, a watch beside it".to_string()).font(k.ty.small_medium).color(k.c.text_muted).wrap())
                    .child(self.block(body, node.map(|n| n.nodes.as_slice()), lit, &At { key: format!("{}.d", at.key), ..at.clone() }));
                // a `when` an entry, in the order written (the first that fits takes the event)
                let mut out = col().w_full().gap(6.0).child(frame);
                for (n, when) in whens.iter().enumerate() {
                    let key = if n == 0 { format!("{}.during-when", at.key) } else { format!("{}.during-when-{n}", at.key) };
                    let entry = self.entry(when, lit, &key, at);
                    out = out.child(row().w_full().gap(8.0).child(text("⇠".to_string()).font(k.ty.ui).color(k.c.text_muted).pt(6.0)).child(col().grow(1.0).w(0.0).min_w(0.0).child(entry)));
                }
                out
            }
            // a decision: the policy answers, the owner when no rule does
            Prog::Decide { name, args, .. } => {
                let call = format!("decide {name}({})", args.iter().map(|a| shown_expr(a, &at.env, self.words)).collect::<Vec<_>>().join(", "));
                let mut body = col()
                    .w_full()
                    .gap(2.0)
                    .child(text("the policy, or you when no rule answers".to_string()).font(k.ty.small_medium).color(k.c.text_accent).wrap())
                    .child(text(call).font(k.ty.small).color(k.c.text_base).wrap());
                if let Some(v) = node.and_then(|n| n.value.as_ref()).filter(|_| state == State::Done) {
                    body = body.child(text(format!("decided {}", value_words(v, None, self.words))).font(k.ty.small).color(k.c.success.fg));
                }
                let halted = lit.is_some_and(|l| halts_here(l, node));
                node_box(body, state, halted, k)
            }
            // an agent's turn stopped, as esc stops it
            Prog::Stop { e, .. } => node_box(text_el(format!("stop {}: its turn is stopped", shown_expr(e, &at.env, self.words)), k), state, false, k),
            // the others in one box: what the statement says
            other => {
                let (_, t) = docket_machine::print::node(other);
                node_box(text_el(short(&t, 80), k), state, false, k)
            }
        };
        let el = el.key(format!("pic-{}", at.key));
        // a press lights its lines in the text beside it (p223): a box whole, a frame where nothing inside takes it
        let el = match s {
            Prog::Step { .. } | Prog::Bind { .. } => el,
            Prog::Match { .. } | Prog::Par { .. } | Prog::Any { .. } | Prog::Iterate { .. } | Prog::For { .. } | Prog::Layer { .. } | Prog::Try { .. } | Prog::With { .. } | Prog::LetAgent { .. } | Prog::During { .. } | Prog::Open { .. } => self.lights(el, s.at(), at, false, false),
            _ => self.lights(el, s.at(), at, false, true),
        };
        if state == State::NotTaken { el.opacity(0.45) } else { el }
    }

    #[allow(clippy::too_many_arguments)]
    fn step(&self, w: &Worker, call: &str, args: &[Expr], inline: Option<&str>, line: usize, node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let written = docket_machine::print::worker(w);
        let mut who = node.and_then(|n| n.who.clone()).filter(|_| state != State::Plain && state != State::ToCome && state != State::NotTaken).unwrap_or_else(|| written.clone());
        if let Worker::Named(x) | Worker::Expr(Expr::Var(x)) = w {
            if at.made.contains(x) && (who == *x) {
                who = match at.forks.iter().rev().find(|(f, _)| f == x) {
                    Some((_, of)) => format!("{x}, a fork of {of}"),
                    None => format!("{x}, the new agent"),
                };
            }
            // a worker named by the form: its value, once the ticket holds one
            if let Some((_, v, _)) = at.env.iter().rev().find(|(n, _, _)| n == x) {
                if let Some(name) = v.as_str().filter(|_| state != State::Plain) {
                    who = name.to_string();
                }
            }
        }
        // whom it goes to, as the Docket's right column names them: an agent, him, coordd's model call (the watcher), else
        // the agent holding the role
        let agent = if who == "owner" || self.p.is_some_and(|p| p.live.agents.iter().any(|a| a.name == who)) { who.clone() } else { crate::refs::who(self.p, &written).label };
        // who owes it, for its call's model line (kinds.rs `model_words`)
        let by = agent.clone();
        let who = worker_words(&who);
        // the call's name shows its declaration and doc under the pointer (docket-ls); an inline step's text is its own
        let what = match inline {
            Some(t) => text(quoted(t, 9)).font(k.ty.small).color(k.c.text_base).wrap(),
            None => {
                let rest = format!("({})", args.iter().map(|a| shown_expr(a, &at.env, self.words)).collect::<Vec<_>>().join(", "));
                row().w_full().min_w(0.0).flex_wrap().child(self.named(call, "call", format!("doc-{}", at.key))).child(text(rest).font(k.ty.small).color(k.c.text_base).wrap().shrink())
            }
        };
        let halted = lit.is_some_and(|l| halts_here(l, node));
        let mut body = col()
            .w_full()
            .gap(2.0)
            // pressed: the Agent scale, that agent picked: who it is now, what it owes, its threads (p223)
            .child(row().child(k.interactive(text(who.clone()).font(k.ty.small_medium).color(if state == State::Plain { k.c.text_accent } else { ink(state, k) }).key(format!("pic-{}-who", at.key))).on_press(move |s: &mut App| docket::open_agent(s, &agent))))
            .child(what);
        if let Some(n) = node.filter(|n| n.rounds.len() > 1) {
            body = body.child(text(format!("given {} times, once a round", n.rounds.len())).font(k.ty.small).color(k.c.text_faint));
        }
        // refused to its agent for its model (`model_refused`); held back unsent (the runner's command could not start)
        if let Some(r) = node.and_then(|n| n.refused.as_ref()) {
            body = body.child(chip(refused_words(r), k.c.danger.fg, k.c.danger.bg, k));
        }
        if let Some(u) = lit.and_then(|l| unsent_at(l.v, node)) {
            body = body.child(chip(unsent_words(u), k.c.warning.fg, k.c.warning.bg, k));
        }
        // the last reading of its agent's answer read none of its outcomes: a click opens the ticket's readings there
        if let Some((step, why)) = lit.and_then(|l| read_none_at(l.v, node)) {
            let words = if why.is_empty() { "its answer read as none of its outcomes ›".to_string() } else { format!("its answer read as none of its outcomes: {why} ›") };
            let (t, id) = (lit.map_or(0, |l| l.v.n), lit.map(|l| l.v.id.clone()).unwrap_or_default());
            body = body.child(k.interactive(chip(words, k.c.warning.fg, k.c.warning.bg, k)).on_press(move |s: &mut App| crate::ticket::open_reading(s, t, &id, step)));
        }
        // the step was passed up after its holder's answers were not read: each answer whole, with why
        if let Some(l) = lit.filter(|l| node.is_some_and(|n| n.state == "now")) {
            if let Some(p) = passed_el(l.v, self.words, k) {
                body = body.child(p);
            }
        }
        // the runner's own drawing: its strip of stations for the commit it checks
        if who.contains("runner") {
            if let (Some(p), Some(c)) = (self.p, args.iter().find_map(|a| commit_in(a, &at.env))) {
                if let Some(strip) = crate::domain::runner_strip(p, &c, k) {
                    body = body.child(col().w_full().pt(4.0).child(strip));
                }
            }
        }
        if halted {
            body = body.child(chip("halted: waits on you · its card is in the thread".into(), k.c.warning.fg, k.c.warning.bg, k));
        }
        // its call's live edits (p294): being edited now, or when it last was; an edit of the call coordd runs for it
        // only on the watcher's step, by its taker as `runs_for` reads it (consult-3 N-1446 p1231, p1233)
        let editing = self.marks.step_editing(call, Some(&by));
        if editing {
            body = body.child(text("editing".to_string()).font(k.ty.small).color(k.c.text_muted).key(format!("pic-{}-editing", at.key)));
        } else if let Some(when) = self.marks.step_edited(call, Some(&by)) {
            body = body.child(text(format!("edited {}", crate::when::at(when))).font(k.ty.small).color(k.c.text_faint).key(format!("pic-{}-edited", at.key)));
        }
        let mut b = node_box(body, state, halted, k);
        if self.picked == Some(at.key.as_str()) || editing {
            b = b.border(2.0, k.c.border_focus);
        }
        // clicked: its panel (edit.rs); on the Kinds page only a step with a call of its own (an inline step's prompt
        // is the kind's text)
        let pick = match self.pick {
            Some(t) => Some(crate::edit::Step { t, key: at.key.clone(), worker: who.clone(), call: call.to_string(), inline: inline.map(String::from), awaited: state == State::Now }),
            None => None,
        };
        if pick.is_none() && (self.pick.is_some() || inline.is_some()) {
            return self.lights(b, line, at, false, true);
        }
        let call = call.to_string();
        let (lit_key, kind) = (format!("{TEXT}{}:{line}", at.kind), at.kind.clone());
        k.interactive(b).on_press(move |s: &mut App| match &pick {
            Some(st) => {
                let same = s.tickets.edits.step.as_ref() == Some(st);
                s.tickets.edits.step = if same { None } else { Some(st.clone()) };
                s.tickets.edits.once = rune::TextEdit::new("");
                s.tickets.edits.said = None;
            }
            // its call, in the card its step is in (kinds.rs), its line lit in the text beside it
            None => docket::open_call(s, &kind, &call, &by, Some(&lit_key)),
        })
    }

    /// NAME, a CONTAINER's (`call`, `kind`), as drawn in a card: the pointer on it shows its declaration and doc
    /// (tickets.rs `doc_hover`, the card drawn by `doc_layer`), when the picture is of a project.
    fn named(&self, name: &str, container: &'static str, key: String) -> El {
        let k = self.k;
        let t = text(name.to_string()).font(k.ty.small).color(k.c.text_base);
        let Some(p) = self.p else { return t };
        let (pid, name) = (p.id.clone(), name.to_string());
        el().key(key.clone()).child(t).on_hover_change(move |s: &mut App, inside: bool, _: &mut rune::Tx| doc_hover(s, &pid, container, &name, &key, inside))
    }

    fn open(&self, kind: &str, args: &[Expr], node: Option<&RunNode>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let opened = node.and_then(|n| n.tickets.first().cloned());
        let key = format!("{}.open", at.key);
        let shown = self.open.contains(&key);
        let rest = format!("({})", args.iter().map(|a| shown_expr(a, &at.env, self.words)).collect::<Vec<_>>().join(", "));
        let head = row()
            .w_full()
            .min_w(0.0)
            .flex_wrap()
            .child(text("open ".to_string()).font(k.ty.small).color(k.c.text_base))
            .child(self.named(kind, "kind", format!("doc-{key}")))
            .child(text(rest).font(k.ty.small).color(k.c.text_base).wrap().shrink());
        // the ticket it opened beside "a ticket of its own", going under it in a narrow lane
        let mut label = row().w_full().gap(8.0).flex_wrap();
        if !at.in_lane {
            label = label.child(text("a ticket of its own".to_string()).font(k.ty.small_medium).color(ink(state, k)).wrap().shrink());
        }
        if let Some(t) = &opened {
            // pressed: that ticket (a question: its thread, where it is answered)
            let n = self.tickets.iter().find(|x| &x.id == t).map(|x| x.n).or_else(|| self.p.and_then(|p| p.live.tickets.values().find(|x| &x.id == t)).map(|x| x.n));
            let id = text(t.clone()).font(k.ty.small_medium).color(k.c.text_accent).key(format!("pic-{key}-ticket"));
            label = label.child(match n {
                Some(n) => k.interactive(id).on_press(move |s: &mut App| crate::threads::show(s, n)),
                None => id,
            });
        }
        let toggle = {
            let key = key.clone();
            k.interactive(row().w_full().min_w(0.0).child(text(format!("{} the {kind} inside", if shown { "⌄" } else { "›" })).font(k.ty.small).color(k.c.text_accent).wrap().shrink())).on_press(move |s: &mut App| {
                if !s.tickets.pic_open.remove(&key) {
                    s.tickets.pic_open.insert(key.clone());
                }
            })
        };
        let mut body = col().w_full().gap(4.0);
        if !at.in_lane || opened.is_some() {
            body = body.child(label);
        }
        body = body.child(head);
        let known = self.kinds.iter().find(|(n, _)| n == kind).map(|(_, t)| t.clone());
        if known.is_some() && at.depth + 1 < DEEP {
            body = body.child(toggle);
        }
        if shown && at.depth + 1 < DEEP {
            if let Some(t) = known {
                // the kind opened into: lit by the ticket it opened, when there is one
                let v = opened.as_ref().and_then(|id| self.tickets.iter().copied().find(|x| &x.id == id));
                let inner = match parse_kind(&t) {
                    Ok(kd) => {
                        let lit = v.map(|v| Lit { v, halted: v.halts.iter().filter(|h| !matches!(h, ViewHalt::Deliver { .. })).collect() });
                        self.kind_of(&kd, &[], lit.as_ref(), &format!("{key}.{kind}"), at.depth + 1)
                    }
                    Err(e) => text_el(e, k),
                };
                body = body.child(col().w_full().mt(6.0).padding(8.0).radius(k.radius.sm).fill(k.c.layer_02).child(inner));
            }
        }
        node_box(body, state, false, k).border(if state == State::Plain { 1.0 } else { 1.5 }, edge(state, false, k))
    }

    /// A match: a ◇ on the spine, a bus across its arms side by side, and a drop into each with its case written on it
    /// (wire.rs). Lit, the way it went is drawn solid; an arm not taken folds to one "not taken", which a press opens
    /// (drawn faint) and folds again.
    fn fork(&self, e: &Expr, arms: &[(Pat, Prog)], guards: &[Option<Expr>], node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let subject = docket_machine::print::expr(e);
        let ran: Vec<Option<&RunNode>> = (0..arms.len()).map(|i| node.and_then(|n| n.nodes.get(i)).filter(|n| n.what == "arm")).collect();
        let taken = ran.iter().position(|an| matches!(state_of(*an), State::Done | State::Now));
        // the bus up to arm I is on the way taken when the arm taken is I or one after it
        let way = |i: usize| if taken.is_some_and(|t| t >= i) { k.c.success.fg } else { k.c.text_faint };
        let words = text(match_words(e, &at.env, self.words)).font(k.ty.small_medium).color(k.c.text_muted).wrap().shrink();
        let head = row().w_full().gap(4.0).child(wire::diamond(if state == State::Plain { k.c.text_muted } else { ink(state, k) }, k.c.layer_02, way(0), 10.0)).child(words);
        let n = arms.len();
        let mut forks = row().w_full();
        for (i, (pat, body)) in arms.iter().enumerate() {
            let an = ran[i];
            let ast = state_of(an);
            let on = matches!(ast, State::Done | State::Now);
            let mut inner = at.clone();
            inner.key = format!("{}.a{i}", at.key);
            let label = docket_machine::print::pat(pat);
            if on {
                crate::ran::bind_arm(&mut inner.env, &subject, &label);
            }
            // its drop, the case written on it: solid on the way taken (its case a chip), dashed to come or not taken
            let drop = wire::down(if on { k.c.success.fg } else { k.c.text_faint }, an.is_some() && !on, 22.0).key(format!("pic-{}.a{i}-drop", at.key));
            let written = if on { chip(label, k.c.success.fg, k.c.success.bg, k) } else { text(label).font(k.ty.small).color(k.c.text_muted) };
            let mut case = row().gap(4.0).center_y().child(drop).child(written);
            // its `where` (N-152): the arm is taken only when the condition holds
            if let Some(g) = guards.get(i).and_then(Option::as_ref) {
                case = case.child(text(format!("where {}", docket_machine::print::expr(g))).font(k.ty.small).color(k.c.text_muted).wrap().shrink());
            }
            // pressed: its arm lit in the text beside it (p223)
            let case = self.lights(case.key(format!("pic-{}.a{i}-case", at.key)), body.at(), &inner, true, true);
            let mut c = col().w_full().min_w(0.0).gap(4.0).child(case);
            if i + 1 < n {
                c = c.pr(12.0);
            }
            let not_taken = lit.is_some() && ast == State::NotTaken;
            let shown = format!("{}.a{i}.shown", at.key);
            let folded = not_taken && !self.open.contains(&shown);
            if not_taken {
                // folded: one pill; open: the same pill, to fold it again, over the arm drawn faint
                let pill = row()
                    .key(format!("pic-{}.a{i}-fold", at.key))
                    .px(10.0)
                    .py(2.0)
                    .radius(12.0)
                    .border(1.0, k.c.border_weak)
                    .border_dash(wire::DASH[0], wire::DASH[1])
                    .child(text(if folded { "not taken ›" } else { "not taken ⌄" }.to_string()).font(k.ty.small).color(k.c.text_faint));
                c = c.child(row().pl(wire::SPINE - 10.0).child(k.interactive(pill).on_press(move |s: &mut App| {
                    if !s.tickets.pic_open.remove(&shown) {
                        s.tickets.pic_open.insert(shown.clone());
                    }
                })));
            }
            if !folded {
                let b = self.block(body, an.map(|n| n.nodes.as_slice()), lit, &inner);
                c = c.child(if not_taken { b.opacity(0.45) } else { b });
            }
            // side by side, sharing the row by what each holds (`weight`), and none narrower than what it cannot wrap
            // below: its drop and case (a case does not wrap) and the longest words of its body, a nested match's arms
            // side by side. That is the layout's own minimum, a flex item's min-content (taffy's automatic minimum
            // size), which holds while no width or minimum is set on the arm. A folded arm is as wide as its case.
            let arm = col().child(wire::bus(i, n, way(i), way(i + 1))).child(c);
            forks = forks.child(if folded { arm } else { arm.grow(weight(body)) });
        }
        col().w_full().child(head).child(forks)
    }

    /// A par or an any: lanes side by side; over a list one lane per element once it runs, one marked "per element"
    /// before. Its wires (wire.rs): a bar on the spine and a bus into the lanes, a drop into each, and a join bus under
    /// them that the arrow after the par goes on from.
    fn lanes(&self, what: &str, branches: &[Prog], each: Option<&syntax::Each>, node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let head = match each {
            // the list by its name and how many it holds, not spelled out
            Some(x) => {
                let many = match &x.e {
                    Expr::Var(v) => at.env.iter().rev().find(|(n, _, _)| n == v).and_then(|(_, val, _)| val.as_array()).map(|a| format!(" ({} elements)", a.len())),
                    _ => None,
                };
                match &x.order {
                    // parts by dependency (N-144): each part starts once those it stands on are done
                    Some(o) => format!(
                        "{what} {} in {}{} · up to {} · as {} after {}{}: each once those it stands on are done",
                        x.x,
                        docket_machine::print::expr(&x.e),
                        many.unwrap_or_default(),
                        x.n,
                        docket_machine::print::expr(&o.key),
                        docket_machine::print::expr(&o.after),
                        o.reach.as_ref().map_or(String::new(), |k| format!(" reach {k}"))
                    ),
                    None => format!("{what} {} in {}{} · up to {}: at once", x.x, docket_machine::print::expr(&x.e), many.unwrap_or_default(), x.n),
                }
            }
            None => format!("{what}: at once"),
        };
        let run: Vec<&RunNode> = node.map(|n| n.nodes.iter().filter(|b| b.what == "branch").collect()).unwrap_or_default();
        let mut lanes: Vec<El> = Vec::new();
        // the parts' tickets, lane by lane (the par's node lists them in its elements' order)
        let parts: &[String] = node.map(|n| n.tickets.as_slice()).unwrap_or_default();
        let lane = |i: usize, title: String, body: &Prog, bn: Option<&RunNode>| {
            let st = state_of(bn);
            let inner = At { key: format!("{}.b{i}", at.key), in_lane: true, ..at.clone() };
            let mut c = col()
                .key(format!("pic-{}.b{i}", at.key))
                .min_w(0.0)
                .gap(6.0)
                .padding(8.0)
                .radius(k.radius.md)
                .border(if st == State::Now { 2.0 } else { 1.0 }, edge(st, false, k))
                .child(text(title).font(k.ty.small_medium).color(ink(st, k)));
            if let Some(e) = self.stands_on(parts, i) {
                c = c.child(e);
            }
            (c.child(self.block(body, bn.map(|n| n.nodes.as_slice()), lit, &inner)), st, bn.is_some())
        };
        let mut boxes: Vec<(El, State, bool)> = Vec::new();
        match (each, run.is_empty()) {
            (Some(x), true) => boxes.push(lane(0, format!("per element: one lane for each {}", x.x), &branches[0], None)),
            (Some(_), false) => {
                for (i, bn) in run.iter().enumerate() {
                    boxes.push(lane(i, format!("lane {} · {}", i + 1, lane_state(bn)), &branches[0], Some(bn)));
                }
            }
            (None, _) => {
                for (i, b) in branches.iter().enumerate() {
                    let bn = run.get(i).copied();
                    let title = match bn {
                        Some(bn) => format!("lane {} · {}", i + 1, lane_state(bn)),
                        None => format!("lane {}", i + 1),
                    };
                    boxes.push(lane(i, title, b, bn));
                }
            }
        }
        // the split solid once the par is reached, the join once it is done; a lane's drop and its way out solid as
        // it runs and is done, dashed to come
        let (faint, path) = (k.c.text_faint, k.c.success.fg);
        let split = if matches!(state, State::Done | State::Now) { path } else { faint };
        let join = if state == State::Done { path } else { faint };
        let n = boxes.len();
        for (i, (b, st, ran)) in boxes.into_iter().enumerate() {
            let into = if matches!(st, State::Done | State::Now) { path } else { faint };
            let out = if st == State::Done { path } else { faint };
            let b = if i + 1 < n { b.mr(8.0) } else { b };
            // side by side, sharing the row evenly, and none narrower than what it cannot wrap below, as a match's arms
            lanes.push(
                col()
                    .grow(1.0)
                    .child(wire::bus(i, n, split, split))
                    .child(wire::down(into, ran && into == faint, 16.0).key(format!("pic-{}.b{i}-drop", at.key)))
                    // as tall as the tallest lane, so every way out starts at one height
                    .child(b.grow(1.0))
                    .child(wire::stem(out, ran && out == faint, 10.0))
                    .child(wire::bus(i, n, join, join)),
            );
        }
        let top = row().w_full().gap(4.0).child(wire::bar(split, 8.0)).child(text(head).font(k.ty.small_medium).color(k.c.text_muted).wrap().shrink());
        col().w_full().child(top).child(row().w_full().children(lanes))
    }

    /// The edges of held part I of a par in order (N-144): an arrow to each lane it stands on, by its number and ticket,
    /// while it is held (its ticket's status `holds`; once freed, that status is what it runs into).
    fn stands_on(&self, parts: &[String], i: usize) -> Option<El> {
        let k = self.k;
        let id = parts.get(i)?;
        let t = self.tickets.iter().find(|t| &t.id == id)?;
        let docket_machine::ViewStatus::Holds { on, reach } = &t.status else { return None };
        let to: Vec<String> = on.iter().map(|o| match parts.iter().position(|p| p == o) {
            Some(j) => format!("lane {} ({o})", j + 1),
            None => o.clone(),
        }).collect();
        // the head says when a part starts; the edge names the lanes, and the kind a `reach` waits for
        let what = match reach {
            Some(r) => format!("after {} (its latest {r})", to.join(", ")),
            None => format!("after {}", to.join(", ")),
        };
        Some(row().w_full().gap(6.0).child(text("⟵".to_string()).font(k.ty.small).color(k.c.warning.fg)).child(text(what).font(k.ty.small).color(k.c.warning.fg).wrap().grow(1.0).w(0.0)))
    }

    /// An iterate or a for: a frame with an arrow back up its side, its bound, and the round it is in once lit.
    fn round_frame(&self, head: String, n: u32, body: &Prog, node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let round = node.and_then(|x| first_step(&x.nodes)).map(|s| s.rounds.len()).filter(|r| *r > 0);
        let mut top = row().w_full().gap(8.0).center_y().child(text(head).font(k.ty.small_medium).color(k.c.text_muted).wrap().grow(1.0).w(0.0));
        if let Some(r) = round {
            top = top.child(chip(if n == docket_machine::syntax::UNBOUNDED { format!("round {r}") } else { format!("round {r} of {n}") }, k.c.info.fg, k.c.info.bg, k));
        }
        let back = if state == State::Plain || round.unwrap_or(0) < 2 { k.c.text_muted } else { k.c.info.fg };
        // the arrow back: up the frame's left side from its foot, into its top (wire.rs)
        let rail = wire::back(back);
        // past the first round the run tree still holds the round before after the point at work: drawn to come, and
        // what that round came back with put on the arrow back
        let (nodes, before) = match node {
            Some(x) if round.unwrap_or(0) >= 2 => {
                let mut ns = x.nodes.clone();
                let mut again = None;
                left_over(&mut ns, &mut false, &mut again);
                (Some(ns), again)
            }
            _ => (node.map(|x| x.nodes.clone()), None),
        };
        let rounds = if n == docket_machine::syntax::UNBOUNDED { "as many rounds as it takes".to_string() } else { format!("at most {n} rounds") };
        let foot = match (&before, round) {
            (Some(v), Some(r)) => format!("↺ Again: back to the top, {rounds} · round {} came back with {}", r - 1, value_words(v, None, self.words)),
            _ => format!("↺ Again: back to the top, {rounds}"),
        };
        let inner = col().grow(1.0).w(0.0).min_w(0.0).gap(6.0).child(top).child(self.block(body, nodes.as_deref(), lit, &At { key: format!("{}.i", at.key), ..at.clone() })).child(text(foot).font(k.ty.small).color(back).wrap());
        row().w_full().gap(6.0).padding(8.0).radius(k.radius.md).border(if state == State::Now { 1.5 } else { 1.0 }, edge(state, false, k)).child(rail).child(inner)
    }

    /// A for: its body once per element, one after another; no arrow back (it is not a retry), its bound, and the
    /// element it is at once lit.
    fn each_frame(&self, head: String, n: u32, body: &Prog, node: Option<&RunNode>, lit: Option<&Lit>, at: &At) -> El {
        let k = self.k;
        let state = state_of(node);
        let at_elem = node.and_then(|x| first_step(&x.nodes)).map(|s| s.rounds.len()).filter(|r| *r > 0);
        let mut top = row().w_full().gap(8.0).center_y().child(text(head).font(k.ty.small_medium).color(k.c.text_muted).wrap().grow(1.0).w(0.0));
        if let Some(r) = at_elem {
            top = top.child(chip(format!("element {r}"), k.c.info.fg, k.c.info.bg, k));
        }
        col()
            .w_full()
            .gap(6.0)
            .padding(8.0)
            .radius(k.radius.md)
            .border(if state == State::Now { 1.5 } else { 1.0 }, edge(state, false, k))
            .child(top)
            .child(self.block(body, node.map(|n| n.nodes.as_slice()), lit, &At { key: format!("{}.e", at.key), ..at.clone() }))
            .child(text(format!("↓ then the next element, one after another, at most {n}")).font(k.ty.small).color(k.c.text_muted))
    }

    #[allow(clippy::too_many_arguments)]
    fn frame(&self, head: String, color: rune::Color, body: &Prog, node: Option<&RunNode>, lit: Option<&Lit>, at: &At, dashed: bool) -> El {
        let k = self.k;
        let c = col()
            .w_full()
            .gap(6.0)
            .padding(8.0)
            .radius(k.radius.md)
            .border(1.5, color)
            .child(text(head).font(k.ty.small_medium).color(color).wrap())
            .child(self.block(body, node.map(|n| n.nodes.as_slice()), lit, &At { key: format!("{}.f", at.key), ..at.clone() }));
        if dashed { c.border_dash(wire::DASH[0], wire::DASH[1]) } else { c }
    }
}

// ---- small parts ---------------------------------------------------------------------------------------------------

/// The held-back step of V that NODE is: a round of it with the step's number, else the step owed now to its worker.
pub fn unsent_at<'a>(v: &'a TicketView, node: Option<&RunNode>) -> Option<&'a docket_machine::ViewUnsent> {
    let n = node?;
    v.unsent.iter().find(|u| n.rounds.iter().any(|r| r.step == u.step) || (n.state == "now" && n.who.as_deref() == Some(u.worker.as_str())))
}

/// The answers to V's awaited step that were not read, when it was passed up (docket-core 11dd0c2): who answered,
/// the answer whole (WORDS reads a transcript place), and the reading's why; None when it was not passed up.
pub fn passed_el(v: &TicketView, words: &dyn Fn(&Value) -> String, k: &Kit) -> Option<El> {
    let ViewStatus::Awaits { passed, agent, .. } = &v.status else { return None };
    if passed.is_empty() {
        return None;
    }
    let to = crate::refs::person(agent);
    let n = passed.len();
    let mut c = col().w_full().gap(8.0).padding(10.0).radius(k.radius.sm).fill(k.c.warning.bg).border(1.0, k.c.warning.border);
    c = c.child(text(format!("Passed to {to}: {n} answer{} could not be read as one of its outcomes. Each, whole, with why:", if n == 1 { "" } else { "s" })).font(k.ty.small_medium).color(k.c.warning.fg).wrap());
    for nr in passed {
        let said = words(&serde_json::to_value(&nr.text).unwrap_or(Value::Null));
        c = c.child(
            col()
                .w_full()
                .gap(4.0)
                .child(text(format!("{} answered:", nr.by)).font(k.ty.small_medium).color(k.c.text_base))
                .child(text(said).font(k.ty.small).color(k.c.text_base).wrap().selectable(true))
                .child(text(if nr.why.is_empty() { "Not read (the reading gave no reason)".to_string() } else { format!("Not read: {}", nr.why) }).font(k.ty.small).color(k.c.text_muted).wrap()),
        );
    }
    Some(c)
}

/// The awaited step of V that NODE is, when the last reading of its answer read none: its number and the why.
fn read_none_at(v: &TicketView, node: Option<&RunNode>) -> Option<(u32, String)> {
    let n = node?;
    let ViewStatus::Awaits { step, read_none: Some(why), .. } = &v.status else { return None };
    (n.state == "now" || n.rounds.iter().any(|r| r.step == *step)).then(|| (*step, why.clone()))
}

/// "not sent: WHY (since T, N tries)", as the step and the ticket page show it.
pub fn unsent_words(u: &docket_machine::ViewUnsent) -> String {
    let why = u.why.clone().unwrap_or_else(|| "the runner's command could not start".into());
    let tries = u.tries.map(|n| if n == 1 { "1 try".to_string() } else { format!("{n} tries") });
    let held: Vec<String> = u.since.as_ref().map(|t| format!("since {}", crate::when::at(t))).into_iter().chain(tries).collect();
    let held = if held.is_empty() { String::new() } else { format!(" ({})", held.join(", ")) };
    format!("not sent: {why}{held}")
}

/// "not given to AGENT for its model: WHY".
pub fn refused_words(r: &docket_machine::ViewRefused) -> String {
    format!("not given to {} for its model: {}", r.agent, r.why)
}

fn text_el(t: String, k: &Kit) -> El {
    text(t).font(k.ty.small).color(k.c.text_base).wrap()
}

fn chip(t: String, fg: rune::Color, bg: rune::Color, k: &Kit) -> El {
    // shrinks to its box and wraps there: a long reason (a model refused) stays inside the step's card
    row().shrink().min_w(0.0).px(8.0).py(2.0).radius(k.radius.sm).fill(bg).child(text(t).font(k.ty.small).color(fg).wrap().shrink())
}

fn terminal(t: String, fg: rune::Color, bg: rune::Color, k: &Kit) -> El {
    // hugs its words: a pill, not a bar
    row().w_full().min_w(0.0).child(row().shrink().min_w(0.0).px(12.0).py(5.0).radius(14.0).fill(bg).border(1.0, fg).child(text(t).font(k.ty.small_medium).color(fg).wrap().shrink()))
}

fn ink(s: State, k: &Kit) -> rune::Color {
    match s {
        State::Done => k.c.success.fg,
        State::Now => k.c.text_accent,
        State::NotTaken => k.c.text_faint,
        State::Plain | State::ToCome => k.c.text_muted,
    }
}

fn edge(s: State, halted: bool, k: &Kit) -> rune::Color {
    if halted {
        return k.c.warning.border;
    }
    match s {
        State::Done => k.c.success.border,
        State::Now => k.c.border_focus,
        State::NotTaken => k.c.border_weak,
        State::Plain => k.c.border_base,
        State::ToCome => k.c.border_weak,
    }
}

fn node_box(body: El, s: State, halted: bool, k: &Kit) -> El {
    let fill = match s {
        State::Now => k.c.layer_02,
        _ => k.c.layer_01,
    };
    col().w_full().padding(8.0).radius(k.radius.md).fill(fill).border(if halted || s == State::Now { 2.0 } else { 1.0 }, edge(s, halted, k)).child(body)
}

fn ticket_state(v: &TicketView) -> String {
    use docket_machine::ViewStatus as S;
    match &v.status {
        S::Done { .. } => "done".into(),
        S::Failed { raised } => format!("raised {}", raised.exc),
        _ if !v.halts.is_empty() => format!("halted: {} waiting on you", v.halts.len()),
        _ => "running".into(),
    }
}

fn lane_state(b: &RunNode) -> &'static str {
    match b.state.as_str() {
        "done" => "done",
        "now" => "running",
        "not_taken" => "not taken",
        _ => "to come",
    }
}

/// The first step down NS.
/// In a round past the first, NS (a sequence) after the node at work is what the round before left: each such node is
/// made to come, and the value an `Again` it returned is kept in AGAIN. A match's arms are alternatives, a par's
/// branches run side by side: neither is a sequence, so only what follows them is.
fn left_over(ns: &mut [RunNode], past: &mut bool, again: &mut Option<Value>) {
    for n in ns.iter_mut() {
        if *past {
            to_come(n, again);
            continue;
        }
        match n.what.as_str() {
            "match" | "try" => {
                let mut any = false;
                for arm in n.nodes.iter_mut() {
                    let mut p = false;
                    left_over(&mut arm.nodes, &mut p, again);
                    any |= p;
                }
                *past = any || n.state == "now";
            }
            "par" | "any" | "step" | "open" => *past = n.state == "now",
            _ => {
                let mut p = false;
                left_over(&mut n.nodes, &mut p, again);
                *past = p || n.state == "now";
            }
        }
    }
}

fn to_come(n: &mut RunNode, again: &mut Option<Value>) {
    if n.what == "return" && n.value.as_ref().is_some_and(|v| v["head"] == "Again") {
        *again = n.value.as_ref().map(|v| v["value"].clone());
    }
    n.state = "to_come".into();
    n.value = None;
    n.at = None;
    for c in n.nodes.iter_mut() {
        to_come(c, again);
    }
}

fn first_step(ns: &[RunNode]) -> Option<&RunNode> {
    ns.iter().find_map(|n| if n.what == "step" { Some(n) } else { first_step(&n.nodes) })
}

/// Whether a halt of L waits at NODE: a step's round it is about, or the step at work when it names none.
fn halts_here(l: &Lit, node: Option<&RunNode>) -> bool {
    let Some(n) = node else { return false };
    l.halted.iter().any(|h| {
        let step = match h {
            ViewHalt::Give { step, .. } | ViewHalt::Take { step, .. } | ViewHalt::Decide { step, .. } => Some(*step),
            _ => None,
        };
        match step {
            Some(s) if n.rounds.iter().any(|r| r.step == s) => true,
            // a give halts before its round exists: the step at work
            _ => n.state == "now" && !n.rounds.iter().any(|r| r.answered.is_none() && r.given.is_some()),
        }
    })
}

/// The values a ticket's form holds, by field: what the program's names stand for from its start.
fn form_env(v: &TicketView) -> crate::ran::Env {
    v.form.as_object().map(|o| o.iter().filter(|(_, x)| !x.is_null()).map(|(f, x)| (f.clone(), x.clone(), None)).collect()).unwrap_or_default()
}

/// Expression E as the picture says it: a name bound to a value shows the value (shortened), else as written.
fn shown_expr(e: &Expr, env: &crate::ran::Env, words: &dyn Fn(&Value) -> String) -> String {
    let look = |x: &str| env.iter().rev().find(|(n, _, _)| n == x).map(|(_, v, t)| value_words(v, t.as_ref(), words));
    match e {
        Expr::Var(x) => look(x).unwrap_or_else(|| x.clone()),
        Expr::Field(inner, f) => {
            if let Expr::Var(x) = inner.as_ref() {
                if let Some((_, v, _)) = env.iter().rev().find(|(n, _, _)| n == x) {
                    if let Some(fv) = v.get(f) {
                        return value_words(fv, None, words);
                    }
                }
            }
            docket_machine::print::expr(e)
        }
        Expr::Con { name, args, .. } if !args.is_empty() => format!("{name}({})", args.iter().map(|a| shown_expr(a, env, words)).collect::<Vec<_>>().join(", ")),
        Expr::Tuple(xs) => format!("({})", xs.iter().map(|a| shown_expr(a, env, words)).collect::<Vec<_>>().join(", ")),
        _ => docket_machine::print::expr(e),
    }
}

/// A match's head: what it is on, and that value once known (`match r · r is commit f995bbe`).
fn match_words(e: &Expr, env: &crate::ran::Env, words: &dyn Fn(&Value) -> String) -> String {
    let written = docket_machine::print::expr(e);
    let shown = shown_expr(e, env, words);
    if shown == written { format!("match {written}") } else { format!("match {written} · {written} is {shown}") }
}

/// The commit an argument names, once known (the runner's step checks it).
fn commit_in(e: &Expr, env: &crate::ran::Env) -> Option<String> {
    let Expr::Var(x) = e else { return None };
    let (_, v, _) = env.iter().rev().find(|(n, _, _)| n == x)?;
    let s = v.as_str()?;
    let s = s.strip_prefix("commit:").unwrap_or(s);
    (s.len() >= 7 && s.chars().all(|c| c.is_ascii_hexdigit())).then(|| s.to_string())
}

/// Value V (of type T when known) in a few words: its constructor in lower case and its fields by type, a commit by
/// its short hash, a long text by its first words.
pub fn value_words(v: &Value, t: Option<&Type>, words: &dyn Fn(&Value) -> String) -> String {
    use crate::ran::Piece;
    let mut ps = Vec::new();
    crate::ran::fields(v, t, words, &mut ps);
    let mut out: Vec<String> = Vec::new();
    if let Some(c) = crate::ran::constructor(v) {
        out.push(crate::ran::lower(c));
    }
    for p in ps {
        out.push(match p {
            Piece::Words(w) => short(&w, 36),
            Piece::Commit(h) => h.chars().take(7).collect(),
            Piece::Path(p) => p.rsplit('/').next().unwrap_or(&p).to_string(),
            Piece::Ticket(id) => id,
        });
    }
    if out.is_empty() {
        return match v {
            Value::String(s) => short(s, 36),
            Value::Null => "none".into(),
            x => short(&x.to_string(), 36),
        };
    }
    out.join(" ")
}

fn short(s: &str, n: usize) -> String {
    let s = s.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if s.chars().count() <= n {
        return s.to_string();
    }
    format!("{}…", s.chars().take(n).collect::<String>().trim_end())
}

/// An inline step's words, their first N, quoted.
fn quoted(t: &str, n: usize) -> String {
    let ws: Vec<&str> = t.split_whitespace().collect();
    format!("\u{201c}{}{}\u{201d}", ws[..ws.len().min(n)].join(" "), if ws.len() > n { " …" } else { "" })
}

/// Whether binding B opens a ticket of KIND.
fn opens_kind(b: &Binding, kind: &str) -> bool {
    b.kind == kind
}

/// Binding B in words: what starts it, and the ticket it opens.
fn binding_words(b: &Binding) -> String {
    let args: Vec<String> = b.args.iter().map(docket_machine::print::expr).collect();
    format!("{}: opens {}({})", start_words(b), b.kind, args.join(", "))
}

/// Worker W of a step as the page names it, a program's picture and the Docket alike (consult-3, N-1396 p155: one
/// name for one worker on one page): "you" for him (`refs::person`), a role by its name ("lead"), an agent by its own.
pub(crate) fn worker_words(w: &str) -> String {
    crate::refs::person(w.strip_prefix("role ").unwrap_or(w))
}

/// What starts binding B's tickets, as he reads it: "your message", "every 15 m", "every 3 of a commit", "when
/// cut-build is build".
pub(crate) fn start_words(b: &Binding) -> String {
    let ev = match b.ev.as_str() {
        "owner_message" => "your message".to_string(),
        "commit" => "a commit".to_string(),
        e => e.replace('_', " "),
    };
    match &b.every {
        Every::Each => ev,
        Every::Count(n) => format!("every {n} of {ev}"),
        Every::Clock(s) => format!("every {}", crate::when::took(*s)),
        Every::When { decision, option } => format!("when {decision} is {option}"),
    }
}

/// A clock of the project's as the snapshot has it (0.2 line 26), in words: when it fires next (its last firing and
/// its period, to coordd's one-minute round; one that never fired fires at the next) or what decides it, and when it
/// last fired.
pub(crate) fn clock_words(c: &coord_types::ClockView) -> Vec<String> {
    let mut out = Vec::new();
    match (c.every, &c.when) {
        (Some(s), _) => out.push(match c.last.as_deref().and_then(|l| next_firing(l, s)) {
            Some(next) => format!("next at {}", crate::when::at(&next)),
            None => "fires at the next minute".to_string(),
        }),
        (None, Some((decision, _))) => out.push(format!("the policy's {decision} decides, each minute")),
        (None, None) => {}
    }
    if let Some(l) = &c.last {
        out.push(format!("last {}", crate::when::at(l)));
    }
    out
}

/// When a clock that last fired at LAST fires again, SECS after it (RFC 3339).
pub(crate) fn next_firing(last: &str, secs: i64) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(last).ok()?;
    Some((t + chrono::Duration::seconds(secs)).to_rfc3339())
}

/// The bindings TEXTS (as the Kinds answer writes them) parsed as the checker parses them; one that does not parse is
/// left out.
pub(crate) fn bindings_of(texts: &[String]) -> Vec<Binding> {
    texts.iter().flat_map(|t| syntax::parse_bindings("binding", t).map(|bs| bs.into_iter().filter_map(Result::ok).collect::<Vec<_>>()).unwrap_or_default()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_round_draws_what_the_first_left_as_to_come() {
        let node = |what: &str, state: &str, value: Option<Value>, nodes: Vec<RunNode>| -> RunNode {
            serde_json::from_value(serde_json::json!({"what": what, "line": 0, "source": 0, "text": "", "state": state, "value": value, "nodes": nodes})).unwrap()
        };
        let again = serde_json::json!({"head": "Again", "value": "red: add a line"});
        let mut ns = vec![
            node("step", "now", None, vec![]),
            node("match", "done", None, vec![node("arm", "done", None, vec![node("return", "done", Some(again), vec![])])]),
        ];
        let mut found = None;
        left_over(&mut ns, &mut false, &mut found);
        assert_eq!(ns[0].state, "now");
        assert_eq!(ns[1].state, "to_come");
        assert_eq!(ns[1].nodes[0].nodes[0].state, "to_come");
        assert_eq!(found, Some(Value::String("red: add a line".into())));
    }

    const PIECE: &str = "kind piece
  letter E
  form task: Text, who: Agent
  result Ref
  raises NoPart, Exhausted, Stalled
  pattern
    iterate errs = \"none\" up to 3 {
      r <- step who \"Implement this: {{ task }}\" -> Commit(c: Ref) | Cannot(why: Text);
      match r {
        Cannot(why) => raise NoPart(why),
        Commit(c) => (v <- step role runner check(c, none);
          match v { Green => return Done(c), Red(log) => return Again(log) })
      }
    }
";

    #[test]
    fn a_kind_is_drawn_from_the_checkers_parse_and_the_run_tree_follows_it() {
        let kd = parse_kind(PIECE).unwrap();
        let Prog::Iterate { body, .. } = &kd.pattern else { panic!("{:?}", kd.pattern) };
        let s: Vec<&str> = flat(body).iter().map(|(p, _)| docket_machine::print::node(p).0).collect();
        assert_eq!(s, vec!["step", "match"]);
        assert_eq!(flat(body)[0].1.as_deref(), Some("r"));
    }

    #[test]
    fn a_named_type_the_kind_does_not_hold_stands_as_any() {
        let t = "kind piece\n  form part: Part, plan: Ref\n  result Ref\n  pattern\n    return Done(plan)\n";
        assert!(parse_kind(t).is_ok());
        assert_eq!(unknown_type("kinds:2: `Part` is not a type"), Some("Part".into()));
    }

    #[test]
    fn a_name_shows_its_value_once_known() {
        let words = |v: &Value| v.as_str().unwrap_or("").to_string();
        let env: crate::ran::Env = vec![("c".into(), serde_json::json!("f995bbe1c2"), Some(Type::Ref))];
        assert_eq!(shown_expr(&Expr::Var("c".into()), &env, &words), "f995bbe");
        assert_eq!(shown_expr(&Expr::Var("d".into()), &env, &words), "d");
        assert_eq!(value_words(&serde_json::json!({"head": "Commit", "c": "f995bbe1c2"}), None, &words), "commit f995bbe");
    }
}

/// Ticket V's stages alone, as its page draws them at its top (a thread shows those of the change it opened).
pub(crate) fn stages_of(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let pg = &s.tickets;
    let words = |x: &Value| words(pg, x);
    let pic = Pic { k, p: Some(p), kinds: kind_texts(pg), tickets: p.live.tickets.values().collect(), open: &pg.pic_open, marks: crate::edit::marks(pg), words: &words, pick: Some(v.n), picked: None };
    pic.stages(&v.program, Some(v), &v.id, &labels(s, p))
}

/// Project P's labels (its calls' and kinds' docs, picture/stages.rs): read once, kept until a reload.
pub(crate) fn labels(s: &App, p: &Proj) -> std::sync::Arc<Labels> {
    let mut m = s.tickets.labels.lock().unwrap_or_else(|e| e.into_inner());
    // YAAIHH_LABEL_FOLDERS=D[:D]: a scene's labels read from those folders instead (a coord whose files predate the docs)
    let folders = match std::env::var("YAAIHH_LABEL_FOLDERS") {
        Ok(f) => f.split(':').filter(|d| !d.is_empty()).map(std::path::PathBuf::from).collect(),
        Err(_) => crate::edit::folders(s, p),
    };
    m.entry(p.id.clone()).or_insert_with(|| std::sync::Arc::new(read_labels(&folders))).clone()
}

/// The project's kinds by name, their text (a picture's opens).
pub(crate) fn kind_texts(pg: &Page) -> Vec<(String, String)> {
    match &pg.kinds {
        Some(Ok(ks)) => ks.iter().map(|f| (f.kind.clone(), f.text.clone())).collect(),
        _ => Vec::new(),
    }
}

/// The pointer on (INSIDE) or off a name in a program's picture: on, its declaration is asked of project PID's
/// docket-ls (once; kept) and its card shown at KEY; off, the card goes.
pub fn doc_hover(s: &mut App, pid: &str, container: &str, name: &str, key: &str, inside: bool) {
    if !inside {
        if s.tickets.docs.on.as_ref().is_some_and(|o| o.key == key) {
            s.tickets.docs.on = None;
        }
        return;
    }
    // a draft drawn from its file (the Kinds page's YAAIHH_PICTURE_FILE) is read with its own folder, as its check is
    let (id, folders, bridge) = match &s.tickets.picture_file {
        Some(f) => (format!("draft {f}"), crate::edit::draft_folders(s, std::path::Path::new(f)), None),
        None => {
            let Some(p) = s.projects.iter().find(|p| p.id == pid) else { return };
            (pid.to_string(), crate::edit::folders(s, p), p.lsp())
        }
    };
    let base = s.base.clone();
    s.tickets.docs.ask(&id, &folders, &base, container, name, bridge.as_ref());
    s.tickets.docs.on = Some(crate::docket_ls::On { key: key.into(), project: id, container: container.into(), name: name.into(), folders });
}

/// The card of the name under the pointer, in the window's LAYER, under the name: its declaration in mono, its doc,
/// and where it stands (the folder named: agent-base, the library, the project).
pub fn doc_layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(o) = &s.tickets.docs.on else { return };
    let Some(r) = s.tickets.docs.got.get(&o.got_key()) else { return };
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let mut body = col();
    match r {
        Ok(Some(h)) => {
            body = body.child(col().w_full().padding(8.0).radius(k.radius.sm).fill(k.c.layer_02).child(text(h.sig.clone()).font(k.ty.mono).color(k.c.text_base).wrap()));
            if !h.doc.is_empty() {
                body = body.child(text(h.doc.clone()).font(k.ty.small).color(k.c.text_base).wrap());
            }
            if !h.at.is_empty() {
                body = body.child(faint(stands(&h.at, &o.folders, o.project.starts_with("draft "))));
            }
        }
        Ok(None) => body = body.child(faint(format!("No {} {} is declared in the project's files.", o.container, o.name))),
        Err(e) => body = body.child(faint(e.clone())),
    }
    layer.push(k.elevate(
        body.key("doc-card")
            .w(460.0)
            .gap(8.0)
            .padding(12.0)
            .radius(k.radius.md)
            .fill(k.c.surface_raised)
            .border(1.0, k.c.border_weak)
            .anchored(o.key.clone(), rune::Side::Bottom, rune::Align::Start, 6.0),
        &k.elevation.floating,
    ));
}

/// AT (`/path/file:line`) said within the folder it is in, FOLDERS being agent-base, the library (when the project
/// has one) and the project's; or, for a DRAFT, agent-base and the draft's own folder.
pub(crate) fn stands(at: &str, folders: &[std::path::PathBuf], draft: bool) -> String {
    let names: &[&str] = match (draft, folders.len()) {
        (true, _) => &["agent-base", "the draft's folder"],
        (false, 3) => &["agent-base", "the library", "the project"],
        _ => &["agent-base", "the project"],
    };
    // the deepest folder holding it (a library may sit inside the project's)
    let found = folders.iter().zip(names).filter_map(|(f, n)| Some((at.strip_prefix(f.to_str()?)?.strip_prefix('/')?, n, f.as_os_str().len()))).max_by_key(|x| x.2);
    match found {
        Some((rest, n, _)) => format!("{rest}, in {n}"),
        None => at.to_string(),
    }
}
