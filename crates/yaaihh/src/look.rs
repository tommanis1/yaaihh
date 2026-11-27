//! The window's look applied (coord_types::look; paper-research N-866, N-412 post 1107): the colours and sizes of
//! the flavour's `look.toml`, as coordd sends it, put into the one Kit the window draws each frame with (lib.rs `view`),
//! so a look that arrives is drawn in the next frame, with no new window. One look for the whole window (N-866.p6, 6):
//! the first project whose coordd sends one.
//!
//! The look on the Kinds page, where kinds are edited (N-866.p6, 2-5; N-1104 p113): one line says who changed it last,
//! when and on which thread, with the way back beside it ("Put back the look from 10:52"); Edit look opens its text as
//! coordd has it (`Sources`), the control drawn beside it from the text as it stands, so the mark is seen before it is
//! saved. Save is the act (agent-base's text-change, coordd change.rs): checked as the window checks a look, committed
//! and loaded together, or nothing changed and the refusal under the editor. The way back is the same act with the
//! last change's commit.

use crate::{App, Pending, Proj};
use coord_types::Request;
use coord_types::look::{Look, Rgb};
use docket_machine::TicketView;
use docket_machine::view::ViewStatus;
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, TextArea, TextInput};
use serde_json::{Value, json};

/// The look the window draws with: the first project's that has one, else none (the built-in look).
pub fn of(s: &App) -> Option<&Look> {
    s.projects.iter().map(|p| &p.live.look).find(|l| **l != Look::default())
}

/// A row of the threads list's MARK, LABEL and ID in the order the window's look gives them (`[list] row`, each once).
pub fn row_fields(s: &App, mark: El, label: El, id: El) -> Vec<El> {
    let order = of(s).map_or_else(|| coord_types::look::List::default().row, |l| l.list.row.clone());
    let mut parts = [Some(mark), Some(label), Some(id)];
    order.iter().filter_map(|f| parts[*f as usize].take()).collect()
}

/// What the window says for KEY, in the window's look (coord_types::look::say): each text of `WORDS` is drawn through
/// it where the page draws it, so its wording is live (N-1104 p113, head 4).
pub fn say(s: &App, key: &str, args: &[(&str, &str)]) -> String {
    coord_types::look::say(of(s).unwrap_or(&Look::default()), key, args)
}

fn colour(c: Rgb) -> rune::Color {
    rune::color::hex(&format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).unwrap_or(rune::Color::TRANSPARENT)
}

/// The window's kit in DARK or light: the neutral kit with the flavour's look over it.
pub fn kit(s: &App, dark: bool) -> Kit {
    let mut k = Kit::neutral(dark);
    if let Some(l) = of(s) {
        apply(&mut k, l);
    }
    k
}

/// Put L's colours (for K's mode) and sizes into K. Each role sets every token the pages draw that role with.
pub fn apply(k: &mut Kit, l: &Look) {
    let t = l.colour.of(k.dark);
    let c = &mut k.c;
    if let Some(x) = t.page.map(colour) {
        (c.page, c.bg_base, c.surface_base) = (x, x, x);
    }
    if let Some(x) = t.read.map(colour) {
        (c.text_base, c.icon_base, c.text_strong) = (x, x, x);
    }
    if let Some(x) = t.support.map(colour) {
        (c.text_muted, c.icon_muted, c.text_weak) = (x, x, x);
    }
    if let Some(x) = t.quiet.map(colour) {
        (c.text_faint, c.icon_faint) = (x, x);
    }
    if let Some(x) = t.layer.map(colour) {
        (c.layer_01, c.layer_02, c.surface_raised) = (x, x, x);
    }
    if let Some(x) = t.border.map(colour) {
        (c.border_muted, c.border_base, c.border_weak) = (x, x, x);
    }
    if let Some(x) = t.attention.map(colour) {
        (c.danger.fg, c.danger.icon, c.text_accent, c.icon_accent) = (x, x, x, x);
    }
    let size = |st: &mut rune::TextStyle, n: f32| {
        if st.line_height > 0.0 {
            st.line_height *= n / st.size;
        }
        st.size = n;
    };
    if let Some(n) = l.size.body {
        size(&mut k.ty.body, n);
        size(&mut k.ty.body_medium, n);
    }
    if let Some(n) = l.size.small {
        size(&mut k.ty.small, n);
        size(&mut k.ty.small_medium, n);
    }
}

// ---- the look on the Kinds page (N-866.p6, 2-5) ---------------------------------------------------------------------

/// The kind whose act changes the look (agent-base kinds/text-change.kind), and the look's file as the act names it.
const KIND: &str = "text-change";
const FILE: &str = "look.toml";

/// The look as he edits it on the Kinds page.
#[derive(Default)]
pub struct Edit {
    /// The look being edited; none while no edit is under way.
    pub open: Option<Open>,
    /// The why its Save commits with.
    pub why: TextEdit,
    /// The text-change a Save or the way back opened, by its project's id and its own, until it ends ([`poll`]).
    pub change: Option<(String, String)>,
    /// What the last Save or way back came to: Ok, live; Err, refused or not sent. Under the editor while it is
    /// open, under the card's line else (N-866.p6, 5).
    pub said: Option<Result<String, String>>,
}

/// The look's file being edited: its text as coordd served it (none: there is no look.toml yet), and the text.
pub struct Open {
    pub orig: Option<String>,
    pub text: TextEdit,
}

/// What the look asked the project's coordd: its file, for the editor (`Sources`); or the text-change a Save or the
/// way back opens (`Open`).
pub enum Asked {
    Sources,
    Opened,
}

/// `Sources`' look (coordd files.rs `sources`): its text, none while there is no file. No look at all: the project is
/// in no flavour's projects folder.
#[derive(serde::Deserialize)]
struct Served {
    #[serde(default)]
    look: Option<Served1>,
}

#[derive(serde::Deserialize)]
struct Served1 {
    #[serde(default)]
    text: Option<String>,
}

/// A change of the look the record holds: a text-change whose check answered Look and whose load made it live; BY who
/// opened it, AT when it went live, COMMIT what its way back names.
pub struct Change<'a> {
    pub v: &'a TicketView,
    pub by: String,
    pub at: String,
    pub commit: String,
}

/// The look's changes in P's record, oldest first: each load of a text-change whose check answered Look. A way back
/// is one too, and so is an agent's Wrong taking its own change back.
pub fn changes(p: &Proj) -> Vec<Change<'_>> {
    let mut out = Vec::new();
    for v in p.live.tickets.values().filter(|v| v.kind == KIND) {
        let mut by = None;
        for (post, h) in v.posts.iter().filter(|x| x.by == "coord").filter_map(|x| Some((x, x.head.as_ref()?))) {
            match h["head"].as_str() {
                Some("Look") => by = h["by"].as_str().map(str::to_string),
                Some("Live") => {
                    if let (Some(by), Some(c)) = (by.take(), h["commit"].as_str()) {
                        out.push(Change { v, by, at: post.at.clone(), commit: c.to_string() });
                    }
                }
                _ => by = None,
            }
        }
    }
    out.sort_by(|a, b| a.at.cmp(&b.at));
    out
}

/// The way back from the last of CHANGES, named for what it does (N-866.p6, 2): the look of the change before it,
/// "Put back the look from 10:52"; with none before it in the record, the look from before the last one.
fn back_words(changes: &[Change]) -> Option<String> {
    let (last, before) = changes.split_last()?;
    Some(match before.last() {
        Some(b) => format!("Put back the look from {}", crate::when::at(&b.at)),
        None => format!("Put back the look from before {}", crate::when::at(&last.at)),
    })
}

/// The text-change that makes TEXT the look, WHY its commit's words: his Save.
fn save_request(text: &str, why: &str) -> Request {
    Request::Open { kind: KIND.into(), under: None, form: json!({"files": [{"path": FILE, "text": text}], "back": null, "why": why}), text: None }
}

/// The text-change that takes COMMIT back, its files as they were before it: the way back.
fn back_request(commit: &str, why: &str) -> Request {
    Request::Open { kind: KIND.into(), under: None, form: json!({"files": [], "back": commit, "why": why}), text: None }
}

/// The look TEXT makes, checked as coordd's text-change checks it before anything is written (change.rs
/// `look_checks`: TOML, then `coord_types::look::check`): the look, or each fault.
pub fn read(text: &str) -> Result<Look, Vec<String>> {
    let v: Value = toml::from_str(text).map_err(|e| vec![format!("it does not read as TOML: {}", e.message())])?;
    coord_types::look::check(&v)
}

/// Edit look: its file asked of the current project's coordd, which serves it beside the kinds (`Sources`); the
/// editor opens with it ([`came`]).
pub fn start(s: &mut App) {
    s.tickets.edits.look.said = None;
    if !s.request_in(s.cur, Request::Sources, Pending::Look(Asked::Sources)) {
        s.tickets.edits.look.said = Some(Err("Not read: disconnected.".into()));
    }
}

/// Send REQ, a text-change, to the current project's coordd; [`came`] takes the ticket it opens.
fn open(s: &mut App, req: Request) {
    s.tickets.edits.look.said = None;
    if !s.request_in(s.cur, req, Pending::Look(Asked::Opened)) {
        s.tickets.edits.look.said = Some(Err("Not sent: disconnected.".into()));
    }
}

fn save(s: &mut App) {
    let e = &s.tickets.edits.look;
    let Some(o) = &e.open else { return };
    let req = save_request(&o.text.text, e.why.text.trim());
    open(s, req);
}

/// A reply to what the look asked the coordd of project PI.
pub fn came(s: &mut App, pi: usize, what: Asked, ok: bool, out: String) {
    let project = s.projects.get(pi).map(|p| p.id.clone());
    let e = &mut s.tickets.edits.look;
    match what {
        Asked::Sources => match serde_json::from_str::<Served>(&out) {
            _ if !ok => e.said = Some(Err(out.trim().to_string())),
            Ok(Served { look: Some(f) }) => e.open = Some(Open { text: TextEdit::new(f.text.clone().unwrap_or_default()), orig: f.text }),
            Ok(Served { look: None }) => e.said = Some(Err("No look.toml for this project.".into())),
            Err(x) => e.said = Some(Err(format!("Unreadable files: {x}"))),
        },
        Asked::Opened => match out.trim().strip_prefix("Opened ").and_then(|r| r.split('.').next()).filter(|_| ok) {
            Some(id) => e.change = project.map(|p| (p, id.to_string())),
            None => e.said = Some(Err(out.trim().to_string())),
        },
    }
}

/// Each frame: the text-change a Save or the way back opened, once it ends: what it came to, said; a change made live
/// closes the editor. True when that changed what is drawn.
pub fn poll(s: &mut App) -> bool {
    let Some((project, id)) = &s.tickets.edits.look.change else { return false };
    let Some(v) = s.projects.iter().find(|p| p.id == *project).and_then(|p| p.live.tickets.values().find(|v| v.id == *id)) else { return false };
    let said = match &v.status {
        ViewStatus::Done { result } => ended(result),
        ViewStatus::Failed { .. } | ViewStatus::Stopped { .. } => Err(format!("{id} stopped early; see its thread.")),
        _ => return false,
    };
    let e = &mut s.tickets.edits.look;
    if said.is_ok() {
        (e.open, e.why) = (None, TextEdit::new(""));
    }
    (e.change, e.said) = (None, Some(said));
    true
}

/// What a text-change's RESULT says him (kinds/text-change.kind): live, with its commit; or why nothing changed, in
/// coordd's words.
fn ended(r: &Value) -> Result<String, String> {
    let commit = r["commit"].as_str().map(|c| &c[..c.len().min(7)]).unwrap_or_default();
    match r["head"].as_str() {
        Some("Live") => Ok(format!("Live, committed as {commit}.")),
        Some("Refused") => {
            let why = r["why"].as_str().unwrap_or("no reason given");
            Err(why.chars().take(1).flat_map(char::to_uppercase).chain(why.chars().skip(1)).collect())
        }
        _ => Err(format!("It ended as {}.", r["head"])),
    }
}

/// A still's look in its edit (lib.rs's `kinds` scene with YAAIHH_EDIT_LOOK=1): its text as the first project's
/// coordd serves it; YAAIHH_LOOK_TEXT=FROM=>TO that text with FROM replaced, as typed (`\n` a new line);
/// YAAIHH_LOOK_WHY the why typed; YAAIHH_LOOK_SAVE=1 Save pressed, the act opened for real, and the scene drawn once its
/// text-change has ended, as the live window's events bring it.
pub fn scene(s: &mut App) {
    let Some(p) = s.projects.first() else { return };
    let r = p.ask_once(&Request::Sources);
    came(s, 0, Asked::Sources, r.is_ok(), r.unwrap_or_else(|e| e));
    let typed = std::env::var("YAAIHH_LOOK_TEXT").ok();
    if let (Some(o), Some((from, to))) = (s.tickets.edits.look.open.as_mut(), typed.as_deref().and_then(|x| x.split_once("=>"))) {
        o.text = TextEdit::new(o.text.text.replacen(&from.replace("\\n", "\n"), &to.replace("\\n", "\n"), 1));
    }
    if let Ok(w) = std::env::var("YAAIHH_LOOK_WHY") {
        s.tickets.edits.look.why = TextEdit::new(w);
    }
    if !std::env::var("YAAIHH_LOOK_SAVE").is_ok_and(|v| v == "1") {
        return;
    }
    let Some(o) = &s.tickets.edits.look.open else { return };
    let req = save_request(&o.text.text, s.tickets.edits.look.why.text.trim());
    let r = s.projects[0].ask_once(&req);
    came(s, 0, Asked::Opened, r.is_ok(), r.unwrap_or_else(|e| e));
    while s.tickets.edits.look.change.is_some() {
        let got = crate::link::once(&s.projects[0].cmd, &[]);
        if got.is_empty() {
            break;
        }
        for m in got {
            if let crate::link::Incoming::Event(ev) = m {
                _ = s.projects[0].apply(ev);
            }
        }
        poll(s);
    }
}

/// The look on P's Kinds page: one line, who changed it last, when and on which thread (id plus label, a press to it),
/// the way back beside it, and Edit look; the editor under it while open, and what the last Save or way back came to.
pub fn card(s: &App, p: &Proj, k: &Kit) -> El {
    let e = &s.tickets.edits.look;
    let all = changes(p);
    let mut head = row().w_full().gap(8.0).center_y().child(text("The window's look").font(k.ty.ui_medium).color(k.c.text_base));
    head = match all.last() {
        Some(c) => {
            let n = c.v.n;
            let thread = k.interactive(crate::refs::el(&crate::refs::ticket(Some(p), c.v), k.ty.small, k.c.text_accent, k)).on_press(move |s: &mut App| crate::threads::show(s, n));
            let who = text(format!("changed by {} at {} on", crate::refs::person(&c.by), crate::when::at(&c.at))).font(k.ty.small).color(k.c.text_muted);
            head.child(row().key("look-changed").gap(6.0).center_y().grow(1.0).shrink().min_w(0.0).child(who).child(thread))
        }
        None => head.child(text("as look.toml has it").font(k.ty.small).color(k.c.text_faint).grow(1.0).shrink()),
    };
    if let (Some(words), Some(c)) = (back_words(&all), all.last()) {
        let (commit, why) = (c.commit.clone(), words.clone());
        head = head.child(Button::new(words).variant(ButtonVariant::Ghost).disabled(e.change.is_some()).on_press(move |s: &mut App| open(s, back_request(&commit, &why))).build(k).key("look-back"));
    }
    if e.open.is_none() {
        head = head.child(Button::new("Edit look").variant(ButtonVariant::Ghost).on_press(start).build(k).key("look-edit"));
    }
    let mut c = col().key("look-card").w_full().gap(10.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(head);
    if let Some(o) = &e.open {
        c = c.child(editor(e, o, k));
    }
    // the change under way, or what the last came to: under the editor (N-866.p6, 5)
    match (&e.change, &e.said) {
        (Some((_, id)), _) => c.child(row().key("look-said").gap(6.0).child(text("Checking, committing and loading it:").font(k.ty.small).color(k.c.text_muted)).child(crate::refs::el(&crate::refs::ticket_id(p, id), k.ty.small, k.c.text_base, k))),
        (None, Some(r)) => c.child(crate::message_box::answer_line(r, k).key("look-said")),
        (None, None) => c,
    }
}

/// The look's text editable, and beside it the control drawn from the text as it stands (N-866.p6, 3), or why it is
/// not drawn; under them the why, Discard and Save.
fn editor(e: &Edit, o: &Open, k: &Kit) -> El {
    let small = |t: &str, c: rune::Color| text(t.to_string()).font(k.ty.small).color(c).wrap();
    let read = read(&o.text.text);
    let area = TextArea::new(&o.text).width(None).spellcheck(false).spans(vec![rune::Span::new(0, o.text.text.len()).mono()]).auto_grow(8, 22).on_edit(|s: &mut App| &mut s.tickets.edits.look.open.as_mut().unwrap().text).build(k);
    let beside = match &read {
        Ok(l) => col().gap(6.0).child(small("Drawn from the text as it stands:", k.c.text_muted)).child(preview(l, k.dark)),
        Err(faults) => faults.iter().fold(col().gap(4.0).child(small("Not drawn while it does not check:", k.c.text_muted)), |c, f| c.child(small(&format!("✗ {f}"), k.c.danger.fg))),
    };
    let unchanged = o.orig.as_deref().unwrap_or_default() == o.text.text;
    let save = Button::new("Save and load")
        .variant(ButtonVariant::Contrast)
        .disabled(read.is_err() || unchanged || e.why.text.trim().is_empty() || e.change.is_some())
        .on_press(save)
        .build(k)
        .key("look-save");
    let why = TextInput::new(&e.why).placeholder("Why: the commit says it").width(None).on_edit(|s: &mut App| &mut s.tickets.edits.look.why).build(k);
    col()
        .w_full()
        .gap(10.0)
        .child(row().w_full().gap(16.0).items_start().child(col().key("look-text").w(0.0).grow(1.0).min_w(0.0).child(area)).child(col().w(0.0).grow(1.0).min_w(0.0).child(beside)))
        .child(col().key("look-why").w_full().child(why))
        .child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(small("Save checks, commits and applies it.", k.c.text_muted).grow(1.0).shrink())
                .child(Button::new("Discard").variant(ButtonVariant::Neutral).on_press(|s: &mut App| s.tickets.edits.look.open = None).build(k))
                .child(save),
        )
}

/// LOOK drawn in the window's own Kit for the mode DARK with LOOK in it, as the frame would be: the context circle
/// with its mark, the three text colours on the page, a card on the layer, the one accent.
fn preview(look: &Look, dark: bool) -> El {
    let mut pk = Kit::neutral(dark);
    apply(&mut pk, look);
    let k = &pk;
    let words = col()
        .gap(4.0)
        .child(text("What you read").font(k.ty.body).color(k.c.text_base))
        .child(text("who and when").font(k.ty.small).color(k.c.text_muted))
        .child(text("a hint").font(k.ty.small).color(k.c.text_faint));
    let card = row()
        .gap(8.0)
        .center_y()
        .padding(8.0)
        .radius(k.radius.sm)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .child(text("A card").font(k.ty.small).color(k.c.text_base))
        .child(text(coord_types::look::say(look, "waits_on_you", &[])).font(k.ty.small).color(k.c.danger.fg));
    col()
        .key("look-preview")
        .gap(12.0)
        .padding(16.0)
        .radius(k.radius.md)
        .fill(k.c.page)
        .border(1.0, k.c.border_base)
        .child(row().gap(14.0).center_y().child(crate::circle::sample(&look.context.mark, k)).child(words))
        .child(card)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A whole dark palette that checks (coord_types::look: a mode that gives a colour gives all seven), with READ.
    fn dark(read: &str) -> serde_json::Value {
        json!({"page": "#101114", "read": read, "support": "#a0a1a6", "quiet": "#6c6d72", "layer": "#1a1b1f", "border": "#2a2b30", "attention": "#e5484d"})
    }

    /// A look that arrives on the pipe is in the next frame's Kit: a colour and a size; one that does not check keeps
    /// the look before it, said in one plain line; none gives the built-in back.
    #[test]
    fn a_look_on_the_pipe_is_drawn_in_the_next_frame_and_a_broken_one_keeps_what_was_drawn() {
        // a look bumps version::LOOK, which every memo's version holds: not while a view_cost test counts builds
        let _one = crate::view_cost::one();
        let mut live = crate::live::Live::default();
        let built = Kit::neutral(true);
        let kit = |live: &crate::live::Live, dark: bool| {
            let mut k = Kit::neutral(dark);
            apply(&mut k, &live.look);
            k
        };
        let rev = crate::version::LOOK.load(std::sync::atomic::Ordering::Relaxed);
        live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(json!({"colour": {"dark": dark("#f0e6d2")}, "type": {"body": 16}})) });
        let k = kit(&live, true);
        assert_eq!(k.c.text_base, colour([0xf0, 0xe6, 0xd2]));
        assert_eq!(k.c.layer_01, colour([0x1a, 0x1b, 0x1f]));
        assert_eq!(k.ty.body.size, 16.0);
        assert_eq!(k.ty.small.size, built.ty.small.size, "what the look leaves out is the window's own");
        assert_eq!(kit(&live, false).c.text_base, Kit::neutral(false).c.text_base, "and so is a mode it gives no colour for");
        assert!(crate::version::LOOK.load(std::sync::atomic::Ordering::Relaxed) > rev, "every memoised part builds again");
        live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Look(json!({"colour": {"dark": dark("#111111")}})) });
        assert_eq!(kit(&live, true).c.text_base, colour([0xf0, 0xe6, 0xd2]), "a look he could not read is not taken");
        assert_eq!(live.refused().count(), 1, "one plain line: {:?}", live.look_refused);
        live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Look(serde_json::Value::Null) });
        assert_eq!(kit(&live, true).c.text_base, built.c.text_base);
        assert_eq!(live.refused().count(), 0);
    }

    /// The same look again builds nothing again (proto-window, N-1446 p1798: a text change's still sends the look once
    /// more to every window, his included): its revision and `version::LOOK` stay, so no memoised part is built again;
    /// a look refused again stays one plain line. Red: `set_look` bumping both whatever came.
    #[test]
    fn the_same_look_again_builds_nothing_again() {
        let _one = crate::view_cost::one();
        let mut live = crate::live::Live::default();
        let at = |live: &crate::live::Live| (live.look_rev, crate::version::LOOK.load(std::sync::atomic::Ordering::Relaxed));
        let v = json!({"colour": {"dark": dark("#f0e6d2")}});
        live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(v.clone()) });
        let taken = at(&live);
        live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Look(v) });
        assert_eq!(at(&live), taken, "the same look again");
        let broken = json!({"colour": {"dark": dark("#111111")}});
        live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Look(broken.clone()) });
        let refused = at(&live);
        assert_ne!(refused, taken, "a refusal is a change: its line is drawn");
        live.apply(coord_types::Event { seq: 4, body: coord_types::Body::Look(broken) });
        assert_eq!((at(&live), live.refused().count()), (refused, 1), "refused again");
    }

    /// A text-change ticket of the record, ID opened by BY, closed with RESULT: its check's answer (Look, or Checks for a
    /// kind's change), and its load's at AT: Live as COMMIT, or refused when COMMIT is empty.
    fn changed(id: &str, by: &str, check: &str, at: &str, commit: &str, result: Value) -> TicketView {
        let n: u64 = id.rsplit('-').next().unwrap().parse().unwrap();
        let load = if commit.is_empty() { json!({"head": "Refused", "why": "w", "at": []}) } else { json!({"head": "Live", "commit": commit}) };
        serde_json::from_value(json!({"id": id, "n": n, "kind": "text-change", "opener": by, "ctx": {}, "form": {"why": "w"}, "title": "w",
            "status": {"is": "done", "result": result}, "children": [], "program": "", "bound": 4, "spent": 3, "opened_at": at, "given": [],
            "posts": [{"n": 1, "at": at, "by": "coord", "text": "", "head": {"head": check, "by": by, "files": []}},
                      {"n": 2, "at": at, "by": "coord", "text": "", "head": load}]}))
        .unwrap()
    }

    /// The look on the Kinds page, where kinds are edited (N-866.p6, 2-5): one line says who changed it last, when and on
    /// which thread, the way back beside it named for what it does; Edit look opens the text coordd serves, the control
    /// drawn beside it from the text as it stands; Save opens the act with the text; its refusal shows under the editor
    /// and keeps it open, a live change closes it. Red: the editor drawn from the window's own look, or Save written
    /// straight to the file, fails it.
    #[test]
    fn the_look_is_edited_where_kinds_are_and_saved_by_the_act() {
        type Rt = rune::runtime::Runtime<App>;
        // a second of frames: a colour that changes is eased to its new value
        let frames = |rt: &mut Rt| {
            for _ in 0..8 {
                rt.frame(0.125);
            }
        };
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        let rect = |rt: &Rt, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
        s.sel = crate::Sel::Kinds;
        s.projects[0].protocol = coord_types::LOOK_PROTOCOL;
        let (t1, t2) = ("2026-11-22T20:40:00Z", "2026-11-22T21:58:00Z");
        let live = |c: &str| json!({"head": "Live", "commit": c, "still": null});
        for v in [
            changed("T-3", "owner", "Look", t1, "aaaa1111", live("aaaa1111")),
            changed("T-5", "dev", "Checks", "2026-11-22T21:00:00Z", "bbbb2222", live("bbbb2222")),
            changed("T-7", "dev", "Look", t2, "cccc3333", live("cccc3333")),
        ] {
            s.projects[0].live.tickets.insert(v.n, v);
        }
        let all = changes(&s.projects[0]);
        assert_eq!(all.iter().map(|c| (c.v.id.as_str(), c.by.as_str(), c.commit.as_str())).collect::<Vec<_>>(), [("T-3", "owner", "aaaa1111"), ("T-7", "dev", "cccc3333")], "a kind's change is no look's");
        assert_eq!(back_words(&all).unwrap(), format!("Put back the look from {}", crate::when::at(t1)), "the look of the change before it");
        assert_eq!(back_words(&all[..1]).unwrap(), format!("Put back the look from before {}", crate::when::at(t1)));
        let mut rt: Rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(true, 1.0));
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "The window's look"), "{t:?}");
        assert!(t.iter().any(|x| *x == format!("changed by dev at {} on", crate::when::at(t2))), "who, when: {t:?}");
        assert!(t.iter().any(|x| x == "T-7"), "on which thread, by its id: {t:?}");
        assert!(t.iter().any(|x| *x == format!("Put back the look from {}", crate::when::at(t1))), "the way back beside it: {t:?}");
        // the way back is the act with the last change's commit
        assert_eq!(serde_json::to_value(back_request("cccc3333", "w")).unwrap()["form"], json!({"files": [], "back": "cccc3333", "why": "w"}));

        // Edit look: the look as coordd served it, beside the kinds, drawn in its own colours, not the window's
        let palette = r##"[colour]
page = "#101114"
read = "#e8e8ea"
support = "#a0a1a6"
quiet = "#6c6d72"
layer = "#1a1b1f"
border = "#2a2b30"
attention = "#336699"
"##;
        let served = json!({"kinds": [], "calls": [], "look": {"path": "/f/look.toml", "text": palette}}).to_string();
        rt.update(|s, _| came(s, 0, Asked::Sources, true, served.clone()));
        assert_eq!(rt.state.tickets.edits.look.open.as_ref().map(|o| (o.orig.as_deref(), o.text.text.as_str())), Some((Some(palette), palette)));
        frames(&mut rt);
        let read_colour = |rt: &Rt| rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some("What you read")).and_then(|e| e.text_color);
        assert_eq!(read_colour(&rt), Some(rune::color::hex("#e8e8ea").unwrap()), "the preview in the text's colours");
        // he types: the control is drawn from the text as it stands, before any Save
        let warm = palette.replace("#e8e8ea", "#f0e6d2");
        rt.update(|s, _| s.tickets.edits.look.open.as_mut().unwrap().text = TextEdit::new(warm.clone()));
        frames(&mut rt);
        assert_eq!(read_colour(&rt), Some(rune::color::hex("#f0e6d2").unwrap()));
        assert_ne!(Some(Kit::neutral(true).c.text_base), read_colour(&rt));
        // a text the window would not take: not drawn, each fault said, Save not offered
        rt.update(|s, _| s.tickets.edits.look.open.as_mut().unwrap().text = TextEdit::new("[context.mark]\nshape = \"none\"\n"));
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Not drawn while it does not check:") && t.iter().any(|x| x.starts_with("✗ `context.mark.shape`")), "{t:?}");
        assert!(read("[context.mark]\nshape = \"none\"\n").is_err() && read("").is_ok_and(|l| l == Look::default()), "an empty look is the built-in one");

        // Save is the act: a text-change with the text as typed, the look's file as the act names it
        assert_eq!(serde_json::to_value(save_request(&warm, "warmer text")).unwrap(), json!({"op": "open", "kind": "text-change", "under": null, "form": {"files": [{"path": "look.toml", "text": warm}], "back": null, "why": "warmer text"}, "text": null}));
        rt.update(|s, _| {
            s.tickets.edits.look.open.as_mut().unwrap().text = TextEdit::new(warm.clone());
            came(s, 0, Asked::Opened, true, "Opened T-9.".into());
        });
        assert_eq!(rt.state.tickets.edits.look.change, Some(("research".to_string(), "T-9".to_string())));
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Checking, committing and loading it:"));
        // refused: under the editor, which stays open with his text
        let why = "nothing changes: look.toml is not a look the window takes: `colour` (dark): `read` stands 1.2 against `page`, 4.5 is wanted";
        rt.update(|s, _| {
            let v = changed("T-9", "owner", "Look", t2, "", json!({"head": "Refused", "why": why, "at": []}));
            s.projects[0].live.tickets.insert(v.n, v);
            assert!(poll(s));
        });
        let said = format!("N{}", &why[1..]);
        assert_eq!(rt.state.tickets.edits.look.said, Some(Err(said.clone())));
        assert!(rt.state.tickets.edits.look.open.is_some() && rt.state.tickets.edits.look.change.is_none());
        frames(&mut rt);
        assert!(texts(&rt).contains(&said));
        let (area, refusal) = (rect(&rt, "look-text").unwrap(), rect(&rt, "look-said").unwrap());
        assert!(refusal[1] > area[1] + area[3], "under the editor: {refusal:?} {area:?}");
        // live: the editor closes and the line says so
        rt.update(|s, _| {
            came(s, 0, Asked::Opened, true, "Opened T-10.".into());
            let v = changed("T-10", "owner", "Look", "2026-11-22T22:05:00Z", "dddd4444", live("dddd4444"));
            s.projects[0].live.tickets.insert(v.n, v);
            assert!(poll(s));
            assert!(!poll(s), "once");
        });
        assert!(rt.state.tickets.edits.look.open.is_none());
        assert_eq!(rt.state.tickets.edits.look.said, Some(Ok("Live, committed as dddd444.".into())));
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| *x == format!("changed by you at {} on", crate::when::at("2026-11-22T22:05:00Z"))), "his own change, the newest: {t:?}");
        // a project that is no flavour's: no editor, one plain line
        rt.update(|s, _| came(s, 0, Asked::Sources, true, json!({"kinds": [], "calls": [], "look": null}).to_string()));
        assert!(rt.state.tickets.edits.look.open.is_none() && matches!(&rt.state.tickets.edits.look.said, Some(Err(e)) if e.contains("No look.toml")));
    }

    /// The window's words and the threads list's row from the look, live (N-1104 p113, head 4; consult-6 p109: "call
    /// `With you` `Mine`"): a look that arrives words the list's headings and its foot, the chat's switch, its empty
    /// line, and its box's line and placeholder, and orders each row's fields; one with neither is the window's own.
    /// Red: a page that draws its own literal in place of `say`, or a row built in a fixed order, fails it.
    #[test]
    fn the_words_and_the_rows_order_come_from_the_look() {
        type Rt = rune::runtime::Runtime<App>;
        let frames = |rt: &mut Rt| {
            for _ in 0..8 {
                rt.frame(0.125);
            }
        };
        let texts = |rt: &Rt| -> Vec<String> { rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect() };
        // T's x in the threads list's row of N-1 (its key `goal-N-1`; the sidebar's structure tree is another part)
        let x_of = |rt: &Rt, t: &str| {
            let all = rt.elements();
            let r = all.iter().find(|e| !e.exiting && e.key.as_deref() == Some("goal-N-1"))?.rect;
            all.iter().find(|e| !e.exiting && e.text.as_deref() == Some(t) && e.rect[1] >= r[1] && e.rect[1] < r[1] + r[3] && e.rect[0] >= r[0] && e.rect[0] < r[0] + r[2]).map(|e| e.rect[0])
        };
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev"]);
        let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "dev"}, "title": "the look's row",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-22T22:00:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-22T22:00:00Z", "by": "owner", "text": "the look's row", "names": ["dev"]}]}))
        .unwrap();
        s.projects[0].live.tickets.insert(1, v);
        // N-2 ended this evening: the list's foot, `Ended today`
        let ended: TicketView = serde_json::from_value(json!({"id": "N-2", "n": 2, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "dev"}, "title": "an ended one",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-22T21:00:00Z", "closed_at": "2026-11-22T21:30:00Z", "given": [],
            "posts": [{"n": 1, "at": "2026-11-22T21:00:00Z", "by": "owner", "text": "an ended one", "names": ["dev"]}]}))
        .unwrap();
        s.projects[0].live.tickets.insert(2, ended);
        s.sel = crate::Sel::Tickets;
        let mut rt: Rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(true, 1.0));
        frames(&mut rt);
        let (id, label) = (x_of(&rt, "N-1"), x_of(&rt, "the look's row"));
        assert!(id.is_some() && label.is_some() && label < id, "the built-in row: the label, then the id: {:?}", texts(&rt));
        let look = json!({"words": {"with_you": "Mine", "nothing_with": "Nothing with {agent} yet", "all_active": "Going", "ended_today": "Done today",
            "to_agent": "Write to {agent}", "new_thread_with": "Start one with {agent}"}, "list": {"row": ["id", "label", "mark"]}});
        rt.update(|s, _| _ = s.projects[0].live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(look.clone()) }));
        frames(&mut rt);
        let (id, label) = (x_of(&rt, "N-1"), x_of(&rt, "the look's row"));
        assert!(id.is_some() && id < label, "the look's order: the id first: {id:?} {label:?}");
        assert!(texts(&rt).iter().any(|t| t == "Going · 1"), "the group's heading in its words: {:?}", texts(&rt));
        assert!(texts(&rt).iter().any(|t| t == "Done today · 1") && !texts(&rt).iter().any(|t| t == "Ended today · 1"), "the list's foot: {:?}", texts(&rt));
        assert!(rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some("group-Ended today")), "its key the window's own, so his fold stays");
        // an agent's chat: the switch and the empty line in the look's words
        rt.update(|s, _| {
            s.projects[0].live.tickets.clear();
            s.sel = crate::Sel::Agent("dev".into());
        });
        frames(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Mine") && !t.iter().any(|x| x == "With you"), "{t:?}");
        assert!(t.iter().any(|x| x == "Nothing with dev yet"), "{t:?}");
        // the box: its line, where his words go, and its placeholder (the field's label)
        assert!(t.iter().any(|x| x == "Start one with dev") && !t.iter().any(|x| x == "New thread with dev"), "{t:?}");
        let placeholder = |rt: &Rt, p: &str| rt.elements().iter().any(|e| !e.exiting && e.label.as_deref() == Some(p));
        assert!(placeholder(&rt, "Write to dev") && !placeholder(&rt, "To dev"));
        // none: the window's own words again
        rt.update(|s, _| _ = s.projects[0].live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Look(Value::Null) }));
        frames(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "With you") && texts(&rt).iter().any(|x| x == "New thread with dev") && placeholder(&rt, "To dev"));
    }

    #[test]
    fn the_snapshot_and_the_event_carry_the_look_whole() {
        let _one = crate::view_cost::one();
        let v = json!({"colour": {"dark": dark("#e8e8ea")}, "words": {"with_you": "Mine"}});
        for body in [coord_types::Body::Look(v.clone())] {
            let e = coord_types::Event { seq: 7, body };
            let back: coord_types::Event = serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap();
            assert!(matches!(back.body, coord_types::Body::Look(ref x) if *x == v));
        }
        let mut live = crate::live::Live::default();
        live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(v) });
        assert_eq!(coord_types::look::say(&live.look, "with_you", &[]), "Mine");
    }

    /// The keys of WORDS that no line of PAGES says through `say` (a comment says nothing).
    fn unsaid<'a>(pages: &[&str], words: &[(&'a str, &[&str], &str)]) -> Vec<&'a str> {
        let said = |key: &str| pages.iter().flat_map(|f| f.lines()).any(|l| !l.trim_start().starts_with("//") && l.contains("say(") && l.contains(&format!("\"{key}\"")));
        words.iter().map(|w| w.0).filter(|k| !said(k)).collect()
    }

    /// Each word of `WORDS` is said by the window through `say`, so a word he changes changes what he reads; and a key no
    /// page says is found (`moving` stood in the list after the Moving group went, 0.2's C13, and changed nothing;
    /// paper-research N-1446 p1783: "make it fail on a key no page says"). Red: a page that draws one of them as its own
    /// literal again.
    #[test]
    fn every_word_is_said() {
        let pages = [include_str!("stamp.rs"), include_str!("outline.rs"), include_str!("chat.rs"), include_str!("withyou.rs"), include_str!("message_box.rs"), include_str!("streams.rs")];
        assert_eq!(unsaid(&pages, coord_types::look::WORDS), Vec::<&str>::new(), "in WORDS and no page says it");
        let as_it_stood: &[(&str, &[&str], &str)] = &[("all_active", &[], "All active"), ("moving", &[], "Moving"), ("ended_today", &[], "Ended today")];
        assert_eq!(unsaid(&pages, as_it_stood), vec!["moving"], "a key no page says is found");
    }

    /// withyou.rs's made-up project with w4 besides, an agent with nothing yet.
    fn with_w4() -> crate::Proj {
        let mut p = crate::withyou::tests::research();
        p.live.agents.push(serde_json::from_value(json!({"id": "AGT-4", "name": "w4", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T08:00:00Z", "tools": [], "items": []})).unwrap());
        p
    }

    /// The look of the stills: the list's headings and foot, the agent's page, the version line in other words, and the
    /// id before the label in a list row (consult-6's tests, N-1104 p109).
    fn worded() -> serde_json::Value {
        json!({"words": {"waits_on_you": "Yours", "all_active": "Open", "ended_today": "Done today", "with_you": "Mine", "to_agent": "Write to {agent}",
            "new_thread_with": "Start one with {agent}", "nothing_with": "Nothing yet with {agent}.", "version": "{version} · {build}"},
            "list": {"row": ["mark", "id", "label"]}})
    }

    /// The stills of head 4 (paper-research N-1446 p1779, p1799), from a scratch scene: withyou.rs's made-up project with
    /// w4, no coordd. Threads with N-8 unfolded, and w4's page, each in the window's own words (`own`) and in `worded`'s
    /// look (`look`), dark and light: words-threads-own-dark.png … words-w4-look-light.png, each with its tree.
    /// `STILL_OUT=DIR … look::tests::words_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn words_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn base(sel: crate::Sel, look: bool, dark: bool) -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut p = with_w4();
            p.protocol = coord_types::PROTOCOL;
            if look {
                p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Look(worded()) });
            }
            s.projects = vec![p];
            s.cur = 0;
            s.sel = sel;
            s.dark = Some(dark);
            s.streams.open.insert("outline|0|N-8".into());
            s
        }
        let shots: [(&str, fn() -> App); 8] = [
            ("words-threads-own-dark.png", || base(crate::Sel::Tickets, false, true)),
            ("words-threads-look-dark.png", || base(crate::Sel::Tickets, true, true)),
            ("words-threads-own-light.png", || base(crate::Sel::Tickets, false, false)),
            ("words-threads-look-light.png", || base(crate::Sel::Tickets, true, false)),
            ("words-w4-own-dark.png", || base(crate::Sel::Agent("w4".into()), false, true)),
            ("words-w4-look-dark.png", || base(crate::Sel::Agent("w4".into()), true, true)),
            ("words-w4-own-light.png", || base(crate::Sel::Agent("w4".into()), false, false)),
            ("words-w4-look-light.png", || base(crate::Sel::Agent("w4".into()), true, false)),
        ];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into(), "--tree".into(), out.join(name.replace(".png", ".json")).display().to_string()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
