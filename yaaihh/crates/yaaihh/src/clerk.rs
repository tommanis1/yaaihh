//! The Clerk page: what the clerk and any other net did on
//! each event, newest first, in plain words: who ended a turn or sent what, what it filed (with Undo) and what it would
//! do were that effect turned on. A run's details (each call and decision with the model and a "Why" that opens the
//! decision, ask.rs `why`) open on demand. Above the runs: what is turned on (the `live` files), and the nets, each
//! editable and tried beside its current version. Faults, raised events and refusals are the notifications: the
//! sidebar row counts those not yet seen and stands out while there are any.

use crate::{App, Pending, Proj};
use coord_types::{MailView, NetEdit, NetMark, NetRun, NetSource, NetsView, Replayed, Request};
use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, Kit, SegmentedControl, TextArea};
use serde_json::json;
use std::collections::{HashMap, HashSet};

/// The nets shown, edited and tried on the page: the files and nets as coordd read them, the net open
/// for editing, the edited texts by file, what to try it on (typed, or a past run), the two runs (edited, current)
/// and what saving said.
#[derive(Default)]
pub struct Editor {
    pub view: Option<Result<NetsView, String>>,
    pub open: Option<String>,
    pub texts: HashMap<String, TextEdit>,
    pub input: Option<TextEdit>,
    pub picked: Option<String>,
    pub tried: [Option<Result<NetRun, String>>; 2],
    pub trying: bool,
    pub saved: Vec<Result<String, String>>,
    /// The runs whose details (calls, decisions) are open.
    pub details: HashSet<String>,
    /// Only the runs that filed something (or would have).
    pub filed_only: bool,
    /// Per net file: its last good drawing and the error of the text since, if it does not parse (netdraw.rs).
    pub graphs: HashMap<String, (Option<Vec<coord_types::NetGraph>>, Option<String>)>,
    /// Per net file: the text last sent to be drawn.
    pub drawn: HashMap<String, String>,
    /// Per net file: the open net's own lines being edited.
    pub pieces: HashMap<String, Piece>,
    /// The call files shown whole to edit, not only the calls and types the open net uses.
    pub whole: HashSet<String>,
    /// The part being marked wrong, with the note being written.
    pub noting: Option<(NetMark, TextEdit)>,
    /// The marked runs run again with the edits (`Request::NetReplay`): none while running; what the marks refused.
    pub replayed: Option<Option<Result<Vec<Replayed>, String>>>,
    pub mark_error: Option<String>,
}

/// Net NAME's own lines in its file, edited apart from the rest: the text before and after them stays as
/// it is, and the file's text is the three joined again on every change, so Save writes those lines back in place.
/// LINE: how many lines come before them.
pub struct Piece {
    pub name: String,
    pub before: String,
    pub after: String,
    pub line: usize,
    pub edit: TextEdit,
}

impl Piece {
    /// Net NAME's lines in file text T: from the comment lines right above its `net NAME` line to the line of the brace
    /// closing its body (braces in strings and comments not counted); all of T when it holds no such net.
    pub fn of(t: &str, name: &str) -> Piece {
        let lines: Vec<(usize, &str)> = t.split_inclusive('\n').scan(0, |at, l| {
            let here = *at;
            *at += l.len();
            Some((here, l))
        }).collect();
        let whole = || Piece { name: name.to_string(), before: String::new(), after: String::new(), line: 0, edit: TextEdit::new(t) };
        let Some(mut first) = lines.iter().position(|(_, l)| l.split_whitespace().take(2).eq(["net", name])) else { return whole() };
        while first > 0 && lines[first - 1].1.trim_start().starts_with("//") {
            first -= 1;
        }
        let from = lines[first].0;
        let (b, mut i, mut depth, mut opened) = (t.as_bytes(), lines[first].0, 0i32, false);
        let mut end = None;
        while i < b.len() {
            match b[i] {
                b'/' if b.get(i + 1) == Some(&b'/') => i = t[i..].find('\n').map_or(b.len(), |n| i + n) - 1,
                b'"' => {
                    i += 1;
                    while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                        i += if b[i] == b'\\' { 2 } else { 1 };
                    }
                }
                b'{' => {
                    depth += 1;
                    opened = true;
                }
                b'}' => {
                    depth -= 1;
                    if opened && depth == 0 {
                        end = Some(t[i..].find('\n').map_or(b.len(), |n| i + n + 1));
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let Some(to) = end else { return whole() };
        Piece { name: name.to_string(), before: t[..from].to_string(), after: t[to..].to_string(), line: first, edit: TextEdit::new(&t[from..to]) }
    }

    /// The file's text with these lines in place.
    pub fn joined(&self) -> String {
        format!("{}{}{}", self.before, self.edit.text, self.after)
    }
}

/// File PATH's whole text as edited (else as read).
fn whole_text(s: &App, path: &str) -> Option<String> {
    s.clerk_ed.texts.get(path).map(|t| t.text.clone()).or_else(|| match &s.clerk_ed.view {
        Some(Ok(v)) => v.sources.iter().find(|x| x.path == path).map(|x| x.text.clone()),
        _ => None,
    })
}

/// Net NAME's lines in file PATH to edit, cut afresh from the file's text when the open ones are another net's or the
/// text changed under them (a reload, a scene).
pub fn piece_mut<'a>(s: &'a mut App, path: &str, name: &str) -> &'a mut Piece {
    let whole = whole_text(s, path).unwrap_or_default();
    let fresh = !s.clerk_ed.pieces.get(path).is_some_and(|p| p.name == name && p.joined() == whole);
    if fresh {
        s.clerk_ed.pieces.insert(path.to_string(), Piece::of(&whole, name));
    }
    s.clerk_ed.pieces.get_mut(path).unwrap()
}

/// Net NAME's piece of file PATH edited: the file's text joined again and drawn.
fn piece_changed(s: &mut App, path: &str) {
    let Some(t) = s.clerk_ed.pieces.get(path).map(Piece::joined) else { return };
    match s.clerk_ed.texts.get_mut(path) {
        Some(e) => e.replace_text(t),
        None => {
            s.clerk_ed.texts.insert(path.to_string(), TextEdit::new(&t));
        }
    }
    ask_drawing(s, path);
}

/// The parts of call file text T a net with calls CALLS uses: those calls and the types they name, and the
/// types those name, each with the comment lines right above it, in the file's order.
pub fn used_part(t: &str, calls: &[String]) -> String {
    // the file's items: each from its comment lines to the line before the next item's comments
    let lines: Vec<&str> = t.split_inclusive('\n').collect();
    let head = |l: &str| l.starts_with("type ") || l.starts_with("call ");
    let mut items: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let mut j = i;
        while j < lines.len() && !head(lines[j]) {
            j += 1;
        }
        if j == lines.len() {
            break;
        }
        let name: String = lines[j][5..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let mut end = j + 1;
        while end < lines.len() && !head(lines[end]) {
            end += 1;
        }
        // the comment lines before the next item are its own
        let mut cut = end;
        while end < lines.len() && cut > j + 1 && lines[cut - 1].trim_start().starts_with("//") {
            cut -= 1;
        }
        let mut from = j;
        while from > i && lines[from - 1].trim_start().starts_with("//") {
            from -= 1;
        }
        let text: String = lines[from..cut].concat();
        items.push((name, text.trim_matches('\n').to_string()));
        i = cut;
    }
    let mut keep: HashSet<String> = calls.iter().cloned().collect();
    loop {
        let words: HashSet<&str> = items.iter().filter(|(n, _)| keep.contains(n)).flat_map(|(_, x)| x.split(|c: char| !c.is_alphanumeric() && c != '_')).collect();
        let more: Vec<String> = items.iter().filter(|(n, _)| !keep.contains(n) && words.contains(n.as_str())).map(|(n, _)| n.clone()).collect();
        if more.is_empty() {
            break;
        }
        keep.extend(more);
    }
    items.into_iter().filter(|(n, _)| keep.contains(n)).map(|(_, x)| x).collect::<Vec<_>>().join("\n\n")
}

/// Have net file PATH's text drawn (coordd parses it), unless that text was sent already.
fn ask_drawing(s: &mut App, path: &str) {
    let Some(t) = s.clerk_ed.texts.get(path).map(|t| t.text.clone()) else { return };
    if s.clerk_ed.drawn.get(path) == Some(&t) {
        return;
    }
    s.clerk_ed.drawn.insert(path.to_string(), t.clone());
    let i = s.cur;
    s.request_in(i, Request::NetGraph { file: path.to_string(), text: t.clone() }, Pending::NetGraph { path: path.to_string(), text: t });
}

/// Ask the current project's coordd for its nets.
pub fn load(s: &mut App) {
    let i = s.cur;
    s.request_in(i, Request::Nets, Pending::Nets);
}

/// The files net NAME is in or calls into: its own file, then the call files of its calls.
fn files_of<'a>(v: &'a NetsView, name: &str) -> Vec<&'a NetSource> {
    let Some(n) = v.nets.iter().find(|n| n.name == name) else { return Vec::new() };
    let mut out: Vec<&NetSource> = v.sources.iter().filter(|x| x.path == n.file).collect();
    out.extend(v.sources.iter().filter(|x| x.kind == "call" && n.calls.iter().any(|c| x.text.contains(&format!("call {c}(")))));
    out
}

/// The edits made: each file whose text differs from what coordd read, as its save target and text.
fn edits(s: &App) -> Vec<NetEdit> {
    let Some(Ok(v)) = &s.clerk_ed.view else { return Vec::new() };
    v.sources
        .iter()
        .filter_map(|x| s.clerk_ed.texts.get(&x.path).filter(|t| t.text != x.text).map(|t| NetEdit { target: x.target.clone(), text: t.text.clone() }))
        .collect()
}

/// Run net NAME dry on the picked run's event, or on what was typed (`@AGENT` names the agent): the edited version
/// and, for typed input, the current one beside it (a picked run shows what it did).
fn try_it(s: &mut App, name: &str) {
    let Some(Ok(v)) = &s.clerk_ed.view else { return };
    let Some(n) = v.nets.iter().find(|n| n.name == name) else { return };
    let Some(p) = s.proj() else { return };
    let (event, agent, value) = match s.clerk_ed.picked.as_ref().and_then(|id| p.live.nets.iter().find(|r| &r.id == id)) {
        Some(r) => (r.event.clone(), r.agent.clone(), r.input.clone()),
        None => {
            let typed = s.clerk_ed.input.as_ref().map(|t| t.text.trim().to_string()).unwrap_or_default();
            let agent = match crate::ask::tagged(p, &[&typed]) {
                Ok(a) => a.into_iter().next(),
                Err(e) => {
                    s.notice = Some(e);
                    return;
                }
            };
            let text = typed.split_whitespace().filter(|w| !w.starts_with('@')).collect::<Vec<_>>().join(" ");
            let a = agent.clone().unwrap_or_default();
            let value = match n.event.as_str() {
                "owner_message" => json!({"id": "try", "from": "owner", "text": text, "thread": null, "to": [a]}),
                "agent_message" => json!({"id": "try", "from": a, "to": ["owner"], "text": text, "thread": null}),
                "turn_end" => json!({"agent": a, "turn": 0, "text": text}),
                other => {
                    s.notice = Some(format!("a net on {other} is tried on a past run of it: pick one"));
                    return;
                }
            };
            if agent.is_none() || text.is_empty() {
                s.notice = Some("type the text and @ the agent it is about".into());
                return;
            }
            (n.event.clone(), agent, value)
        }
    };
    let typed = s.clerk_ed.picked.is_none();
    let edits = edits(s);
    let i = s.cur;
    s.clerk_ed.tried = [None, None];
    s.clerk_ed.trying = true;
    s.request_in(i, Request::NetTry { edits, event: event.clone(), agent: agent.clone(), value: value.clone() }, Pending::NetTried { edited: true });
    if typed {
        s.request_in(i, Request::NetTry { edits: Vec::new(), event, agent, value }, Pending::NetTried { edited: false });
    }
}

/// Save each edited file (coordd refuses one the check refuses, with its errors).
fn save(s: &mut App) {
    let i = s.cur;
    s.clerk_ed.saved.clear();
    for edit in edits(s) {
        s.request_in(i, Request::NetSave { edit }, Pending::NetSaved);
    }
}

/// Mark M (its verdict set) on its run; coordd sends the run back with its marks.
fn mark(s: &mut App, project: &str, m: NetMark) {
    if let Some(i) = s.projects.iter().position(|x| x.id == project) {
        s.clerk_ed.noting = None;
        s.request_in(i, Request::NetMark { mark: m }, Pending::NetMarked);
    }
}

/// Run every marked run again dry with the edits in place, each marked part beside what it gave then.
fn replay(s: &mut App) {
    let (i, edits) = (s.cur, edits(s));
    s.clerk_ed.replayed = Some(None);
    s.request_in(i, Request::NetReplay { edits }, Pending::NetReplayed);
}

/// Right / Wrong on the K-th part WHAT (KIND `step` or `effect`) of run R, and its note when marked wrong; pressing a
/// standing verdict again takes it back. Tried runs are not marked: they are not in the log.
fn marks_of(s: &App, p: &Proj, r: &NetRun, kind: &str, what: &str, k: usize, kit: &Kit) -> (El, Option<El>) {
    if r.id == "try" || r.id.is_empty() {
        return (col(), None);
    }
    let on = r.marks.iter().find(|m| m.kind == kind && m.what == what && m.k == k);
    let verdict: &'static str = match on.map(|m| m.verdict.as_str()) {
        Some("right") => "right",
        Some("wrong") => "wrong",
        _ => "none",
    };
    let base = NetMark { run: r.id.clone(), kind: kind.into(), what: what.into(), k, verdict: String::new(), note: String::new(), at: String::new() };
    let (project, right) = (p.id.clone(), base.clone());
    let mut c = row().gap(2.0).center_y().child(
        Button::new(if verdict == "right" { "✓ right" } else { "✓" })
            .variant(if verdict == "right" { ButtonVariant::Neutral } else { ButtonVariant::GhostMuted })
            .on_press(move |s: &mut App| {
                let v = if verdict == "right" { "none" } else { "right" };
                mark(s, &project, NetMark { verdict: v.into(), ..right.clone() })
            })
            .build(kit),
    );
    let (project, wrong, note) = (p.id.clone(), base.clone(), on.map(|m| m.note.clone()).unwrap_or_default());
    c = c.child(
        Button::new(if verdict == "wrong" { "✗ wrong" } else { "✗" })
            .variant(if verdict == "wrong" { ButtonVariant::Neutral } else { ButtonVariant::GhostMuted })
            .on_press(move |s: &mut App| {
                if verdict == "wrong" {
                    mark(s, &project, NetMark { verdict: "none".into(), ..wrong.clone() })
                } else {
                    s.clerk_ed.noting = Some((NetMark { verdict: "wrong".into(), ..wrong.clone() }, TextEdit::new(&note)))
                }
            })
            .build(kit),
    );
    let noting = s.clerk_ed.noting.as_ref().filter(|(m, _)| m.run == base.run && m.kind == kind && m.what == what && m.k == k);
    let below = match (noting, on) {
        (Some((_, t)), _) => {
            let project = p.id.clone();
            Some(
                row().w_full().gap(8.0).center_y().key(format!("note-{}-{kind}-{what}-{k}", r.id))
                    .child(col().grow(1.0).shrink().child(
                        TextArea::new(t).width(None).placeholder("What it should have done (optional)").auto_grow(1, 4)
                            .on_edit(note_text)
                            .build(kit),
                    ))
                    .child(Button::new("Mark wrong").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| {
                        if let Some((m, t)) = s.clerk_ed.noting.take() {
                            mark(s, &project, NetMark { note: t.text.trim().to_string(), ..m })
                        }
                    }).build(kit))
                    .child(Button::new("Cancel").variant(ButtonVariant::GhostMuted).on_press(|s: &mut App| s.clerk_ed.noting = None).build(kit)),
            )
        }
        (None, Some(m)) if m.verdict == "wrong" && !m.note.is_empty() => Some(text(format!("Should have: {}", m.note)).font(kit.ty.small).color(kit.c.danger.fg).wrap()),
        _ => None,
    };
    (c, below)
}

/// The note being written (`Editor::noting`), as the text box edits it.
fn note_text(s: &mut App) -> &mut TextEdit {
    &mut s.clerk_ed.noting.get_or_insert_with(|| (NetMark::default(), TextEdit::new(""))).1
}

/// How often each judgment was right, from the standing marks; and Replay, which runs the marked runs again with the
/// edits, each marked part beside what it gave then.
fn accuracy(s: &App, k: &Kit) -> El {
    let Some(Ok(v)) = &s.clerk_ed.view else { return col() };
    if v.marks.is_empty() {
        return text("Mark a run's steps and effects ✓ or ✗ (live or only shown) to measure the clerk and replay it on them after an edit.")
            .font(k.ty.small).color(k.c.text_faint).wrap();
    }
    let mut by: Vec<(String, usize, usize)> = Vec::new();
    for m in &v.marks {
        let key = format!("{} {}", if m.kind == "step" { "step" } else { "effect" }, m.what);
        let i = by.iter().position(|(w, ..)| *w == key).unwrap_or_else(|| {
            by.push((key, 0, 0));
            by.len() - 1
        });
        by[i].1 += usize::from(m.verdict == "right");
        by[i].2 += 1;
    }
    let mut line = row().w_full().gap(12.0).wrap().center_y().child(text("Marked:").font(k.ty.small_medium).color(k.c.text_base));
    for (w, right, all) in by {
        line = line.child(text(format!("{w} {right}/{all} right")).font(k.ty.small).color(k.c.text_muted));
    }
    let label = if edits(s).is_empty() { "Replay the marked runs" } else { "Replay the marked runs with my edits" };
    line = line.child(Button::new(label).variant(ButtonVariant::Ghost).on_press(replay).build(k));
    let mut c = col().w_full().gap(4.0).child(line);
    if let Some(e) = &s.clerk_ed.mark_error {
        c = c.child(text(format!("Not marked: {e}")).font(k.ty.small).color(k.c.danger.fg).wrap());
    }
    match &s.clerk_ed.replayed {
        None => {}
        Some(None) => c = c.child(text("replaying… (each marked run asks the models again)").font(k.ty.small).color(k.c.text_muted)),
        Some(Some(Err(e))) => c = c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap()),
        Some(Some(Ok(rs))) => {
            let show = |x: &Option<serde_json::Value>| x.as_ref().map(|v| one_line(&v.as_str().map(String::from).unwrap_or_else(|| v.to_string()), 200)).unwrap_or_else(|| "nothing".into());
            let changed = rs.iter().filter(|x| x.error.is_none() && x.before != x.after).count();
            c = c.child(text(format!("{} marked part{} replayed, {changed} changed", rs.len(), if rs.len() == 1 { "" } else { "s" })).font(k.ty.small_medium).color(k.c.text_base));
            for x in rs {
                let m = &x.mark;
                let head = format!("{} {} #{} in run {} (marked {}{})", m.kind, m.what, m.k, m.run, m.verdict, if m.note.is_empty() { String::new() } else { format!(": {}", m.note) });
                let body = match &x.error {
                    Some(e) => format!("could not replay: {e}"),
                    None if x.before == x.after => format!("same as then: {}", show(&x.after)),
                    None => format!("then: {}\nnow: {}", show(&x.before), show(&x.after)),
                };
                let ink = if x.error.is_none() && x.before != x.after { k.c.text_base } else { k.c.text_muted };
                c = c.child(col().w_full().gap(2.0).padding(8.0).radius(k.radius.sm).fill(k.c.layer_01)
                    .child(text(head).font(k.ty.small_medium).color(k.c.text_base).wrap())
                    .child(text(body).font(k.ty.small).color(ink).wrap()));
            }
        }
    }
    c
}

/// The nets section: each net with its bindings and calls; the open one with its files to edit, the try box and the
/// two runs side by side.
fn nets(s: &App, p: &Proj, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(8.0).child(
        row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(text("Nets").font(k.ty.ui_medium).color(k.c.text_base).grow(1.0))
            .child(Button::new("Reload").variant(ButtonVariant::Ghost).on_press(load).build(k)),
    );
    let v = match &s.clerk_ed.view {
        None => return c.child(small("reading the nets…".into())),
        Some(Ok(v)) if v.nets.is_empty() => return c.child(small("No nets in this project yet.".into())),
        Some(Err(e)) => return c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap()),
        Some(Ok(v)) => v,
    };
    for n in &v.nets {
        let open = s.clerk_ed.open.as_deref() == Some(n.name.as_str());
        let bound = if n.bound.is_empty() { "bound nowhere: it never runs".to_string() } else { format!("for {}", n.bound.iter().map(|b| scope_words(b)).collect::<Vec<_>>().join("; ")) };
        let name = n.name.clone();
        let files: Vec<(String, String)> = files_of(v, &n.name).iter().map(|x| (x.path.clone(), x.text.clone())).collect();
        let toggle = Button::new(if open { "Close" } else { "Edit and try" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            if s.clerk_ed.open.as_deref() == Some(name.as_str()) {
                s.clerk_ed.open = None;
            } else {
                s.clerk_ed.open = Some(name.clone());
                for (path, text) in &files {
                    s.clerk_ed.texts.entry(path.clone()).or_insert_with(|| TextEdit::new(text));
                }
                if let Some((path, _)) = files.first() {
                    ask_drawing(s, path);
                }
                s.clerk_ed.tried = [None, None];
                s.clerk_ed.picked = None;
            }
        });
        let mut card = col().key(format!("netc-{}", n.name)).w_full().gap(6.0).padding(12.0).radius(k.radius.sm).fill(k.c.layer_01).child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(text(format!("{} — {}", n.name, event_words(&n.event))).font(k.ty.small_medium).color(k.c.text_base).grow(1.0).shrink())
                .child(toggle.build(k)),
        );
        let calls = if n.calls.is_empty() { "asks no model".into() } else { format!("asks the model: {}", n.calls.join(", ")) };
        card = card.child(small(format!("{bound} · {}, line {} · {calls}", short(&n.file), n.line)));
        if open {
            card = card.child(editor(s, p, v, &n.name, &n.event, k));
        }
        c = c.child(card);
    }
    for e in &v.errors {
        c = c.child(row().w_full().gap(8.0).child(Badge::new("refused").variant(BadgeVariant::Accent).build(k)).child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap().grow(1.0).shrink()));
    }
    c
}

/// Net NAME open: its files to edit (saved at their target), the input to try it on, Try and Save, and the runs.
fn editor(s: &App, p: &Proj, v: &NetsView, name: &str, event: &str, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(8.0);
    let info = v.nets.iter().find(|n| n.name == name);
    let calls: HashSet<String> = v.sources.iter().filter(|x| x.kind == "call").flat_map(|x| x.text.lines().filter_map(|l| l.strip_prefix("call ")).map(|l| l.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect::<String>())).collect();
    let mono = |t: String| text(t.clone()).font(k.ty.small).spans(crate::netdraw::highlight(&t, &calls, k)).wrap();
    for f in files_of(v, name) {
        let whole = s.clerk_ed.texts.get(&f.path).map_or(f.text.clone(), |t| t.text.clone());
        let changed = whole != f.text;
        let saves = if f.target == f.path { "Save changes it here".to_string() } else { format!("Save writes the project's own {} (this project only)", short(&f.target)) };
        let edited = if changed { " · edited" } else { "" };
        let is_net = f.kind == "net";
        let shown_whole = !is_net && s.clerk_ed.whole.contains(&f.path);
        // a net file shows the open net's own lines; a call file the calls and types the net uses, until asked for whole
        let (title, edit_owned);
        let edit: &TextEdit = if is_net {
            match s.clerk_ed.pieces.get(&f.path) {
                Some(p) if p.name == name && p.joined() == whole => {
                    title = format!("{name} in {}, lines {}–{}", short(&f.path), p.line + 1, p.line + p.edit.text.lines().count());
                    &p.edit
                }
                _ => {
                    let p = Piece::of(&whole, name);
                    title = format!("{name} in {}, lines {}–{}", short(&f.path), p.line + 1, p.line + p.edit.text.lines().count());
                    edit_owned = p.edit;
                    &edit_owned
                }
            }
        } else {
            title = if shown_whole { short(&f.path) } else { format!("{} — what {name} uses", short(&f.path)) };
            edit_owned = TextEdit::new(&whole);
            &edit_owned
        };
        let path = f.path.clone();
        let toggle = (!is_net).then(|| {
            Button::new(if shown_whole { "Show only what it uses" } else { "Edit the whole file" })
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut App| {
                    if !s.clerk_ed.whole.remove(&path) {
                        s.clerk_ed.whole.insert(path.clone());
                    }
                })
                .build(k)
        });
        let mut head = row().w_full().gap(8.0).center_y().child(text(format!("{title} ({} file){edited}", scope_words(&f.scope))).font(k.ty.small_medium).color(k.c.text_base).grow(1.0).shrink());
        if let Some(t) = toggle {
            head = head.child(t);
        }
        c = c.child(head).child(small(saves));
        if !is_net && !shown_whole {
            let part = used_part(&whole, info.map_or(&[][..], |n| &n.calls[..]));
            c = c.child(col().w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).child(mono(part)));
            continue;
        }
        let (path, name_) = (f.path.clone(), name.to_string());
        let area = TextArea::new(edit).width(None).auto_grow(6, if is_net { 80 } else { 18 }).spans(crate::netdraw::highlight(&edit.text, &calls, k));
        let area = if is_net {
            let changed_path = f.path.clone();
            area.on_edit(move |s: &mut App| &mut piece_mut(s, &path, &name_).edit).on_change(move |s: &mut App, _| piece_changed(s, &changed_path))
        } else {
            let original = f.text.clone();
            area.on_edit(move |s: &mut App| s.clerk_ed.texts.entry(path.clone()).or_insert_with(|| TextEdit::new(&original)))
        };
        let area = col().key(format!("nf-{}", f.path)).w(0.0).grow(1.0).min_w(if is_net { 400.0 } else { 260.0 }).child(area.build(k));
        let line = if is_net { s.clerk_ed.pieces.get(&f.path).filter(|p| p.name == name && p.joined() == whole).map_or_else(|| Piece::of(&whole, name).line, |p| p.line) } else { 0 };
        c = c.child(if is_net { row().w_full().gap(12.0).items_start().child(area).child(drawing(s, p, f, edit, line, name, k)) } else { row().w_full().child(area) });
    }
    // what to try it on: typed, or one of its past runs
    let hint = match event {
        "owner_message" => "A message of yours to try it on; @ the agent it goes to",
        "agent_message" => "An agent's mail to try it on; @ the agent that sends it",
        "turn_end" => "An agent's turn text to try it on; @ the agent",
        _ => "Pick a past run to try it on",
    };
    c = c.child(text("Try it").font(k.ty.small_medium).color(k.c.text_base).pt(8.0));
    let empty = TextEdit::new("");
    c = c.child(
        col().key("net-try").w_full().child(
            TextArea::new(s.clerk_ed.input.as_ref().unwrap_or(&empty))
                .width(None)
                .placeholder(hint)
                .auto_grow(2, 8)
                .on_edit(|s: &mut App| {
                    s.clerk_ed.picked = None;
                    s.clerk_ed.input.get_or_insert_with(|| TextEdit::new(""))
                })
                .build(k),
        ),
    );
    let mut past = row().w_full().gap(4.0).wrap().center_y().child(small("or on what really happened:".into()));
    for r in p.live.nets.iter().rev().filter(|r| r.event == event && r.id != "try").take(6) {
        let (id, on) = (r.id.clone(), s.clerk_ed.picked.as_deref() == Some(r.id.as_str()));
        past = past.child(
            Button::new(format!("{} {}", r.agent.as_deref().unwrap_or("?"), crate::domain::local(&r.at)))
                .variant(if on { ButtonVariant::Neutral } else { ButtonVariant::Ghost })
                .on_press(move |s: &mut App| s.clerk_ed.picked = if s.clerk_ed.picked.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) })
                .build(k),
        );
    }
    c = c.child(past);
    let n = name.to_string();
    let edited = !edits(s).is_empty();
    let mut acts = row().gap(8.0).center_y().child(Button::new("Try").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| try_it(s, &n)).build(k));
    if edited {
        acts = acts.child(Button::new("Save").variant(ButtonVariant::Warning).on_press(save).build(k));
        acts = acts.child(Button::new("Discard edits").variant(ButtonVariant::Ghost).on_press(|s: &mut App| {
            s.clerk_ed.texts.clear();
            s.clerk_ed.pieces.clear();
        }).build(k));
    } else {
        acts = acts.child(small("Edit the text above to change it; Try runs it without doing anything.".into()));
    }
    c = c.child(acts);
    for r in &s.clerk_ed.saved {
        c = c.child(match r {
            Ok(t) => small(format!("Saved: {t}")),
            Err(e) => text(format!("Not saved — the check refused it:\n{e}")).font(k.ty.small).color(k.c.danger.fg).wrap(),
        });
    }
    // the runs: the edited version, and beside it the current one (typed) or what the picked run did
    if s.clerk_ed.trying || s.clerk_ed.tried.iter().any(Option::is_some) {
        let picked = s.clerk_ed.picked.as_ref().and_then(|id| p.live.nets.iter().find(|r| &r.id == id));
        let side = |title: String, r: Option<Result<&NetRun, &String>>| -> El {
            let mut col_ = col().grow(1.0).shrink().w(0.0).gap(6.0).child(text(title).font(k.ty.small_medium).color(k.c.text_base));
            col_ = col_.child(match r {
                None => small("running… (the clerk's model answers in a few seconds)".into()),
                Some(Err(e)) => text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap(),
                Some(Ok(r)) => run(s, p, r, true, k),
            });
            col_
        };
        let edited = side(if edits(s).is_empty() { "Now (no edits)".into() } else { "With your edits".into() }, s.clerk_ed.tried[0].as_ref().map(|r| r.as_ref()));
        let other = match picked {
            Some(r) => side(format!("What it did then ({})", crate::domain::local(&r.at)), Some(Ok(r))),
            None => side("As it is now".into(), s.clerk_ed.tried[1].as_ref().map(|r| r.as_ref())),
        };
        c = c.child(row().w_full().gap(12.0).child(edited).child(other));
    }
    c
}

/// The drawing of net NAME beside its lines EDIT in file F, LINE lines in (netdraw.rs): the last good one, the parse error over it, the caret's
/// step marked, and the path of the run shown below (the edited try, else the picked past run).
fn drawing(s: &App, p: &Proj, f: &NetSource, edit: &TextEdit, line: usize, name: &str, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let (graphs, error) = s.clerk_ed.graphs.get(&f.path).map_or((None, None), |(g, e)| (g.as_ref(), e.as_ref()));
    let mut c = col().gap(6.0).max_w(900.0);
    let picked = s.clerk_ed.picked.as_ref().and_then(|id| p.live.nets.iter().find(|r| &r.id == id));
    let run = match &s.clerk_ed.tried[0] {
        Some(Ok(r)) => Some((r, "your try".to_string())),
        _ => picked.map(|r| (r, format!("{}'s run at {}", r.agent.as_deref().unwrap_or("the"), crate::domain::local(&r.at)))),
    };
    let head = match &run {
        Some((_, which)) => format!("Drawing (read-only): the green path is what {which} did; hover a step for its output"),
        None => "Drawing (read-only): click a step to select its text; the caret's step is outlined".to_string(),
    };
    c = c.child(small(head).max_w(560.0));
    if let Some(e) = error {
        let e = e.replace(&f.path, &short(&f.path));
        c = c.child(text(format!("The text does not parse (the drawing is the last one that did): {e}")).font(k.ty.small).color(k.c.danger.fg).max_w(560.0).wrap());
    }
    let Some(g) = graphs.and_then(|gs| gs.iter().find(|g| g.name == name)) else {
        return c.child(small(if graphs.is_some() { format!("No net {name} in this text.") } else { "drawing…".into() }));
    };
    let caret_line = line + edit.line_of_byte(edit.focus.min(edit.text.len())) + 1;
    let live = |effect: &str| s.clerk_ed.view.as_ref().and_then(|v| v.as_ref().ok()).is_some_and(|v| live_effects(v).iter().any(|e| e == effect));
    let marks = crate::netdraw::Marks {
        path: &f.path,
        name,
        caret: if edit.focused || edit.anchor != edit.focus { crate::netdraw::caret_step(g, caret_line) } else { None },
        run: run.map(|(r, _)| crate::netdraw::outputs(r)),
        live: &live,
    };
    // natural size, no viewport of its own: the page scrolls it, and the text beside it takes the width it leaves
    c.child(col().key(format!("nd-{}", f.path)).radius(k.radius.sm).fill(k.c.bg_deep).border(1.0, k.c.border_weak).child(crate::netdraw::draw(g, &marks, k)))
}

/// The effects turned on for everyone, from the `live` files (the narrowest scope's line wins).
fn live_effects(v: &NetsView) -> Vec<String> {
    let mut on: Vec<String> = Vec::new();
    for x in v.sources.iter().filter(|x| x.kind == "live") {
        for l in x.text.lines().map(|l| l.split("//").next().unwrap_or("").trim()) {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w.len() == 2 {
                on.retain(|e| e != w[1]);
                if w[0] == "live" {
                    on.push(w[1].to_string());
                }
            }
        }
    }
    on
}

/// The runs seen on the page, by project: the highest run number shown when it was last open.
#[derive(Default)]
pub struct Seen(pub HashMap<String, u64>);

fn number(id: &str) -> u64 {
    id.trim_start_matches('N').parse().unwrap_or(0)
}

/// Whether R is a notification: it faulted or raised an event.
fn notable(r: &NetRun) -> bool {
    r.fault.is_some() || !r.raised.is_empty()
}

/// The notifications of P not yet seen: its refusals, and its runs that faulted or raised since the page was open.
pub fn unseen(s: &App, p: &Proj) -> usize {
    let seen = s.clerk_seen.0.get(&p.id).copied().unwrap_or(0);
    p.live.net_errors.len() + p.live.nets.iter().filter(|r| number(&r.id) > seen && notable(r)).count()
}

/// The page is shown: its runs are seen.
pub fn mark_seen(s: &mut App) {
    let Some(p) = s.proj() else { return };
    let (id, top) = (p.id.clone(), p.live.nets.iter().map(|r| number(&r.id)).max().unwrap_or(0));
    s.clerk_seen.0.insert(id, top);
}

pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(12.0).padding(24.0);
    c = c.child(small(
        "The clerk reads each agent's turn as it ends and each message before it goes. It files an agent's questions as \
         questions and its other words in the thread they belong to; Undo under a filed message takes it back. What it \
         would do beyond what is turned on is only shown here, marked \"would\"."
            .into(),
    ));
    c = c.child(live_line(s, k));
    c = c.child(nets(s, p, k));
    let filed_only = s.clerk_ed.filed_only;
    c = c.child(
        row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(text("What it did").font(k.ty.ui_medium).color(k.c.text_base).grow(1.0))
            .child(SegmentedControl::new(&["Everything", "Only filing"], Some(filed_only as usize)).fit().label("Runs shown").on_pick(|s: &mut App, i, _| s.clerk_ed.filed_only = i == 1).build(k)),
    );
    c = c.child(accuracy(s, k));
    for e in &p.live.net_errors {
        c = c.child(row().w_full().gap(8.0).child(Badge::new("refused").variant(BadgeVariant::Accent).build(k)).child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap().grow(1.0).shrink()));
    }
    let shown: Vec<&NetRun> = p.live.nets.iter().rev().filter(|r| !filed_only || r.effects.iter().any(|e| FILING.contains(&e.effect.as_str()))).collect();
    if shown.is_empty() {
        c = c.child(small(if filed_only { "Nothing filed yet.".into() } else { "The clerk has not run yet: it runs when an agent ends a turn or a message is sent.".into() }));
    }
    for r in shown {
        c = c.child(run(s, p, r, false, k));
    }
    col().w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// The effects that file: a question opened, an agent's words put in its thread.
const FILING: [&str; 2] = ["ask", "post"];

/// What is turned on, from the `live` files in plain words (the narrowest scope's line wins, as in coordd).
fn live_line(s: &App, k: &Kit) -> El {
    let Some(Ok(v)) = &s.clerk_ed.view else { return col() };
    let mut on: Vec<String> = Vec::new();
    for x in v.sources.iter().filter(|x| x.kind == "live") {
        for l in x.text.lines().map(|l| l.split("//").next().unwrap_or("").trim()).filter(|l| !l.is_empty()) {
            let mut w = l.split_whitespace();
            let (Some(sw), Some(effect)) = (w.next(), w.next()) else { continue };
            let rest: String = w.collect::<Vec<_>>().join(" ");
            let who = if rest.is_empty() { String::new() } else { format!(" ({rest})") };
            let said = format!("{}{who}", effect_words(effect));
            on.retain(|o| o != &said);
            if sw == "live" {
                on.push(said);
            }
        }
    }
    let t = if on.is_empty() { "Turned on: nothing. Everything it would do is only shown.".to_string() } else { format!("Turned on: {}. Everything else is only shown.", on.join(", ")) };
    text(t).font(k.ty.small_medium).color(k.c.text_base).wrap()
}

/// EFFECT in plain words.
fn effect_words(effect: &str) -> &str {
    match effect {
        "ask" => "opening questions",
        "post" => "filing replies in threads",
        "deliver" => "delivering messages",
        "reject" => "sending messages back",
        "nudge" => "nudging agents",
        "accept" => "letting turns end",
        "tell" => "telling agents",
        other => other,
    }
}

/// When a net on EVENT runs, in plain words.
fn event_words(event: &str) -> String {
    match event {
        "turn_end" => "runs when an agent ends a turn".into(),
        "owner_message" => "runs on each message you send".into(),
        "agent_message" => "runs on each message an agent sends".into(),
        other => format!("runs on {other}"),
    }
}

/// A scope in plain words.
fn scope_words(scope: &str) -> String {
    match scope {
        "global" => "all projects".into(),
        "project" => "this project".into(),
        other => other.trim_start_matches("agent ").to_string(),
    }
}

/// PATH as its folder and name (`nets/clerk.net`).
fn short(path: &str) -> String {
    let parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// Run R as a card: who did what, what it filed or would do; its calls and decisions under Details (open when
/// EXPANDED, as in a tried run beside another).
fn run(s: &App, p: &Proj, r: &NetRun, expanded: bool, k: &Kit) -> El {
    let line = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let who = r.agent.as_deref().unwrap_or("someone");
    let what = match r.event.as_str() {
        "turn_end" => format!("{who} ended a turn"),
        "owner_message" => format!("you wrote to {}", r.input.get("to").map(names).unwrap_or_default()),
        "agent_message" => format!("{who} wrote to {}", r.input.get("to").map(names).unwrap_or_default()),
        other => format!("{other} ({who})"),
    };
    let mut head = row().w_full().gap(8.0).center_y().child(
        text(format!("{what} · {}", crate::domain::local(&r.at))).font(k.ty.small_medium).color(k.c.text_base).grow(1.0).shrink(),
    );
    let filed = r.effects.iter().filter(|e| e.live && e.done.as_deref().is_some_and(|d| !d.starts_with("not taken"))).count();
    if filed > 0 {
        head = head.child(Badge::new(format!("did {filed}")).variant(BadgeVariant::Accent).build(k));
    } else if r.dry {
        head = head.child(Badge::new("only shown").build(k));
    }
    if r.fault.is_some() {
        head = head.child(Badge::new("fault").variant(BadgeVariant::Accent).build(k));
    }
    let mut c = col().key(format!("net-{}", r.id)).w_full().gap(4.0).padding(12.0).radius(k.radius.sm).fill(k.c.layer_01).child(head);
    let input = r.input.get("text").and_then(|t| t.as_str()).map(|t| one_line(t, 160)).unwrap_or_else(|| one_line(&r.input.to_string(), 160));
    c = c.child(text(format!("“{input}”")).font(k.ty.small).color(k.c.text_faint).truncate().w_full());
    if r.effects.is_empty() && r.fault.is_none() {
        c = c.child(line("It did nothing.".into()));
    }
    let mut seen: HashMap<String, usize> = HashMap::new();
    for e in &r.effects {
        let nth = seen.entry(e.effect.clone()).or_default();
        let (marks, note) = marks_of(s, p, r, "effect", &e.effect, *nth, k);
        *nth += 1;
        let arg = |name: &str| e.args.get(name).map(|v| v.as_str().map(String::from).unwrap_or_else(|| names(v))).filter(|v| !v.is_empty() && v != "null");
        let words = arg("text").map(|t| format!(": “{}”", one_line(&t, 200))).unwrap_or_default();
        let thread = arg("thread").or_else(|| arg("on"));
        let in_thread = thread.as_ref().map(|q| format!(" in {q}")).unwrap_or_default();
        let to = arg("to").map(|t| format!(" to {t}")).unwrap_or_default();
        let plain = match e.effect.as_str() {
            "ask" if thread.is_some() => format!("file a follow-up question{in_thread}{words}"),
            "ask" => format!("open a question{words}"),
            "post" => format!("file a reply{in_thread}{words}"),
            "deliver" => format!("deliver{to}{in_thread}{words}"),
            "reject" => format!("send it back{to}: {}", arg("why").unwrap_or_default()),
            "nudge" => format!("nudge{to}{words}"),
            "tell" => format!("tell{to}{words}"),
            "accept" => match e.args.get("wake").filter(|w| !w.is_null()) {
                Some(w) => format!("let the turn end, waking it {}", wake_words(w)),
                None => "let the turn end".into(),
            },
            other => format!("{other} ({})", one_line(&e.args.to_string(), 200)),
        };
        // a live effect says what it made, and a filed mail can be taken back here
        let (t, made) = match (e.live, e.done.as_deref()) {
            (false, _) => (format!("Would {plain}"), None),
            (true, Some(d)) if d.starts_with("not taken") => (format!("Did not {plain} — {}", d.trim_start_matches("not taken").trim_start_matches(':').trim()), None),
            (true, d) => {
                let id = d.unwrap_or("?");
                let done = match e.effect.as_str() {
                    "ask" => format!("Opened question {id}{words}"),
                    "post" => format!("Filed in {}{words} (as {id})", thread.as_deref().unwrap_or("its thread")),
                    _ => format!("Did {plain} → {id}"),
                };
                (done, d.and_then(|d| p.live.mail.get(d)))
            }
        };
        let ink = if e.live { k.c.text_base } else { k.c.text_muted };
        let mut row_ = row().w_full().gap(8.0).child(text(t).font(k.ty.small).color(ink).wrap().grow(1.0).shrink());
        if let Some(m) = made {
            row_ = row_.child(undo(p, m, k));
        }
        c = c.child(row_.child(marks));
        if let Some(n) = note {
            c = c.child(n);
        }
    }
    if let Some(f) = &r.fault {
        c = c.child(text(format!("Fault: {f}")).font(k.ty.small).color(k.c.danger.fg).wrap());
    }
    if !r.raised.is_empty() {
        c = c.child(line(format!("Raised: {}", r.raised.join(", "))));
    }
    let open = expanded || s.clerk_ed.details.contains(&r.id);
    if !expanded && !r.steps.is_empty() {
        let id = r.id.clone();
        let label = if open { "Hide details".to_string() } else { format!("Details: what the model said ({} step{})", r.steps.len(), if r.steps.len() == 1 { "" } else { "s" }) };
        c = c.child(row().child(Button::new(label).variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| {
            if !s.clerk_ed.details.remove(&id) {
                s.clerk_ed.details.insert(id.clone());
            }
        }).build(k)));
    }
    if !open {
        return c;
    }
    c = c.child(line(format!("run {} · net {}", r.id, r.net)));
    let mut seen: HashMap<String, usize> = HashMap::new();
    for st in &r.steps {
        let nth = seen.entry(st.what.clone()).or_default();
        let (marks, note) = marks_of(s, p, r, "step", &st.what, *nth, k);
        *nth += 1;
        let model = st.model.as_deref().map(|m| format!(" ({m})")).unwrap_or_default();
        let out = st.out.as_ref().map(|o| one_line(&o.as_str().map(String::from).unwrap_or_else(|| o.to_string()), 400)).unwrap_or_else(|| "no answer".into());
        let mut step = row().w_full().gap(8.0).child(line(format!("{}{model}, line {} → {out}", st.what, st.line)).grow(1.0).shrink());
        if let Some(d) = &st.decision {
            let key = format!("{}/{d}", p.id);
            let open = s.notes.why.contains_key(&key);
            let (project, id) = (p.id.clone(), d.clone());
            step = step.child(Button::new(if open { "Hide why" } else { "Why this model" }).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::ask::toggle_why(s, &project, &id)).build(k));
        }
        c = c.child(step.child(marks));
        if let Some(n) = note {
            c = c.child(n);
        }
        if let Some(d) = &st.decision {
            match s.notes.why.get(&format!("{}/{d}", p.id)) {
                Some(Some(Ok(d))) => c = c.child(crate::ask::why(d, k)),
                Some(Some(Err(e))) => c = c.child(line(e.clone())),
                Some(None) => c = c.child(line("…".into())),
                None => {}
            }
        }
    }
    c
}

/// When a wake token W wakes the agent, in plain words.
fn wake_words(w: &serde_json::Value) -> String {
    let field = |f: &str| w.get(f).and_then(|x| x.as_str()).unwrap_or("?").to_string();
    match w.get("kind").and_then(|k| k.as_str()) {
        Some("Answer") => format!("when {} is answered", field("q")),
        Some("Check") => format!("when {} finishes", field("check")),
        Some("Mail") => format!("when {} writes", field("from")),
        _ => format!("on {}", one_line(&w.to_string(), 80)),
    }
}

/// A `to` value (a name or a list of names) as "a, b".
fn names(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Array(xs) => xs.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(", "),
        serde_json::Value::String(x) => x.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The Undo of mail M a net filed: the owner takes it back (`Request::Unfile`), or that it was taken back.
fn undo(p: &Proj, m: &MailView, k: &Kit) -> El {
    if m.unfiled {
        return text("taken back").font(k.ty.small).color(k.c.text_faint);
    }
    let (project, id) = (p.id.clone(), m.id.clone());
    Button::new("Undo")
        .variant(ButtonVariant::GhostMuted)
        .on_press(move |s: &mut App| {
            if let Some(i) = s.projects.iter().position(|x| x.id == project) {
                s.mail_in(i, Request::Unfile { id: id.clone() });
            }
        })
        .build(k)
        .help("Take back what the clerk filed: the message leaves the thread (a filed question is no longer a question)")
}

/// Under mail M of a thread a net filed: by which run, and its Undo.
pub fn filed_row(p: &Proj, m: &MailView, k: &Kit) -> Option<El> {
    let run = m.filed.as_ref()?;
    let run_at = p.live.nets.iter().find(|r| &r.id == run).map(|r| format!(" at {}", crate::domain::local(&r.at))).unwrap_or_default();
    let what = if m.kind == "question" { "Opened as a question" } else { "Filed here" };
    let t = if m.unfiled { format!("{what} by the clerk from {}'s turn{run_at}; taken back", m.from) } else { format!("{what} by the clerk from {}'s turn{run_at}", m.from) };
    let mut r = row().w_full().gap(8.0).center_y().child(text(t).font(k.ty.small).color(k.c.text_faint).grow(1.0).shrink());
    if !m.unfiled {
        r = r.child(undo(p, m, k));
    }
    Some(r)
}

fn one_line(t: &str, n: usize) -> String {
    let t: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() > n { format!("{}…", t.chars().take(n).collect::<String>()) } else { t }
}
