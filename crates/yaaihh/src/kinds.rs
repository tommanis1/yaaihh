//! The Kinds page (S6, in place of the Clerk page): the kinds of ticket the project runs, as coordd read them
//! (`Request::Kinds`, the Tickets page's own read), each with its form, result, the failures it raises and the most
//! steps a ticket of it can take; its text, as written, opens on demand. Above them: what the kinds' and policy's
//! check refused (`Body::Refusals`), which the sidebar row counts.

use crate::{App, Proj};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use std::collections::HashSet;
use serde_json::json;
use serde_json::Value;
use serde_json::Map;
use serde::Deserialize;
use rune_kit::TextInput;
use rune_kit::TextArea;
use rune::TextEdit;
use docket_machine::Type;
use crate::threads::{Cleared, Page};
use crate::ask::is_text;
use crate::Pending;
use coord_types::Request;

/// What a page drawn from the kinds says while it holds none (N-1446 p198): while coordd's pipe is DOWN, that they are
/// read once it is back (why it is down is said once, in the bar and the sidebar); an ask the pipe's end lost is asked
/// again then (lib.rs `Incoming::Down`). Else they are on their way.
pub(crate) fn waiting_words(down: bool) -> &'static str {
    if down { "Disconnected; kinds load on reconnect." } else { "Reading the kinds…" }
}

pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let v = s.tickets.shown.and_then(|n| crate::find::ticket(s, p, n));
    page_at(s, p, crate::picture::docket::scale(&s.tickets.pic_open), v, k)
}

/// The Docket at scale AT, thread V's program at the Thread scale: the page's (`page`, the thread open and the scale
/// picked), and a Docket lens's in a pane that is not focused (lens.rs `docket_apart`, the thread it keeps).
pub(crate) fn page_at(s: &App, p: &Proj, at: crate::picture::docket::Scale, v: Option<&coord_types::TicketView>, k: &Kit) -> El {
    use crate::picture::docket::{self, Scale};
    if let Some(el) = crate::programs::engine_page(s, p, k) {
        return el;
    }
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut c = col().w_full().gap(12.0).padding(24.0);
    // the Docket's scale at the page's head, one switch (paper-research N-1396 p218): the project's board and its kinds,
    // one agent's part, the program of the thread open
    let labels: Vec<&str> = Scale::ALL.iter().map(|x| x.label()).collect();
    c = c.child(
        rune_kit::SegmentedControl::new(&labels, Scale::ALL.iter().position(|x| *x == at))
            .fit()
            .label("The Docket at")
            .on_pick(|s: &mut App, i, _| docket::set_scale(s, Scale::ALL[i]))
            .build(k)
            .key("docket-scale"),
    );
    if at == Scale::Thread {
        return c.child(thread_scale(s, p, v, k));
    }
    if at == Scale::Project {
        c = c.child(small(
            "The kinds of ticket this project runs: what opening one asks for (its form), what it ends with (its result), \
             the failures it can raise, and the most steps a ticket of it takes. An agent's turn and the owner's words are \
             filed by the filing kind."
                .into(),
        ));
        c = c.child(crate::programs::v1(s, p, k));
        // the window's look, edited in the kinds editor's place (look.rs; N-866.p6, 2), from a coordd that serves it
        if p.protocol >= coord_types::LOOK_PROTOCOL && s.tickets.picture_file.is_none() {
            c = c.child(crate::look::card(s, p, k));
        }
    }
    // a step of a picture clicked: its call's template (edit.rs), in the card its step is in when that card is drawn
    // open, else here
    let call_in = docket::picked(&s.tickets.pic_open, docket::CALL_IN).filter(|n| at == Scale::Project && s.kinds_open.contains(*n) && s.tickets.picture_file.is_none());
    if let Some(panel) = crate::edit::call_panel(s, k).filter(|_| call_in.is_none()) {
        c = c.child(panel);
    } else if let Some(Err(e)) = &s.tickets.edits.said {
        c = c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap());
    } else if let Some(said) = crate::edit::said(s, k).filter(|_| s.tickets.edits.kind.is_none()) {
        // a kind's Save: its line, and under it the reload's reply, each open ticket on its own line
        c = c.child(said);
    }
    if let Some(f) = &s.tickets.picture_file {
        return c.child(file_card(s, f, k));
    }
    let kinds = match &s.tickets.kinds {
        Some(Ok(ks)) => ks,
        Some(Err(e)) => return c.child(text(format!("The kinds could not be read: {e}")).font(k.ty.small).color(k.c.danger.fg).wrap()),
        None => return c.child(small(waiting_words(p.down.is_some()).into())),
    };
    if kinds.is_empty() {
        c = c.child(small("This project has no kinds yet.".into()));
    }
    if at == Scale::Agent {
        return c.child(docket::agent_scale(s, p, kinds, k)).children(crate::edit::clock_editor(s, k));
    }
    // the project's Docket over the cards (0.2 line 26): its programs as one drawing, what starts each and whom each
    // step goes to; the program picked from it is the first card under it, its text open
    let shown = crate::picture::shown_kind(&s.tickets.pic_open);
    if !kinds.is_empty() {
        let words = |x: &Value| crate::texts::words(&s.tickets, x);
        let pic = crate::picture::Pic { k, p: Some(p), kinds: crate::picture::kind_texts(&s.tickets), tickets: Vec::new(), open: &s.tickets.pic_open, marks: crate::edit::marks(&s.tickets), words: &words, pick: None, picked: None };
        c = c.child(pic.docket(kinds)).children(crate::edit::clock_editor(s, k));
    }
    for f in kinds.iter().filter(|f| shown == Some(f.kind.as_str())).chain(kinds.iter().filter(|f| shown != Some(f.kind.as_str()))) {
        c = c.child(card(s, f, k));
    }
    c
}

/// The Docket at the Thread scale (p218): the program of thread V, the one open on the Threads page or a Docket lens's,
/// lit where it stands (ticket.rs `program_part`, as its own page draws it), the panel of a stage or step pressed in it
/// beside it; one press back to the thread.
fn thread_scale(s: &App, p: &Proj, v: Option<&coord_types::TicketView>, k: &Kit) -> El {
    let small = |t: &str| text(t.to_string()).font(k.ty.small).color(k.c.text_muted).wrap();
    let Some(v) = v else {
        return small("Open a thread to see its program.");
    };
    let n = v.n;
    // a handler's run (docket.rs `open_run`): back to the thread it ran under, at this scale
    let up = v.under.as_deref().and_then(|u| p.live.tickets.values().find(|t| t.id == u && t.ran.iter().any(|r| r.id == v.id)));
    let back = match up {
        Some(t) => {
            let up = t.n;
            Button::new(format!("‹ {}", t.id)).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::picture::docket::open_thread(s, up))
        }
        // the thread he came from, one Back away: that Back, so Back after it is not this page again (the walk's 6,
        // paper-research N-1446 p2120); else the thread opened
        None => Button::new("‹ the thread").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            if crate::history::back_is(s, |p| p.sel.shows_threads() && p.thread == Some(n)) {
                crate::history::go(s, true, 1);
            } else {
                crate::threads::show(s, n);
            }
        }),
    }
    .build(k)
    .key("docket-back");
    let head = row().w_full().gap(10.0).center_y().child(back).child(crate::refs::el(&crate::refs::ticket(Some(p), v), k.ty.body_medium, k.c.text_base, k));
    let c = col().key("docket-thread").w_full().gap(12.0).child(head);
    // a thread with no program: nothing of its own to draw; the project's are a press away (consult-3 p222)
    if v.program.trim().is_empty() {
        return c.child(small("It runs no program."));
    }
    let mut part = crate::ticket::program_part(s, p, v, false, k);
    // a press in its whole program lit lines of its text: the text, those lines lit, under it (p223); edited in place
    // there (p294)
    let spans = crate::picture::lit_spans(&s.tickets.pic_open, &v.kind, &v.program, k);
    let box_of = |el: El| col().w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).scroll_x().child(el);
    if let Some(e) = s.tickets.edits.kind.as_ref().filter(|e| e.kind == v.kind && e.lines.is_some()) {
        part = col().w_full().gap(8.0).child(part).child(box_of(crate::edit::lines_in_place(e, &v.program, k))).child(crate::edit::kind_bar(s, e, crate::edit::kind_status(e, k), k));
    } else if !spans.is_empty() {
        part = col().w_full().gap(8.0).child(part).child(box_of(text(v.program.trim_end().to_string()).font(k.ty.mono).color(k.c.text_base).spans(spans).key("docket-thread-text"))).child(edit_lines(s, &v.kind, &v.program, k));
    }
    // every handler of its kind, with the runs each made under it (0.3 W3, N-1446 p16)
    if let Some(rows) = crate::ticket::page_pic(s, p, v, k, |pic| pic.handler_rows(v, &v.id)) {
        part = col().w_full().gap(16.0).child(part).child(rows);
    }
    c.child(beside(s, p, v, part, k))
}

/// PART, a picture of ticket V's program, with the panel of a stage or step pressed in it beside it (ticket.rs
/// `side_panel`): the Thread scale's, and the whole program's opened apart from the ticket's page (popup.rs).
pub(crate) fn beside(s: &App, p: &Proj, v: &coord_types::TicketView, part: El, k: &Kit) -> El {
    match crate::ticket::side_panel(s, p, v, k) {
        Some(panel) => row().w_full().gap(20.0).items_start().child(col().w(0.0).grow(3.0).min_w(0.0).child(part)).child(col().w(0.0).grow(2.0).min_w(0.0).child(panel)),
        None => part,
    }
}

/// Under a kind's text, the lines a press lit in it (picture.rs `lit_lines`) to edit in place (p294), when TEXT is the
/// kind's as coordd reads it now; a thread's program of an earlier text says where its lines are edited.
fn edit_lines(s: &App, kind: &str, text_of: &str, k: &Kit) -> El {
    let Some(lines) = crate::picture::lit_lines(&s.tickets.pic_open, kind, text_of) else { return col() };
    let now = match &s.tickets.kinds {
        Some(Ok(ks)) => ks.iter().find(|f| f.kind == kind).map(|f| f.text.clone()),
        _ => None,
    };
    if now.as_deref() != Some(text_of) {
        return text(format!("Older {kind}; edit it in Project.")).font(k.ty.small).color(k.c.text_muted).wrap();
    }
    let (kind, words) = (kind.to_string(), text_of.to_string());
    let label = if lines.0 == lines.1 { "Edit this line".to_string() } else { "Edit these lines".to_string() };
    row().w_full().justify_end().child(Button::new(label).variant(ButtonVariant::Contrast).on_press(move |s: &mut App| crate::edit::start_kind_lines(s, &kind, &words, lines)).build(k).key("edit-lines"))
}

/// Kind F as a card: its name and letter, form, result, raises and bound; its text under Show text.
fn card(s: &App, f: &KindForm, k: &Kit) -> El {
    let line = |label: &str, t: String| {
        row()
            .w_full()
            .gap(8.0)
            .child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_muted).w(64.0))
            .child(text(t).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink())
    };
    let open = s.kinds_open.contains(&f.kind);
    let kind = f.kind.clone();
    let head = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(f.kind.clone()).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(f.letter.as_deref().map(|l| format!("{l}-1, {l}-2, …")).unwrap_or_default()).font(k.ty.small).color(k.c.text_faint).grow(1.0).shrink())
        // its last live edit (p294)
        .child(text(crate::edit::marks(&s.tickets).kinds_at.get(&f.kind).map(|w| format!("edited {}", crate::when::at(w))).unwrap_or_default()).font(k.ty.small).color(k.c.text_faint).key(format!("kind-{}-edited", f.kind)))
        .child({
            let kind = f.kind.clone();
            Button::new("Edit kind")
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut App| {
                    s.kinds_open.insert(kind.clone());
                    crate::edit::start_kind(s, &kind, None);
                })
                .build(k)
        })
        .child(
            Button::new(if open { "Hide text" } else { "Show text" })
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut App| {
                    if !s.kinds_open.remove(&kind) {
                        s.kinds_open.insert(kind.clone());
                    }
                })
                .build(k),
        );
    let form = if f.form.is_empty() { "nothing".to_string() } else { f.form.iter().map(|(n, t)| format!("{n}: {t}")).collect::<Vec<_>>().join(", ") };
    let mut c = col()
        .key(format!("kind-{}", f.kind))
        .w_full()
        .gap(6.0)
        .padding(14.0)
        .radius(k.radius.md)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_weak)
        .child(head)
        .child(line("form", form))
        .child(line("result", f.result.as_ref().map_or_else(|| "—".to_string(), |t| t.to_string())));
    if !f.raises.is_empty() {
        c = c.child(line("raises", f.raises.join(", ")));
    }
    if let Some(b) = f.bound {
        c = c.child(line("steps", if b == 0 { "none by a model: held until someone acts".to_string() } else { if f.per_round { format!("at most {b} per round") } else { format!("at most {b}") } }));
    }
    if let Some(e) = s.tickets.edits.kind.as_ref().filter(|e| e.kind == f.kind && e.lines.is_none() && e.binding.is_none()) {
        return c.child(crate::edit::kind_editor(s, e, k));
    }
    // lines a press lit, being edited in place (p294)
    let in_place = s.tickets.edits.kind.as_ref().filter(|e| e.kind == f.kind && e.lines.is_some());
    if open || in_place.is_some() {
        // its text, the source, and beside it its picture, drawn from it
        let words = |x: &serde_json::Value| crate::texts::words(&s.tickets, x);
        let pic = crate::picture::Pic { k, p: s.proj(), kinds: crate::picture::kind_texts(&s.tickets), tickets: Vec::new(), open: &s.tickets.pic_open, marks: crate::edit::marks(&s.tickets), words: &words, pick: None, picked: None };
        let shown = match in_place {
            Some(e) => crate::edit::lines_in_place(e, &f.text, k),
            // the lines a press in its picture lit (p223)
            None => text(f.text.trim_end().to_string()).font(k.ty.mono).color(k.c.text_base).spans(crate::picture::lit_spans(&s.tickets.pic_open, &f.kind, &f.text, k)).key(format!("kind-{}-text", f.kind)),
        };
        c = c.child(
            row()
                .w_full()
                .gap(16.0)
                .child(col().w(0.0).grow(1.0).min_w(0.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).scroll_x().child(shown))
                .child(col().w(0.0).grow(1.0).min_w(0.0).scroll_x().child(pic.kind_into(&f.text, None, &crate::picture::bindings_of(&f.bindings), &format!("kind-{}", f.kind)).w_min_content())),
        );
        c = match in_place {
            Some(e) => c.child(crate::edit::kind_bar(s, e, crate::edit::kind_status(e, k), k)),
            None => c.child(edit_lines(s, &f.kind, &f.text, k)),
        };
        // a step pressed in its picture: its call, here under it
        if crate::picture::docket::picked(&s.tickets.pic_open, crate::picture::docket::CALL_IN) == Some(f.kind.as_str()) {
            if let Some(panel) = crate::edit::call_panel(s, k) {
                c = c.child(panel);
            }
        }
    }
    c
}

/// A kinds file (a draft not installed: the scene's YAAIHH_PICTURE_FILE) drawn: each kind's text beside its picture,
/// the bindings that open it on its edge.
fn file_card(s: &App, path: &str, k: &Kit) -> El {
    let t = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return text(format!("{path}: {e}")).font(k.ty.small).color(k.c.danger.fg),
    };
    let (kinds, bindings) = match crate::picture::parse_file(path, &t) {
        Ok(x) => x,
        Err(e) => return text(format!("{path}: {e}")).font(k.ty.small).color(k.c.danger.fg).wrap(),
    };
    let words = |x: &serde_json::Value| crate::texts::words(&s.tickets, x);
    let texts: Vec<(String, String)> = kinds.iter().map(|kd| (kd.name.clone(), kd.text.clone())).collect();
    let pic = crate::picture::Pic { k, p: s.proj(), kinds: texts, tickets: Vec::new(), open: &s.tickets.pic_open, marks: crate::edit::marks(&s.tickets), words: &words, pick: None, picked: None };
    let mut c = col().key("picture-file").w_full().gap(18.0).child(text(format!("{}: drawn from the file, not installed", path.rsplit('/').next().unwrap_or(path))).font(k.ty.ui_medium).color(k.c.text_base));
    // a kind's text runs to the next kind's: the comments above that one are not its own
    let own = |t: &str| {
        let ls: Vec<&str> = t.trim_end().lines().collect();
        let end = ls.iter().rposition(|l| { let l = l.trim(); !l.is_empty() && !l.starts_with("//") }).map_or(0, |i| i + 1);
        ls[..end].join("\n")
    };
    for kd in &kinds {
        let into: Vec<_> = bindings.iter().filter(|b| b.kind == kd.name).cloned().collect();
        if let Some(e) = s.tickets.edits.kind.as_ref().filter(|e| e.kind == kd.name && e.binding.is_none()) {
            c = c.child(col().key(format!("file-kind-{}", kd.name)).w_full().padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(crate::edit::kind_editor(s, e, k)));
            continue;
        }
        let (name, file) = (kd.name.clone(), path.to_string());
        let edit = row().w_full().justify_end().child(Button::new("Edit kind").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::edit::start_kind(s, &name, Some(&file))).build(k));
        c = c.child(
            col().key(format!("file-kind-{}", kd.name)).w_full().gap(6.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(edit).child(
                row()
                    .w_full()
                    .gap(16.0)
                    .child(col().w(0.0).grow(1.0).min_w(0.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).scroll_x().child(text(own(&kd.text)).font(k.ty.mono).color(k.c.text_base)))
                    .child(col().w(0.0).grow(1.0).min_w(0.0).scroll_x().child(pic.file(std::slice::from_ref(kd), &into, "file").w_min_content())),
            ),
        );
    }
    // the bindings into kinds the file does not hold: an arrow from outside, into a kind drawn elsewhere
    let outside: Vec<_> = bindings.iter().filter(|b| !kinds.iter().any(|kd| kd.name == b.kind)).cloned().collect();
    if !outside.is_empty() {
        c = c.child(pic.bindings(&outside, "file-bindings"));
    }
    c
}

/// A kind as the open form needs it (`Request::Kinds`).
#[derive(Clone, Debug, Deserialize)]
pub struct KindForm {
    pub kind: String,
    #[serde(default)]
    pub letter: Option<String>,
    pub form: Vec<(String, Type)>,
    #[serde(default)]
    pub result: Option<Type>,
    #[serde(default)]
    pub raises: Vec<String>,
    /// The most steps a ticket of it takes (docket-machine's cost bound).
    #[serde(default)]
    pub bound: Option<u64>,
    /// BOUND is one round's, not a total: an `iterate` with no `up to` (docket-core, N-147).
    #[serde(default)]
    pub per_round: bool,
    #[serde(default)]
    pub text: String,
    /// Its `///` doc, and its parts' (form fields, results, exceptions) by name; empty from a coordd before them.
    #[serde(default)]
    pub doc: String,
    #[serde(default)]
    pub docs: std::collections::BTreeMap<String, String>,
    /// Where it is written, whose own program it is (`kinds/agents/NAME.kind`; none: the project's), and the bindings
    /// that open it as written (0.2 line 26): what the Docket draws what starts it from. None from a coordd before them.
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub bindings: Vec<String>,
    /// The calls its steps name, as coordd read them, with the model of a call coordd runs (paper-research N-1396
    /// p174, p235); none from a coordd before them.
    #[serde(default)]
    pub calls: Vec<CallForm>,
    /// Its last live edit, from the record (the last `Reload` line naming it); none from a coordd before it.
    #[serde(default)]
    pub edited: Option<Edited>,
}

/// A live edit of a kind or a call as the record has it (the last `Reload` line naming it): when, by whom, why, and
/// the commit it was read again after.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Edited {
    pub at: String,
    pub by: String,
    pub why: String,
    pub commit: Option<String>,
}

/// A call a kind's steps name, as coordd read it (the Kinds answer): what a step pressed on the Docket shows.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CallForm {
    pub name: String,
    pub file: String,
    pub line: usize,
    /// `call NAME(PARAMS) -> HEAD`, its `where` with it.
    pub sig: String,
    /// Each attribute line but the template (`task deliver`, `blind`): its first word and the rest.
    pub attrs: Vec<(String, String)>,
    pub template: Option<String>,
    pub doc: String,
    pub task: Option<String>,
    /// For a watcher's step with no template, the call coordd runs for it (`choose` for `next`).
    pub runs: Option<RunsCall>,
    /// Its task's model as the policy gives it, for a call coordd runs.
    pub model: Option<TaskModel>,
    /// Its last live edit, from the record.
    pub edited: Option<Edited>,
}

/// The call coordd runs for a watcher's step (`CallForm::runs`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct RunsCall {
    pub name: String,
    pub template: Option<String>,
    /// Its last live edit, from the record.
    pub edited: Option<Edited>,
}

/// A task's model as the policy gives it: the model and its effort, or why there is none.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct TaskModel {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub why: Option<String>,
}

impl CallForm {
    /// Call D as the files here have it (a coordd before the Kinds answer's calls): no task model, no call it runs.
    pub fn of(d: &docket_machine::syntax::CallDef) -> CallForm {
        CallForm {
            name: d.sig.name.clone(),
            file: d.file.clone(),
            line: d.line,
            sig: d.written.clone(),
            attrs: d.attrs.clone(),
            template: d.template.clone(),
            doc: d.doc.text.clone(),
            task: d.attr("task").map(String::from),
            runs: None,
            model: None,
            edited: None,
        }
    }
}

/// Call NAME as the Kinds answer gives it, from the first kind whose steps name it.
pub fn said_call<'a>(s: &'a App, name: &str) -> Option<&'a CallForm> {
    s.tickets.kinds.as_ref()?.as_ref().ok()?.iter().flat_map(|k| &k.calls).find(|c| c.name == name)
}

/// The model a step of call F runs on, owed by BY as the picture names it (paper-research N-1396 p235): the watcher's,
/// coordd runs it on its task's model as the policy gives it; his, none; an agent's, its own, and which it is now when
/// BY is one agent. None when nothing says who owes it and F is not coordd's.
pub fn model_words(p: Option<&Proj>, f: &CallForm, by: Option<&str>) -> Option<String> {
    let task = f.task.as_deref().unwrap_or(&f.name);
    let decided = || match &f.model {
        Some(TaskModel { model: Some(m), effort, .. }) => format!("task {task} · {m}{}", effort.as_ref().map(|e| format!(" · effort {e}")).unwrap_or_default()),
        Some(TaskModel { why: Some(w), .. }) => format!("task {task} · no model: {w}"),
        _ => format!("task {task} · model loading"),
    };
    match by {
        Some(w) if w == docket_machine::WATCHER => Some(decided()),
        Some("owner") => Some("You answer it: it runs on no model.".into()),
        Some(w) => Some(match p.and_then(|p| p.live.agents.iter().find(|a| a.name == w)).filter(|a| !a.model.is_empty()) {
            Some(a) => format!("the agent's own model · {w}: {} now", crate::projects::plain_model(&a.model)),
            None => "runs on the agent's own model".into(),
        }),
        None => f.model.is_some().then(decided),
    }
}

/// The kinds a person opens: those no other kind opens, that have a form (a ticket about nothing given is the
/// engine's: `base`, a binding's `build`), and not `thread` (the composer opens those). The rest the program opens itself.
pub fn owners_kinds(kinds: &[KindForm]) -> Vec<&KindForm> {
    let opened: HashSet<String> = kinds
        .iter()
        .flat_map(|k| {
            let code: String = k.text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
            let words: Vec<String> = code.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| !w.is_empty()).map(str::to_string).collect();
            let me = k.kind.clone();
            words.windows(3).filter_map(move |w| match (w[0].as_str(), w[1].as_str()) {
                ("open", "fresh") => Some(w[2].clone()),
                ("open", x) if x != me => Some(x.to_string()),
                _ => None,
            }).collect::<Vec<_>>()
        })
        .collect();
    kinds.iter().filter(|k| !opened.contains(&k.kind) && !k.form.is_empty() && k.kind != "thread").collect()
}

/// A kind's doc's first line, else its name.
fn kind_line(f: &KindForm) -> String {
    f.doc.lines().map(str::trim).find(|l| !l.is_empty()).map_or_else(|| f.kind.clone(), str::to_string)
}

/// What a value of type T is, in words, for a field's hint; none for words (a Text field needs none).
pub(crate) fn type_hint(t: &Type) -> Option<String> {
    Some(match t {
        Type::Text => return None,
        Type::Opt(t) => return Some(type_hint(t).map_or_else(|| "optional".to_string(), |h| format!("{h}, optional"))),
        Type::Agent => "an agent's name".into(),
        Type::Ref => "a commit: repository and hash, as a 1a2b3c4".into(),
        Type::Ticket => "a ticket's id, as C-1".into(),
        Type::Int => "a number".into(),
        Type::Bool => "yes or no".into(),
        Type::List(t) => format!("one or more, each {}", type_hint(t).unwrap_or_else(|| "words".into())),
        _ => "written as the program's value (JSON)".into(),
    })
}

/// The open form (the stand-in's way in for the owner, paper-research N-101 p889): first what kind of ticket, each by
/// its doc's first line, the ones a person opens shown and the rest folded; then the kind's doc, its fields by their
/// docs, and a first post folded; Open. DOWN: whether coordd's pipe is down (`waiting_words`).
pub(crate) fn open_form(pg: &Page, down: bool, kind: &str, k: &Kit) -> El {
    let mut c = col().w_full().px(24.0).py(16.0).gap(12.0).key("open-form");
    c = c.child(text("Open a ticket").font(k.ty.body_medium).color(k.c.text_base));
    let kinds = match &pg.kinds {
        Some(Ok(ks)) => ks,
        Some(Err(e)) => return c.child(text(format!("The kinds could not be read: {e}")).font(k.ty.small).color(k.c.danger.fg).wrap()),
        None => return c.child(text(waiting_words(down)).font(k.ty.small).color(k.c.text_muted).wrap()),
    };
    let mine = owners_kinds(kinds);
    let all = pg.open_folds.contains("kinds-all");
    let picked = kinds.iter().find(|f| f.kind == kind);
    // a kind picked shows as one line, what it is; the list only while another is being picked
    if let (Some(f), false) = (picked, pg.open_folds.contains("kinds")) {
        let mut r = row().w_full().gap(8.0).center_y().child(text(kind_line(f)).font(k.ty.ui_medium).color(k.c.text_base).truncate().shrink());
        if !f.doc.trim().is_empty() {
            r = r.child(text(f.kind.clone()).font(k.ty.small).color(k.c.text_faint));
        }
        r = r.child(row().grow(1.0)).child(k.interactive(row().child(text("› another kind").font(k.ty.small).color(k.c.text_accent))).on_press(|s: &mut App| {
            s.tickets.open_folds.insert("kinds".into());
        }));
        return fields_part(pg, f, kind, c.child(r), k);
    }
    let shown: Vec<&KindForm> = if all { kinds.iter().filter(|f| f.kind != "thread").collect() } else { mine.clone() };
    // each kind as a row: what it is in words, its name small after it; the picked one marked
    let mut list = col().w_full().gap(2.0);
    for f in shown {
        let on = f.kind == kind;
        let name = f.kind.clone();
        let r = row()
            .key(format!("open-kind-{}", f.kind))
            .w_full()
            .gap(8.0)
            .px(10.0)
            .py(6.0)
            .center_y()
            .radius(k.radius.sm)
            .fill(if on { k.c.layer_02 } else { k.c.bg_base })
            .child(text(kind_line(f)).font(if on { k.ty.ui_medium } else { k.ty.ui }).color(k.c.text_base).truncate().shrink())
            .children((!f.doc.trim().is_empty()).then(|| text(f.kind.clone()).font(k.ty.small).color(k.c.text_faint)));
        list = list.child(k.interactive(r).on_press(move |s: &mut App| {
            if s.tickets.opening.as_deref() != Some(name.as_str()) {
                s.tickets.opening = Some(name.clone());
                s.tickets.fields.clear();
            }
            s.tickets.open_folds.remove("kinds");
        }));
    }
    c = c.child(list);
    if mine.len() < kinds.len() {
        c = c.child(k.interactive(row().child(text(if all { "⌄ only the kinds a person opens" } else { "› the kinds the program opens itself" }).font(k.ty.small).color(k.c.text_accent))).on_press(|s: &mut App| {
            if !s.tickets.open_folds.remove("kinds-all") {
                s.tickets.open_folds.insert("kinds-all".into());
            }
        }));
    }
    match picked {
        Some(f) => fields_part(pg, f, kind, c, k),
        None => c,
    }
}

/// The open form below its kind: the rest of the kind's doc, its fields by their docs, a first post folded, Open.
fn fields_part(pg: &Page, f: &KindForm, kind: &str, mut c: El, k: &Kit) -> El {
    // the kind picked: the rest of its doc, then its fields
    let rest: Vec<&str> = f.doc.lines().map(str::trim).filter(|l| !l.is_empty()).skip(1).collect();
    if !rest.is_empty() {
        c = c.child(text(rest.join(" ")).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    for (name, t) in &f.form {
        let edit = pg.fields.get(name).cloned().unwrap_or_else(|| TextEdit::new(""));
        let nk = name.clone();
        let mut head = col().w_full().gap(2.0).child(text(name.replace('_', " ")).font(k.ty.small_medium).color(k.c.text_base));
        let doc = f.docs.get(name).map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
        let hint = type_hint(t);
        if let Some(d) = doc.iter().chain(hint.iter()).cloned().reduce(|a, b| format!("{a} ({b})")) {
            head = head.child(text(d).font(k.ty.small).color(k.c.text_muted).wrap());
        }
        let el = if is_text(t) {
            TextArea::new(&edit).width(None).auto_grow(3, 12).on_edit(move |s: &mut App| s.tickets.fields.entry(nk.clone()).or_insert_with(|| TextEdit::new(""))).build(k)
        } else {
            TextInput::new(&edit).width(None).on_edit(move |s: &mut App| s.tickets.fields.entry(nk.clone()).or_insert_with(|| TextEdit::new(""))).build(k)
        };
        c = c.child(col().key(format!("field-{kind}-{name}")).w_full().gap(6.0).child(head).child(el));
    }
    // a first post: rarely needed, folded until asked for
    let post_open = pg.open_folds.contains("first-post") || pg.fields.get("").is_some_and(|e| !e.text.is_empty());
    if post_open {
        let first = pg.fields.get("").cloned().unwrap_or_else(|| TextEdit::new(""));
        c = c.child(col().key(format!("field-{kind}-post")).w_full().gap(6.0).child(text("A first post on it").font(k.ty.small_medium).color(k.c.text_base)).child(TextArea::new(&first).width(None).auto_grow(2, 8).on_edit(|s: &mut App| s.tickets.fields.entry(String::new()).or_insert_with(|| TextEdit::new(""))).build(k)));
    } else {
        c = c.child(k.interactive(row().child(text("› add a first post").font(k.ty.small).color(k.c.text_accent))).on_press(|s: &mut App| {
            s.tickets.open_folds.insert("first-post".into());
        }));
    }
    let kind_ = kind.to_string();
    c.child(
        row()
            .w_full()
            .gap(8.0)
            .justify_end()
            .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.opening = None).build(k))
            .child(Button::new(format!("Open the {}", f.kind.replace('_', " "))).variant(ButtonVariant::Contrast).on_press(move |s: &mut App| open_it(s, &kind_)).build(k)),
    )
}

fn open_it(s: &mut App, kind: &str) {
    let mut form = Map::new();
    for (name, e) in &s.tickets.fields {
        if !name.is_empty() && !e.text.trim().is_empty() {
            form.insert(name.clone(), json!(e.text.trim()));
        }
    }
    let text = s.tickets.fields.get("").map(|e| e.text.trim().to_string()).filter(|t| !t.is_empty());
    s.request_in(s.cur, Request::Open { kind: kind.into(), under: None, form: Value::Object(form), text }, Pending::TicketDone { clear: Cleared::Open });
}

#[cfg(test)]
mod moved_tests {
    use super::*;
    /// The kinds as coordd sends them (`Request::Kinds`): a result is a type, not a string, and the Kinds page's
    /// fields come along.
    #[test]
    fn kinds_read_as_coordd_sends_them() {
        let out = r#"[{"kind":"check","letter":"K","form":[["commit","Ref"],["branch",{"Opt":"Ref"}]],"result":{"Variant":[["Green",[]],["Red",[["log","Text"]]]]},"raises":[],"bound":1,"text":"kind check\n"},
                      {"kind":"note","letter":"N","form":[],"result":{"Tuple":[]},"raises":[],"bound":0,"text":"kind note\n"}]"#;
        let ks: Vec<KindForm> = serde_json::from_str(out).unwrap();
        assert_eq!(ks[0].letter.as_deref(), Some("K"));
        assert_eq!(ks[0].bound, Some(1));
        assert!(ks[0].result.as_ref().unwrap().to_string().contains("Green"), "{:?}", ks[0].result);
        assert!(ks[1].form.is_empty());
    }

    /// Build 36 in his window (N-1446 p198): the kinds were asked on the pipe the install's restart of coordd ended,
    /// and the Docket said "Reading the kinds…" from then on. An ask the pipe's end loses, the kinds' or a text's, is
    /// asked again once the pipe is back; until then the page says why it holds no kinds; the answer is drawn.
    #[test]
    fn the_kinds_asked_on_a_pipe_that_ends_are_asked_again_once_it_is_back() {
        type Rt = rune::runtime::Runtime<App>;
        let frames = |rt: &mut Rt| {
            for _ in 0..4 {
                rt.frame(0.016);
            }
        };
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let asked = |s: &App| -> Vec<u64> { s.projects[0].pending.iter().filter(|(_, w)| matches!(w, Pending::TicketKinds)).map(|(id, _)| *id).collect() };
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&[]);
        s.sel = crate::Sel::Kinds;
        let mut rt: Rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        // the page asks the kinds; a post's words are on their way with them
        crate::texts::poll(&mut rt.state);
        assert_eq!(asked(&rt.state).len(), 1);
        // asked of the project shown, which the cloud's cards name it by (cloud.rs `whose`, paper-research N-1446 p1844)
        assert_eq!(rt.state.tickets.kinds_of.as_deref(), Some(rt.state.projects[0].id.as_str()));
        rt.state.projects[0].pending.insert(1_000, Pending::TicketText { key: "words".into() });
        rt.state.tickets.texts.insert("words".into(), None);
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == "Reading the kinds…"));
        // coordd restarts (an install): the pipe ends before the answer, and the bridge started again finds no coordd yet
        let why = "the pipe ended (3): coord: coordd is not running (/run/user/1000/coordd/research.sock: No such file or directory)";
        // a pipe that is down takes no ask
        let mut link = None;
        rt.update(|s, _| {
            s.take(0, crate::link::Incoming::Down(why.into()));
            link = s.projects[0].link.take();
            crate::texts::poll(s);
        });
        assert!(!rt.state.tickets.kinds_asked && !rt.state.tickets.texts.contains_key("words") && rt.state.projects[0].pending.is_empty());
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Disconnected; kinds load on reconnect.") && !t.iter().any(|x| x == "Reading the kinds…"), "{t:?}");
        // the pipe back (coordd's first event clears `down`): the kinds asked again, and drawn when they come
        rt.update(|s, _| {
            s.projects[0].link = link.take();
            s.projects[0].down = None;
            crate::texts::poll(s);
        });
        let id = asked(&rt.state);
        assert_eq!(id.len(), 1);
        let out = r#"[{"kind":"note","letter":"N","form":[],"result":{"Tuple":[]},"raises":[],"bound":0,"text":"kind note\n"}]"#;
        rt.update(|s, _| {
            s.take(0, crate::link::Incoming::Reply(id[0], coord_types::Reply { ok: true, out: out.into(), post: None }));
        });
        assert!(matches!(&rt.state.tickets.kinds, Some(Ok(ks)) if ks.len() == 1));
        frames(&mut rt);
        assert!(!texts(&rt).iter().any(|x| x == "Reading the kinds…" || x.starts_with("The kinds are read once")));
    }

    /// p1859: the Kinds page asked the kinds once, of the project shown then, and drew those under every project he
    /// changed to after. Changed to another, it asks that one's pipe and says it reads them until they come, the first's
    /// no longer drawn; an answer of a project no longer shown, come late, is not taken as the one's shown.
    #[test]
    fn the_kinds_page_after_a_change_of_project_asks_the_project_shown_for_its_kinds() {
        type Rt = rune::runtime::Runtime<App>;
        let frames = |rt: &mut Rt| {
            for _ in 0..4 {
                rt.frame(0.016);
            }
        };
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let asked = |s: &App, pi: usize| -> Vec<u64> { s.projects[pi].pending.iter().filter(|(_, w)| matches!(w, Pending::TicketKinds)).map(|(id, _)| *id).collect() };
        let kinds = |name: &str, letter: &str| format!(r#"[{{"kind":"{name}","letter":"{letter}","form":[],"result":{{"Tuple":[]}},"raises":[],"bound":0,"text":"kind {name}\n"}}]"#);
        let answer = |rt: &mut Rt, pi: usize, id: u64, out: String| {
            rt.update(|s, _| {
                s.take(pi, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None }));
            });
            for _ in 0..4 {
                rt.frame(0.016);
            }
        };
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&[]);
        let mut q = crate::Proj::new(&crate::projects::ProjectConfig { name: "tovrin-dev".into(), id: "tovrin-dev".into(), boxed: false }, Vec::new());
        q.down = None;
        let mut cat = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        q.link = Some(crate::link::Link::to_child(cat.stdin.take().unwrap()));
        s.projects.push(q);
        s.sel = crate::Sel::Kinds;
        let mut rt: Rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        // research shown: its kinds asked and drawn
        crate::texts::poll(&mut rt.state);
        let id = asked(&rt.state, 0);
        assert_eq!(id.len(), 1);
        answer(&mut rt, 0, id[0], kinds("note", "N"));
        assert!(texts(&rt).iter().any(|x| x == "note"), "research's kind drawn: {:?}", texts(&rt));
        // he changes to tovrin-dev, as its row in the sidebar does: asked of its pipe, research's no longer drawn
        rt.update(|s, _| {
            s.cur = 1;
            crate::texts::poll(s);
        });
        let late = asked(&rt.state, 1);
        assert_eq!(late.len(), 1, "asked of tovrin-dev");
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Reading the kinds…") && !t.iter().any(|x| x == "note"), "{t:?}");
        // back to research before tovrin-dev's answer: research's asked again, tovrin-dev's come late not taken for it
        rt.update(|s, _| {
            s.cur = 0;
            crate::texts::poll(s);
        });
        let again = asked(&rt.state, 0);
        assert_eq!(again.len(), 1, "asked of research again");
        answer(&mut rt, 1, late[0], kinds("part", "P"));
        assert!(rt.state.tickets.kinds.is_none() && !texts(&rt).iter().any(|x| x == "part"), "tovrin-dev's kinds not drawn under research");
        answer(&mut rt, 0, again[0], kinds("note", "N"));
        assert!(texts(&rt).iter().any(|x| x == "note"));
        // tovrin-dev shown again: its own asked, and drawn
        rt.update(|s, _| {
            s.cur = 1;
            crate::texts::poll(s);
        });
        let id = asked(&rt.state, 1);
        assert_eq!(id.len(), 1);
        answer(&mut rt, 1, id[0], kinds("part", "P"));
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "part") && !t.iter().any(|x| x == "note"), "tovrin-dev's kind drawn, not research's: {t:?}");
    }
}
