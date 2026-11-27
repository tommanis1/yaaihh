//! One ticket's page, opened from Threads: its state, whom it waits on, its stages and whole program, the steps owed
//! and halts on it, and under "more about it" when it opened, how its answers were read, its tree, its form, where it
//! went, what happened and changing it.

use crate::{App, Pending, Proj};
use coord_types::{Request, TicketView};
use docket_machine::ViewStatus;
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Badge, BadgeVariant, Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, TextArea, TextInput};
use serde_json::Value;
use std::collections::HashSet;
use crate::threads::{asks_on, blocked_line, Cleared, CLOSE, open, owners_stops, Page, resting};
use crate::texts::{badge_outcome, met_by, short_hashes, words};
use crate::seen::unread_line;
use crate::picture::{kind_texts, labels};
use crate::ask::{answer_box, ask_block, question_headline};

/// V's title; an agent's turn filed whose view has none says whose turn it is (its form's `by`); a part with none
/// (under `any n of`) whose part it is.
pub(crate) fn title(p: &Proj, v: &TicketView) -> String {
    // an agent's question: whose, and about what (its words are in its block below), not its kind
    if v.kind == "ask" {
        let by = v.opener.clone().unwrap_or_else(|| "an agent".into());
        let on = v.under.as_deref().and_then(|u| p.live.tickets.values().find(|t| t.id == u)).map(|t| title(p, t)).filter(|t| !t.is_empty());
        return match on {
            Some(t) => format!("{by}'s question on {}", crate::refs::cut(&t, 80)),
            None => format!("{by}'s question"),
        };
    }
    if !v.title.is_empty() {
        return v.title.clone();
    }
    // coordd's label reads a name the view's title cannot (a place in a session)
    if let Some(l) = p.labels.get(&v.id) {
        return l.to_string();
    }
    if let Some(up) = v.parent.as_deref().and_then(|id| p.live.tickets.values().find(|t| t.id == id)) {
        let t = title(p, up);
        return if t.is_empty() { format!("part of {}", up.id) } else { format!("part of {}: {t}", up.id) };
    }
    match v.form.get("by").and_then(Value::as_str).or_else(|| v.form.get("by").and_then(|b| b.get("new")).and_then(Value::as_str)) {
        Some(a) if v.kind == "filing" => format!("turn of {a}"),
        _ => String::new(),
    }
}

/// The steps of V that did not go out, one line each: held back unsent (`unsent`), refused to their agent for its model
/// (a run node's `refused`).
pub fn step_lines(v: &TicketView) -> Vec<String> {
    fn refused(ns: &[docket_machine::RunNode], out: &mut Vec<String>) {
        for n in ns {
            if let Some(r) = &n.refused {
                out.push(format!("{}: {}", n.text, crate::picture::refused_words(r)));
            }
            refused(&n.nodes, out);
        }
    }
    let mut out: Vec<String> = v.unsent.iter().map(|u| format!("Step {} ({}, to {}): {}", u.step, u.call, u.worker, crate::picture::unsent_words(u))).collect();
    refused(&v.run, &mut out);
    out
}

/// Whether an agent answered a step of V, so that its answer was read (formed): a post on a step not the owner's.
fn read_answers(v: &TicketView) -> bool {
    v.posts.iter().any(|p| p.step.is_some() && p.by != "owner")
}

/// How V's answers were read (paper-research N-101 p623): a link that asks coordd for the forming files, then each
/// reading by step, in order: when, whose answer, the call and its model, what it read (or none, and why). A reading
/// opens apart (popup.rs) to what the call was sent and what it answered, whole.
pub(crate) fn readings(pg: &Page, p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    if !read_answers(v) {
        return None;
    }
    let t = v.n;
    let id = v.id.clone();
    let Some(got) = pg.forming.get(&t) else {
        let link = text("How its answers were read ›").font(k.ty.small).color(k.c.text_accent);
        return Some(k.interactive(row().key(format!("readings-{id}")).child(link)).on_press(move |s: &mut App| {
            let r = s.proj().ok_or_else(|| "no project".to_string()).and_then(|p| p.ask_once(&coord_types::Request::Forming { t: id.clone() })).and_then(|o| serde_json::from_str(&o).map_err(|e| format!("reading failed: {e}")));
            s.tickets.forming.insert(t, r);
        }));
    };
    let hide = k.interactive(text("How its answers were read ▾").font(k.ty.small).color(k.c.text_accent)).on_press(move |s: &mut App| {
        s.tickets.forming.remove(&t);
    });
    let mut c = col().key(format!("readings-{}", v.id)).w_full().gap(6.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_01).child(hide);
    let reads = match got {
        Err(e) => return Some(c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap())),
        Ok(r) if r.is_empty() => return Some(c.child(text("No reading kept.").font(k.ty.small).color(k.c.text_faint).wrap())),
        Ok(r) => r,
    };
    let mut step = 0;
    for r in reads {
        if r.step != step {
            step = r.step;
            let n = reads.iter().filter(|x| x.step == step).count();
            c = c.child(text(format!("Step {step}: {} {}", n, if n == 1 { "reading" } else { "readings" })).font(k.ty.small_medium).color(k.c.text_base));
        }
        c = c.child(reading(p, v, r, k));
    }
    Some(c)
}

/// Ticket T's (ID's) readings asked for, and the last reading of STEP opened apart (the step card's chip).
pub fn open_reading(s: &mut App, t: u64, id: &str, step: u32) {
    let r: Result<Vec<coord_types::FormingRead>, String> = s.proj().ok_or_else(|| "no project".to_string()).and_then(|p| p.ask_once(&coord_types::Request::Forming { t: id.to_string() })).and_then(|o| serde_json::from_str(&o).map_err(|e| format!("reading failed: {e}")));
    let open = r.as_ref().ok().and_then(|rs| rs.iter().rev().find(|x| x.step == step)).and_then(|last| {
        let p = s.proj()?;
        let v = p.live.tickets.get(&t)?;
        Some((reading_title(p, v, last), last.file.clone()))
    });
    s.tickets.forming.insert(t, r);
    s.tickets.shown = Some(t);
    if let Some((title, file)) = open {
        crate::popup::open(s, title, crate::popup::What::Reading { t, file });
    }
}

/// The title of reading R of V's answers in P, opened apart.
pub(crate) fn reading_title(p: &Proj, v: &TicketView, r: &coord_types::FormingRead) -> String {
    crate::popup::title(&crate::refs::ticket(Some(p), v).label, &format!("{}.s{}", v.id, r.step), &format!("how {}'s answer was read", r.agent))
}

/// Text T in a box, in mono, to select from: what an agent read, a call's prompt or reply.
pub(crate) fn boxed(t: String, k: &Kit) -> El {
    lit_boxed(None, t, k)
}

/// [`boxed`], in a popup (LIT: the window's state, its text's key, its comment source): the passage he is commenting
/// on lit in it, the comment box's room under it while the box stands in the flow (ask.rs `lit_words`).
pub(crate) fn lit_boxed(lit: Option<(&App, &str, &str)>, t: String, k: &Kit) -> El {
    let make = |t: String| text(t).font(k.ty.mono).color(k.c.text_base).wrap().selectable(true);
    let words = match lit {
        Some((s, key, source)) => crate::ask::lit_words(s, key, source, &t, make, k),
        None => make(t),
    };
    col().w_full().padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).child(words)
}

/// The agent's answer inside a forming call's PROMPT: what stands between the last "<<<" and ">>>" after "The agent's
/// answer:" (form.rs, form_parts.rs).
fn answer_in(prompt: &str) -> Option<&str> {
    let at = prompt.rfind("The agent's answer:\n<<<\n")?;
    let rest = &prompt[at + "The agent's answer:\n<<<\n".len()..];
    Some(rest.rfind("\n>>>").map_or(rest, |e| &rest[..e]))
}

/// One reading of V's answers in P: its line (when, whose answer, the call, what it read), which opens the prompt and
/// the reply apart (`reading_body`).
pub(crate) fn reading(p: &Proj, v: &TicketView, r: &coord_types::FormingRead, k: &Kit) -> El {
    let key = format!("forming-{}", r.file);
    let calls: Vec<String> = r.calls.iter().map(|c| match &c.model { Some(m) => format!("{} on {m}", c.call), None => c.call.clone() }).collect();
    let when = if r.at.is_empty() { String::new() } else { format!("{} · ", crate::when::at(&r.at)) };
    let what = match (&r.head, &r.why) {
        (Some(h), _) => format!("read as {h}"),
        (None, Some(w)) => format!("read as none of its outcomes: {w}"),
        (None, None) => "read as none of its outcomes".to_string(),
    };
    let line = format!("› {when}{}'s answer · {} · {what}", r.agent, calls.join(", "));
    let color = if r.head.is_some() { k.c.text_base } else { k.c.warning.fg };
    let (title, what) = (reading_title(p, v, r), crate::popup::What::Reading { t: v.n, file: r.file.clone() });
    let mut c = col().key(key).w_full().gap(4.0).child(k.interactive(text(line).font(k.ty.small).color(color).wrap()).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), what.clone())));
    if let Some(n) = &r.not_taken {
        c = c.child(text(format!("refused: {n}")).font(k.ty.small).color(k.c.warning.fg).wrap());
    }
    c
}

/// Reading R of ticket T as its popup shows it: the agent's answer, then each call's reply, under it the line that opens
/// what the call was sent in a window of its own (sent_window.rs; paper-research N-1446 p2112, 2), and the reading's
/// file.
pub(crate) fn reading_body(t: u64, r: &coord_types::FormingRead, lit: Option<(&App, &str, &str)>, k: &Kit) -> El {
    // in its popup, each text keyed apart, the passage commented on lit there (LIT: as `lit_boxed`'s, the key a prefix)
    let shown = |j: &str, t: String| match lit {
        Some((s, key, source)) => lit_boxed(Some((s, &format!("{key}-{j}"), source)), t, k),
        None => boxed(t, k),
    };
    let mut c = col().w_full().gap(4.0);
    if let Some(a) = r.calls.first().and_then(|c| answer_in(&c.prompt)) {
        c = c.child(text(format!("{}'s answer", r.agent)).font(k.ty.small_medium).color(k.c.text_muted)).child(shown("answer", a.to_string()));
    }
    for (i, call) in r.calls.iter().enumerate() {
        let reply = call.reply.as_ref().map_or_else(|| "(no reply came)".to_string(), |v| serde_json::to_string_pretty(v).unwrap_or_default());
        c = c.child(text(format!("What {} answered", call.call)).font(k.ty.small_medium).color(k.c.text_muted)).child(shown(&format!("reply{i}"), reply));
        let file = r.file.clone();
        c = c.child(k.interactive(text(format!("› What {} was sent", call.call)).key(format!("forming-sent-{}-{i}", r.file)).font(k.ty.small).color(k.c.text_accent)).on_press(move |s: &mut App| crate::sent_window::open_call(s, t, &file, i)));
    }
    c.child(text(r.file.clone()).font(k.ty.small).color(k.c.text_faint).selectable(true))
}

/// A filing the clerk filed: each piece, the ticket it went to, and how (an answer to its step, or a post), with
/// the owner's words of that piece (a span of the filing's text).
pub(crate) fn filed(pg: &Page, v: &TicketView, k: &Kit) -> Option<El> {
    let ViewStatus::Done { result } = &v.status else { return None };
    let pieces = result.get("pieces")?.as_array()?;
    let all = words(pg, v.form.get("text")?);
    let mut c = col().w_full().gap(6.0);
    for p in pieces {
        let (at, len) = (p["at"].as_u64().unwrap_or(0) as usize, p["len"].as_u64().unwrap_or(0) as usize);
        let said = all.get(at..at + len).unwrap_or("(the words are not where the filing says)");
        let how = match p["answer"].as_str() {
            Some(a) => format!("answered {a}"),
            None => "a post".to_string(),
        };
        c = c.child(
            col()
                .w_full()
                .gap(2.0)
                .child(text(format!("{} · {how}", p["ticket"].as_str().unwrap_or("?"))).font(k.ty.small_medium).color(k.c.text_muted))
                .child(text(format!("“{said}”")).font(k.ty.body).color(k.c.text_base).wrap().selectable(true)),
        );
    }
    Some(c)
}

/// Ticket V's panel: the step picked in its whole program (its once-panel, edit.rs), else the stage clicked, with a
/// close at its top; None when neither is.
pub(crate) fn side_panel(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    let pg = &s.tickets;
    // the usual × in the panel's corner, not words that read as its heading (paper-research N-101 p821)
    let close = |what: &'static str| {
        let tid = v.id.clone();
        let x = IconButton::new(&CLOSE).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label(format!("Close {what}")).help(format!("Close {what}"));
        row().w_full().child(row().grow(1.0)).child(x.build(k).on_press(move |s: &mut App| {
            let pre = format!("sel:{tid}.");
            s.tickets.pic_open.retain(|k| !k.starts_with(&pre));
        }))
    };
    if let Some(panel) = crate::edit::step_panel(s, v, k) {
        return Some(panel);
    }
    let key = crate::picture::shown_stage(&pg.pic_open, &v.id)?;
    let words = |x: &Value| words(pg, x);
    let pic = crate::picture::Pic { k, p: Some(p), kinds: kind_texts(pg), tickets: p.live.tickets.values().collect(), open: &pg.pic_open, marks: crate::edit::marks(pg), words: &words, pick: Some(v.n), picked: None };
    let labels = labels(s, p);
    let body = pic.stage_panel(&v.program, v, &key, &labels)?;
    Some(col().w_full().gap(8.0).child(close("the stage")).child(body))
}

/// Ticket V's stages (picture/stages.rs), in the docs' words, lit by where it stands; under them the whole program,
/// every branch, with the tree of tickets it opened, behind a press: APART (the ticket's page), it opens apart
/// (popup.rs), else it unfolds in place (the Docket's Thread scale, whose job is to show it: N-1396 p282's rule).
pub(crate) fn program_part(s: &App, p: &Proj, v: &TicketView, apart: bool, k: &Kit) -> El {
    let pg = &s.tickets;
    let n = v.n;
    let labels = labels(s, p);
    let mut c = col().key("picture").w_full().gap(8.0).child(page_pic(s, p, v, k, |pic| pic.stages(&v.program, Some(v), &v.id, &labels)));
    const WHOLE: &str = "the whole program, every branch";
    if apart {
        let title = crate::popup::title(&crate::refs::ticket(Some(p), v).label, &v.id, WHOLE);
        let what = crate::popup::What::Program { n };
        return c.child(k.interactive(row().pt(6.0).child(text(format!("› {WHOLE}")).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), what.clone())));
    }
    let whole = format!("whole-{n}");
    let shown = pg.pic_open.contains(&whole);
    c = c.child(k.interactive(row().pt(6.0).child(text(format!("{} {WHOLE}", if shown { "⌄" } else { "›" })).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
        if !s.tickets.pic_open.remove(&whole) {
            s.tickets.pic_open.insert(whole.clone());
        }
    }));
    if shown {
        c = c.child(whole_program(s, p, v, k));
    }
    c
}

/// Ticket V's whole program, every branch, lit by where it stands, with the binding that opened it and the tree of
/// tickets it opened: under its fold, or apart (popup.rs).
pub(crate) fn whole_program(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let mut c = col().w_full().gap(8.0);
    if open(v) {
        c = c.child(text("Click a step to edit it.").font(k.ty.small).color(k.c.text_faint));
    }
    // the binding that opened it, on its edge (0.2 line 26); wrapped to the page, and where an arm cannot wrap
    // below it, scrolling, as on the Kinds card
    let whole = page_pic(s, p, v, k, |pic| pic.kind_into(&v.program, Some(v), &crate::picture::bindings_of(v.binding.as_slice()), &v.id));
    c = c.child(col().w_full().min_w(0.0).scroll_x().child(whole.w_min_content()));
    c.child(section("The tickets it opened", k)).child(tree(p, v, k))
}

/// Ticket V's picture as its page draws it, lit, a step clicked opening its once-panel and the one open drawn picked,
/// handed to DRAW: the stages' and the whole program's one builder.
pub(crate) fn page_pic<R>(s: &App, p: &Proj, v: &TicketView, k: &Kit, draw: impl FnOnce(&crate::picture::Pic<'_>) -> R) -> R {
    let pg = &s.tickets;
    let words = |x: &Value| words(pg, x);
    let picked = pg.edits.step.as_ref().filter(|st| st.t == v.n).map(|st| st.key.as_str());
    draw(&crate::picture::Pic { k, p: Some(p), kinds: kind_texts(pg), tickets: p.live.tickets.values().collect(), open: &pg.pic_open, marks: crate::edit::marks(pg), words: &words, pick: Some(v.n), picked })
}

/// Where ticket V is, in plain words (picture/stages.rs `where_now`), when it is at work.
fn where_now(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    let pg = &s.tickets;
    let words = |x: &Value| words(pg, x);
    let pic = crate::picture::Pic { k, p: Some(p), kinds: kind_texts(pg), tickets: p.live.tickets.values().collect(), open: &pg.pic_open, marks: crate::edit::marks(pg), words: &words, pick: Some(v.n), picked: None };
    pic.where_now(v, &labels(s, p))
}

/// Ticket V in full: what it is and where, the owner's words cut short, its stages, what it asks of him; the rest
/// under "more about it".
pub(crate) fn detail(s: &App, p: &Proj, v: &TicketView, k: &Kit) -> El {
    let pg = &s.tickets;
    let mut c = col().w_full().px(24.0).py(16.0).gap(12.0);
    // the state's chip, unless the line below says whom it waits on (two answers to one question read as a conflict:
    // "waits on P-2" above "Waits on runner")
    let has_program = !v.program.trim().is_empty();
    let now = if has_program { where_now(s, p, v, k) } else { None };
    let chip = if waits_line(v).is_some() || now.is_some() { None } else { chip(v, k) };
    // its own words first, cut to three lines; then what it is, "change · C-1", small; then where it is, in plain
    // words (paper-research N-101 p773, the owner N-286.p1: label first, the id small)
    let headline = question_headline(pg, v);
    // a question: who asks; its words once, in the card below where the card says them, else under who asks (N-371.p7)
    let shown_title = headline.as_ref().map(|h| h.0.clone()).unwrap_or_else(|| title(p, v));
    let n = v.n;
    let what = crate::refs::Ref { label: v.kind.clone(), id: v.id.clone() };
    if shown_title.is_empty() {
        c = c.child(row().w_full().gap(10.0).center_y().child(crate::refs::el(&what, k.ty.body_medium, k.c.text_base, k)).children(chip));
    } else {
        let whole = format!("title-{n}");
        let all = pg.pic_open.contains(&whole);
        let mut t = text(shown_title.clone()).font(k.ty.body_medium).color(k.c.text_base).wrap().selectable(true);
        if !all {
            t = t.max_lines(3);
        }
        // a question: its kind's description and its state's badge are the engine's detail, behind a click beside
        // who asks
        let detail = format!("detail-{n}");
        if headline.is_some() && !pg.pic_open.contains(&detail) {
            let more = k.interactive(row().child(text(format!("› {} {}", v.kind, v.id)).font(k.ty.small).color(k.c.text_faint))).on_press(move |s: &mut App| {
                s.tickets.pic_open.insert(detail.clone());
            });
            c = c.child(row().w_full().gap(10.0).center_y().child(t).child(more));
        } else if headline.is_some() {
            c = c.child(t);
            if !v.title.is_empty() {
                c = c.child(text(v.title.clone()).font(k.ty.small).color(k.c.text_muted).wrap());
            }
            let mut under = row().w_full().gap(10.0).center_y().child(crate::refs::el(&what, k.ty.small, k.c.text_muted, k)).children(chip);
            if shown_title.chars().count() > TITLE_SHOWN {
                under = under.child(more_less(all, whole, k));
            }
            c = c.child(under);
        } else {
            // a thread: its title with its id small beside it (a reference is id plus label); no kind name and no state
            // word under it, both under "more about it" (paper-research N-1446 p3207, 6)
            let mut head = row().w_full().gap(8.0).center_y().child(t.shrink().min_w(0.0)).child(text(v.id.clone()).font(k.ty.small).color(k.c.text_faint).key(format!("title-id-{n}")));
            if shown_title.chars().count() > TITLE_SHOWN {
                head = head.child(more_less(all, whole, k));
            }
            c = c.child(head);
        }
    }
    // a question: what it asks, under who asks, where its card does not say it (ask.rs `asked_on_page`)
    c = c.children(crate::ask::asked_on_page(pg, p, v, k));
    if let Some(now) = now {
        c = c.child(now);
    } else if let Some(w) = waits_line(v) {
        c = c.child(text(w).font(k.ty.small_medium).color(k.c.text_base).wrap());
    }
    // a build of a piece of its stopped or failed: where the change really waits (paper-research N-101 p962)
    c = c.children(blocked_line(p, v, k));
    // a stop down its tree passed to the owner: the stopped ticket and his two answers
    for (_, h) in owners_stops(p, v) {
        c = c.child(answer_box(pg, p, h, k));
    }
    let when = format!(
        "opened by {} at {}{}",
        // no opener: a child is opened by its parent's program, a ticket with neither by an event a binding is on
        v.opener.clone().unwrap_or_else(|| if v.parent.is_some() { "its parent".into() } else { "an event".into() }),
        crate::when::at(&v.opened_at.to_string()),
        v.closed_at.as_ref().map(|c| format!(", closed at {}", crate::when::at(&c.to_string()))).unwrap_or_default()
    );
    if let Some(u) = unread_line(v) {
        c = c.child(text(u).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    for l in step_lines(v) {
        c = c.child(text(l).font(k.ty.small_medium).color(k.c.warning.fg).wrap());
    }
    if let Some((at, t)) = pg.edits.reloaded.as_ref().and_then(|(at, r)| r.tickets.iter().find(|t| t.id == v.id).map(|t| (at, t))) {
        c = c.child(text(format!("At the reload of {}: {}", crate::when::at(at), crate::edit::reloaded_words(t))).font(k.ty.small).color(if t.took { k.c.text_muted } else { k.c.warning.fg }).wrap());
    }
    c = c.children(broken_line(v, k));
    if crate::waits::owes_owner(v) {
        c = c.child(answer_box(pg, p, v, k));
    }
    // slow mode's halts on it and below it (N-141): its own cards, then each handler ticket's (a kind's `when`, a
    // `during`'s look) under a line naming it; approve and the edits act on the ticket the halt is on
    let halts = crate::slow::shown(p, v);
    if !halts.is_empty() {
        let first = halts.first().map(|(t, h)| (t.id.clone(), crate::slow::n_of(h)));
        let mut hc = col().w_full().gap(10.0);
        let mut last: Option<&str> = None;
        for (t, h) in &halts {
            if t.id != v.id && last != Some(t.id.as_str()) {
                hc = hc.child(text(format!("{} {}, handling an event under {}", t.id, t.kind, v.id)).font(k.ty.small_medium).color(k.c.text_muted).wrap());
            }
            last = Some(t.id.as_str());
            hc = hc.child(crate::slow::card(pg, p, v, t, h, first == Some((t.id.clone(), crate::slow::n_of(h))), k));
        }
        c = c.child(section("Halted: waiting on you", k)).child(hc);
    }
    // an agent's question: the question and its answer, as the owner meets it (paper-research N-101 p984)
    if v.kind == "ask" {
        c = c.child(ask_block(pg, p, v, k));
    }
    // the agents' questions about it, where it stands
    for a in asks_on(p, v) {
        c = c.child(ask_block(pg, p, a, k));
    }
    if has_program && v.kind != "ask" {
        c = c.child(row().pt(6.0)).child(program_part(s, p, v, true, k));
    }
    if open(v) || resting(v) {
        // a question that waits on him: the answer box is the one he needs, a post behind a click (N-371.p5)
        let owes = matches!(&v.status, ViewStatus::Awaits { agent, .. } if agent == "owner");
        let key = format!("post-{n}");
        if owes && !pg.pic_open.contains(&key) && s.composers.get(&(s.cur, crate::message_box::thread_place(&v.id))).is_none_or(|t| t.text.is_empty()) {
            c = c.child(k.interactive(row().pt(6.0).child(text("› write to this thread").font(k.ty.small).color(k.c.text_faint))).on_press(move |s: &mut App| {
                s.tickets.pic_open.insert(key.clone());
            }));
        } else {
            c = c.child(post_box(s, v, k));
        }
    }
    // what is read now and then: when it opened, how its answers were read, the tickets under it, its form, where it
    // went, what happened, and changing it, under one fold
    let more = pg.more.contains(&n);
    c = c.child(k.interactive(row().pt(6.0).child(text(if more { "⌄ less" } else { "› more about it" }).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
        if !s.tickets.more.remove(&n) {
            s.tickets.more.insert(n);
        }
    }));
    if !more {
        return c;
    }
    // what it is and its state, in the engine's words, here and not under the title
    c = c.child(row().w_full().gap(10.0).center_y().child(crate::refs::el(&what, k.ty.small, k.c.text_muted, k)).children(self::chip(v, k)));
    c = c.child(text(when).font(k.ty.small).color(k.c.text_muted));
    if let Some(r) = readings(pg, p, v, k) {
        c = c.child(r);
    }
    if !has_program {
        c = c.child(section("The tree", k)).child(tree(p, v, k));
    }
    // the field the title is (an item's title) is shown once, as the title
    let fields: Vec<(&String, &Value)> = v.form.as_object().into_iter().flatten().filter(|(_, x)| v.title.is_empty() || x.as_str() != Some(v.title.as_str())).collect();
    if !fields.is_empty() {
        let mut f = col().w_full().gap(6.0);
        for (name, x) in fields {
            f = f.child(col().w_full().gap(2.0).child(text(name.clone()).font(k.ty.small_medium).color(k.c.text_muted)).child(text(words(pg, x)).font(k.ty.body).color(k.c.text_base).wrap().selectable(true)));
        }
        c = c.child(section("Its form", k)).child(f);
    }
    if let Some(f) = filed(pg, v, k) {
        c = c.child(section("Where it went", k)).child(f);
    }
    c = c.child(section("What happened", k)).child(history(pg, p, v, k));
    if open(v) {
        c = c.child(section("Change it", k)).child(change_box(pg, v, k));
    }
    c
}

/// "more" or "less" (ALL) beside a long title, its fold WHOLE.
fn more_less(all: bool, whole: String, k: &Kit) -> El {
    k.interactive(row().child(text(if all { "less" } else { "more" }).font(k.ty.small).color(k.c.text_accent))).on_press(move |s: &mut App| {
        if !s.tickets.pic_open.remove(&whole) {
            s.tickets.pic_open.insert(whole.clone());
        }
    })
}

/// From how many characters a ticket's text gets "more" (it shows three lines).
const TITLE_SHOWN: usize = 200;

pub(crate) fn section(t: &str, k: &Kit) -> El {
    row().pt(6.0).child(text(t.to_string()).font(k.ty.small_medium).color(k.c.text_muted))
}

/// The ticket V is under (each one up, clickable), V, and the tickets under it, indented.
fn tree(p: &Proj, v: &TicketView, k: &Kit) -> El {
    let by_id = |id: &str| p.live.tickets.values().find(|t| t.id == id);
    let mut up = Vec::new();
    let mut at = v.parent.as_deref().and_then(by_id);
    while let Some(t) = at {
        up.push(t);
        at = t.parent.as_deref().and_then(by_id);
    }
    let mut c = col().w_full().gap(2.0);
    let mut depth = 0;
    for t in up.into_iter().rev() {
        c = c.child(node(p, t, depth, false, k));
        depth += 1;
    }
    fn down<'a>(p: &'a Proj, t: &'a TicketView, depth: usize, me: u64, k: &Kit, c: &mut Vec<El>) {
        let kids: Vec<&TicketView> = t.children.iter().filter_map(|id| p.live.tickets.values().find(|x| &x.id == id)).collect();
        // a branch whose one child reads as it does (a `par` and its named part: P-2 and P-3 both "part about-note")
        // is drawn as the child's row, not a second row with the same words (paper-research N-101 p815)
        if let [ch] = kids[..] {
            if t.n != me && crate::refs::ticket(Some(p), ch).label == crate::refs::ticket(Some(p), t).label {
                return down(p, ch, depth, me, k, c);
            }
        }
        c.push(node(p, t, depth, t.n == me, k));
        for ch in kids {
            down(p, ch, depth + 1, me, k, c);
        }
    }
    let mut rows = Vec::new();
    down(p, v, depth, v.n, k, &mut rows);
    c.children(rows)
}

fn node(p: &Proj, t: &TicketView, depth: usize, me: bool, k: &Kit) -> El {
    let n = t.n;
    let r = row()
        .key(format!("node-{}", t.id))
        .pl(16.0 * depth as f32 + 6.0)
        .pr(6.0)
        .py(4.0)
        .gap(8.0)
        .center_y()
        .radius(k.radius.sm)
        .fill(if me { k.c.layer_02 } else { k.c.bg_base })
        .child(crate::refs::el(&crate::refs::ticket(Some(p), t), k.ty.ui_medium, k.c.text_base, k).shrink())
        .children(chip(t, k));
    if me { r } else { k.interactive(r).on_press(move |s: &mut App| s.tickets.shown = Some(n)) }
}

/// V's deliveries and posts in P, in time order; a thread's posts are the thread itself, drawn there once, so its
/// details list only what each agent was given. What an agent read opens apart (popup.rs).
pub(crate) fn history(pg: &Page, p: &Proj, v: &TicketView, k: &Kit) -> El {
    enum Entry<'a> {
        Given(&'a docket_machine::view::ViewGiven),
        Post(&'a docket_machine::Post),
    }
    let mut es: Vec<(String, Entry)> = Vec::new();
    // a delivery is listed once: the record with its place, when the session took it
    let mut seen: HashSet<&str> = HashSet::new();
    for g in v.given.iter().rev() {
        if seen.insert(g.delivery.as_str()) {
            es.push((g.at.to_string(), Entry::Given(g)));
        }
    }
    let thread = v.kind == "thread" || v.kind == "note";
    for p in v.posts.iter().filter(|_| !thread) {
        es.push((p.at.to_string(), Entry::Post(p)));
    }
    es.sort_by(|a, b| a.0.cmp(&b.0));
    let mut c = col().w_full().gap(8.0);
    if es.is_empty() {
        return c.child(text("Nothing yet.").font(k.ty.small).color(k.c.text_faint));
    }
    for (at, e) in es {
        let when = crate::when::at(&at);
        match e {
            Entry::Given(g) => {
                let what = match g.delivery.rsplit('.').next().unwrap_or("") {
                    "c" => "the closing notice".to_string(),
                    // a ticket repaired or rekinded closes again: C-12.c2, c3 (docket-core m830)
                    d if d.starts_with('c') && d[1..].parse::<u32>().is_ok() => format!("closing notice {}", &d[1..]),
                    "again" => "a reminder of its step".to_string(),
                    "withdrawn" => "the withdrawal of its step".to_string(),
                    d if d.starts_with('s') => format!("step {}", &d[1..]),
                    d if d.starts_with('p') => "posts naming it".to_string(),
                    d if d.starts_with("refused") => "a refusal of its answer".to_string(),
                    d => d.to_string(),
                };
                let id = g.delivery.clone();
                let mut line = row().w_full().gap(8.0).center_y().key(format!("given-{}-{}", g.delivery, g.to)).child(text(format!("{when} · {} was given {what} ({})", g.to, g.delivery)).font(k.ty.small).color(k.c.text_muted));
                if let Some(place) = g.place.clone() {
                    let title = crate::popup::title(&crate::refs::ticket(Some(p), v).label, &g.delivery, &format!("what {} read", g.to));
                    line = line.child(Button::new("What it read").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                        // its text asked for as the page shows it (texts.rs `wanted`), and shown apart
                        s.tickets.unfolded.insert(id.clone());
                        crate::popup::open(s, title.clone(), crate::popup::What::Sent { place: place.clone() });
                    }).build(k));
                } else if g.to != "clerk" {
                    // the clerk is coordd itself: it has no session for the step to show up in
                    line = line.child(text("not yet in its session").font(k.ty.small).color(k.c.text_faint));
                }
                c = c.child(line);
            }
            Entry::Post(p) => {
                let what = match (&p.step, p.head.as_ref().filter(|h| h.get("head").is_some_and(Value::is_string)).map(|h| short_hashes(&badge_outcome(h)))) {
                    // a goal's Done says the lines that met it; a hand's words are the post's body below
                    (Some(s), Some(h)) => format!(" · answer to step {s}: {h}{}", p.head.as_ref().and_then(met_by).map(|b| format!(" {b}")).unwrap_or_default()),
                    (Some(s), None) => format!(" · on step {s}"),
                    _ => String::new(),
                };
                // a post moved away stays as a stub saying where (design-two-systems §3, Move)
                if let Some(to) = &p.moved {
                    c = c.child(text(format!("{when} · {}'s post #{} moved to {to}", p.by, p.n)).key(format!("post-{}-{}", v.id, p.n)).font(k.ty.small).color(k.c.text_faint));
                    continue;
                }
                let body = words(pg, &serde_json::to_value(&p.text).unwrap_or(Value::Null));
                let mut from = p.from.as_ref().map(|f| format!(" · moved here from {f}")).unwrap_or_default();
                // design §13: the owner's post a handler holds, dropped, or told to others than it was typed to
                if p.dropped {
                    from.push_str(" · dropped: no agent was given it");
                } else if p.held.is_some() {
                    from.push_str(" · held: not given to anyone yet");
                } else if let Some(t) = crate::chat::told_elsewhere(p) {
                    from.push_str(&format!(" · to {t}"));
                }
                let body = if p.dropped { crate::chat::struck(&body) } else { body };
                let (vn, pn) = (v.n, p.n as u32);
                let head = row()
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    // wraps rather than run under the picture beside the ticket
                    .child(text(format!("{when} · {}{what}{from}", p.by)).font(k.ty.small_medium).color(k.c.text_muted).wrap().grow(1.0).shrink())
                    .child(Button::new("Move").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                        s.tickets.moving = if s.tickets.moving == Some((vn, pn)) { None } else { Some((vn, pn)) };
                        s.tickets.move_to = TextEdit::new("");
                    }).build(k));
                let mut card = col()
                    .key(format!("post-{}-{}", v.id, p.n))
                    .w_full()
                    .gap(4.0)
                    .padding(10.0)
                    .radius(k.radius.sm)
                    .fill(k.c.layer_01)
                    .child(head)
                    .child(crate::post::message_body(None, &body, None, None, k));
                if pg.moving == Some((vn, pn)) {
                    card = card.child(move_box(pg, v, pn, k));
                }
                // a note's readers that ended with Nothing, as on a thread (N-19 point 2)
                c = c.child(card).children(crate::threads::quiet_line(v, p, k));
            }
        }
    }
    c
}

/// Where post N of V goes (the owner's Move): a ticket's id, or nothing for a thread of its own; V keeps a stub.
fn move_box(pg: &Page, v: &TicketView, n: u32, k: &Kit) -> El {
    let t = v.id.clone();
    let area = TextArea::new(&pg.move_to).width(None).placeholder("To thread (e.g. N-5)").auto_grow(1, 1).on_edit(|s: &mut App| &mut s.tickets.move_to).build(k);
    let go = Button::new("Move it").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| {
        let to = s.tickets.move_to.text.trim().to_string();
        let req = Request::Move { t: t.clone(), n, to: (!to.is_empty()).then_some(to) };
        if s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Act }) {
            s.tickets.moving = None;
        }
    }).build(k);
    col().w_full().gap(6.0).pt(6.0).child(col().key(format!("move-box-{}-{n}", v.id)).w_full().child(area)).child(row().w_full().justify_end().child(go))
}

/// The one message box on ticket V: a post on it (message_box.rs).
fn post_box(s: &App, v: &TicketView, k: &Kit) -> El {
    let pi = s.cur;
    let default = crate::message_box::Target::Thread { project: pi, t: v.id.clone(), post: None, way: crate::message_box::Way::Here };
    let target = crate::message_box::chosen(s, default.clone());
    let actions = crate::message_box::thread_actions();
    col().key(format!("post-box-{}", v.id)).w_full().gap(8.0).pt(8.0).children(crate::outgoing::dock(s, pi, &target.place(), k)).child(crate::message_box::build(s, &target, &default, actions, k)).children(crate::message_box::said(s, pi, &target.place(), k))
}

/// The owner's changes to open ticket V (A4; repair and rekind stay commands): Cancel; Withdraw its step, a program
/// going on in its place; Edit what it does after its step's answer, from its printed continuation (as `coord show
/// --program`). Each takes a why.
fn change_box(pg: &Page, v: &TicketView, k: &Kit) -> El {
    let n = v.n;
    let mut c = col().w_full().gap(8.0).padding(12.0).radius(k.radius.md).border(1.0, k.c.border_weak);
    let why = pg.why.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
    c = c.child(col().key(format!("why-{}", v.id)).w_full().child(TextInput::new(&why).label("Why (each change says why)").width(None).on_edit(move |s: &mut App| s.tickets.why.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)));
    c = c.child(row().w_full().gap(8.0).center_y().child(text("Cancel it and its open children").font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink()).child(Button::new("Cancel ticket").variant(ButtonVariant::Warning).on_press(move |s: &mut App| change(s, n, Change::Cancel)).build(k)));
    if matches!(v.status, ViewStatus::Awaits { .. }) {
        let w = pg.withdraw.get(&n).cloned().unwrap_or_else(|| TextEdit::new(""));
        c = c
            .child(text("Withdraw its step; say what replaces it").font(k.ty.small).color(k.c.text_muted).wrap())
            .child(col().key(format!("withdraw-{}", v.id)).w_full().child(TextArea::new(&w).width(None).spellcheck(false).placeholder("return Dismiss(\"…\")").auto_grow(2, 8).on_edit(move |s: &mut App| s.tickets.withdraw.entry(n).or_insert_with(|| TextEdit::new(""))).build(k)))
            .child(row().w_full().justify_end().child(Button::new("Withdraw step").variant(ButtonVariant::Neutral).on_press(move |s: &mut App| change(s, n, Change::Withdraw)).build(k)));
    }
    if v.continuation.is_some() {
        // the Edit moved beside the step it follows (N-117 post 25): the step's panel in the picture
        c = c.child(text("Edit what follows its step").font(k.ty.small).color(k.c.text_muted).wrap());
    }
    c
}

/// The step panel's Edit (edit.rs): ticket N's program after its awaited step, as typed there.
pub(crate) fn change_edit(s: &mut App, n: u64) {
    change(s, n, Change::Edit)
}

pub(crate) enum Change {
    Cancel,
    Withdraw,
    Edit,
}

/// Send the change box's WHAT for ticket N; refused here when it has no why or no program.
pub(crate) fn change(s: &mut App, n: u64, what: Change) {
    let Some(v) = s.proj().and_then(|p| p.live.tickets.get(&n)).cloned() else { return };
    let why = s.tickets.why.get(&n).map(|e| e.text.trim().to_string()).unwrap_or_default();
    if why.is_empty() {
        s.tickets.said = Some(Err("Say why first: each change carries its why.".into()));
        return;
    }
    let t = v.id.clone();
    let req = match what {
        Change::Cancel => Request::Cancel { t, why },
        Change::Withdraw => {
            let program = s.tickets.withdraw.get(&n).map(|e| e.text.trim().to_string()).unwrap_or_default();
            if program.is_empty() {
                s.tickets.said = Some(Err("Say what replaces the step.".into()));
                return;
            }
            Request::Withdraw { t, program, why }
        }
        Change::Edit => {
            let typed = s.tickets.edit.get(&n).map(|e| e.text.clone()).or(v.continuation.clone()).unwrap_or_default();
            Request::Edit { t, program: edit_program(&typed), why }
        }
    };
    s.request_in(s.cur, req, Pending::TicketDone { clear: Cleared::Change(n) });
}

/// What an edit sends of the printed continuation TYPED: what follows its in-flight line (its comments and that line
/// may stay), as `coord edit` does.
fn edit_program(typed: &str) -> String {
    match typed.split_once("answer <- (in flight);") {
        Some((_, rest)) => rest.trim_start_matches('\n').to_string(),
        None => typed.to_string(),
    }
}

/// The line "Waits on …" for open ticket V: who has it, down its tree (the steps owed at its leaves, as the machine
/// gives them in the view), as "fixer (repair, F-12)"; the design's first promise. A hidden ticket's step is named
/// without its id, and a stop as what it asks: "owner (whether it runs again)". None when nothing is owed.
pub(crate) fn waits_line(v: &TicketView) -> Option<String> {
    let hidden = |id: &str| id.rsplit('-').next().and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n >= docket_machine::HIDDEN);
    let w: Vec<String> = v
        .waits_on
        .iter()
        .map(|w| match (w.call.as_str(), hidden(&w.ticket)) {
            ("stopped", _) => format!("{} (whether it runs again)", w.agent),
            (call, true) => format!("{} ({call})", w.agent),
            (call, false) => format!("{} ({call}, {})", w.agent, w.ticket),
        })
        .collect();
    (!w.is_empty()).then(|| format!("Waits on {}", w.join("; ")))
}

#[cfg(test)]
mod moved_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn an_open_ticket_says_who_it_waits_on_down_its_tree() {
        let v: TicketView = serde_json::from_value(json!({"id": "C-1", "n": 1, "kind": "claim", "opener": "owner", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "waits", "children": ["F-2"]}, "posts": [], "children": ["F-2"], "program": "", "bound": 1, "spent": 1, "given": [],
            "opened_at": "2026-11-10T21:00:00Z",
            "waits_on": [{"ticket": "P-3", "agent": "f3-1", "call": "fixes"}, {"ticket": "D-5", "agent": "owner", "call": "decide"}]}))
        .unwrap();
        assert_eq!(waits_line(&v).unwrap(), "Waits on f3-1 (fixes, P-3); owner (decide, D-5)");
        let closed = TicketView { waits_on: vec![], ..v };
        assert_eq!(waits_line(&closed), None);
    }

    /// The thread page in his words (paper-research N-1446 p3207, 6; docket-edge8's walk on build 89, p3205): his
    /// one-word thread's title with its id small beside it, its posts as its body, no kind name or state word under it
    /// (the id and state behind the thread's ⋯), no line above the box that only names the thread, the box's words say
    /// thread, and "Open a thread of a kind…" behind the list's ⋯. His hello opens a thread of kind `thread`, as in
    /// docket-edge7's real run (p3214); this record had kind `text`, which no hello of his opens.
    #[test]
    fn the_thread_page_says_thread_and_shows_no_kind_state_or_name_only_line() {
        use rune::runtime::Runtime;
        fn settle(rt: &mut Runtime<App>) {
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
        }
        fn texts(rt: &Runtime<App>) -> Vec<String> {
            rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
        }
        fn press(rt: &mut Runtime<App>, find: impl Fn(&rune::runtime::ElementInfo) -> bool) {
            let all = rt.elements();
            let r = all.iter().find(|e| !e.exiting && find(e)).unwrap_or_else(|| panic!("the press, among {:?}", all.iter().filter_map(|e| e.key.clone()).collect::<Vec<_>>())).rect;
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            settle(rt);
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let at = "2026-11-24T21:21:30Z";
        let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "try-1", "text": "hello"}, "title": "text hello",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "given": [], "opened_at": at,
            "posts": [{"n": 1, "at": at, "by": "owner", "text": "hello", "names": ["try-1"]}, {"n": 2, "at": "2026-11-24T21:21:40Z", "by": "try-1", "text": "Hello. I'm up and waiting for a task."}]}))
        .unwrap();
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let all = texts(&rt);
        assert!(all.iter().any(|t| t == "text hello"), "{all:?}");
        assert!(rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some("title-id-1")), "the id beside the title");
        // his hello opens a thread of kind `thread` (docket-edge7's real run, p3214): its posts are its body
        assert!(all.iter().any(|t| t == "hello") && all.iter().any(|t| t == "Hello. I'm up and waiting for a task."), "the thread's posts: {all:?}");
        assert!(!all.iter().any(|t| t == "resting" || t == "thread" || t.starts_with("N-1 · ")), "no state word or kind name before ⋯: {all:?}");
        assert!(!all.iter().any(|t| t.starts_with("→ ")), "no line that only names the thread: {all:?}");
        assert!(!all.iter().any(|t| t.contains("ticket")), "his words say thread: {all:?}");
        assert!(!all.iter().any(|t| t.starts_with("Open a ")), "behind the list's ⋯: {all:?}");
        // the list's ⋯ is at the foot of the list by time (Outline is the list a thread of his opens on)
        press(&mut rt, |e| e.text.as_deref() == Some("By time"));
        press(&mut rt, |e| e.key.as_deref() == Some("threads-foot-more"));
        assert!(texts(&rt).iter().any(|t| t == "Open a thread of a kind…"), "{:?}", texts(&rt));
        // the menu closed by its ⋯ again (a press outside it closes it and goes no further)
        press(&mut rt, |e| e.key.as_deref() == Some("threads-foot-more"));
        press(&mut rt, |e| e.key.as_deref() == Some("thread-more"));
        let more = texts(&rt);
        assert!(more.iter().any(|t| t.starts_with("N-1 · ") && t.ends_with("resting")), "the id and state behind the thread's ⋯: {more:?}");
    }

    /// One box on a thread's own page, whichever page draws it (paper-research N-1446 p3240; proto-window's walk on build
    /// 91, p3239): a thread his hello opens (kind `thread`, threads.rs) and a page of another kind (here) give the box the
    /// same actions, so the same placeholder and the same button word, Send; no Post.
    #[test]
    fn a_threads_box_has_one_wording_and_one_button_on_both_pages() {
        let a = crate::message_box::thread_actions();
        assert_eq!((a.send.as_deref(), a.placeholder.as_str()), (Some("Send"), crate::message_box::THREAD_PLACEHOLDER));
        for kind in ["thread", "text"] {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            let at = "2026-11-24T21:21:30Z";
            let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": kind, "opener": "owner", "ctx": {}, "form": {"to": "try-1", "text": "hello"}, "title": "text hello",
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "given": [], "opened_at": at,
                "posts": [{"n": 1, "at": at, "by": "owner", "text": "hello", "names": ["try-1"]}]}))
            .unwrap();
            p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(1);
            let mut rt = crate::view_cost::window(s);
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
            let in_box: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("post-box-N-1")).filter_map(|e| e.text.or(e.label)).collect();
            assert!(in_box.iter().any(|t| t == "Send") && !in_box.iter().any(|t| t == "Post"), "kind {kind}: {in_box:?}");
        }
    }

    /// docket-edge7's real-engine still on build 92 (paper-research N-1446 p3296): a thread's header is its title, its id
    /// and ⋯; Docket, Events and Record are behind ⋯. A project with one thread offers no "Archive everything up to
    /// now…" in its list: the press is in the list's ⋯ then, and back in the list once there is a second thread.
    #[test]
    fn engine_detail_and_an_empty_archive_are_behind_the_dots() {
        use rune::runtime::Runtime;
        fn settle(rt: &mut Runtime<App>) {
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
        }
        fn has(rt: &Runtime<App>, key: &str) -> bool {
            rt.elements().into_iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
        }
        fn texts(rt: &Runtime<App>) -> Vec<String> {
            rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
        }
        fn press(rt: &mut Runtime<App>, key: &str) {
            let all = rt.elements();
            let r = all.iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}, among {:?}", all.iter().filter_map(|e| e.key.clone()).collect::<Vec<_>>())).rect;
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            settle(rt);
        }
        let thread = |n: u64, at: &str| -> TicketView {
            serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "try-1", "text": "hello"}, "title": "hello",
                "status": {"is": "resting"}, "children": [], "program": "on owner_post(m) post(m.text);", "bound": 3, "spent": 1, "given": [], "opened_at": at,
                "posts": [{"n": 1, "at": at, "by": "owner", "text": "hello", "names": ["try-1"]}]}))
            .unwrap()
        };
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "try-80".into(), id: "try-80".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(thread(1, "2026-11-25T03:48:00Z")) });
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(has(&rt, "title-id-1") && has(&rt, "thread-more"), "the title's id and ⋯");
        for key in ["thread-docket", "thread-events", "thread-record", "archive-all"] {
            assert!(!has(&rt, key), "{key} not before a click: {:?}", texts(&rt));
        }
        press(&mut rt, "thread-more");
        for key in ["thread-docket", "thread-events", "thread-record"] {
            assert!(has(&rt, key), "{key} behind ⋯");
        }
        press(&mut rt, "threads-foot-more");
        assert!(texts(&rt).iter().any(|t| t == "Archive everything up to now…"), "in the list's ⋯: {:?}", texts(&rt));
        press(&mut rt, "threads-foot-more");
        rt.update(|s, _| {
            s.projects[0].live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Ticket(thread(2, "2026-11-25T03:50:00Z")) });
        });
        settle(&mut rt);
        assert!(has(&rt, "archive-all"), "two threads: the press in the list again");
    }

    #[test]
    fn an_edit_sends_what_follows_the_in_flight_line() {
        assert_eq!(edit_program("// awaits owner\nanswer <- (in flight);\nreturn answer;"), "return answer;");
        assert_eq!(edit_program("return answer;"), "return answer;");
    }

}

/// V's state in a chip, in the words Threads and the Outline give it (outline::state); the accent when it waits on him.
/// A closed ticket's chip holds its result's start (`Fixed: 6217ce824`), the page shows it whole.
fn chip(v: &TicketView, k: &Kit) -> Option<El> {
    let (w, owed) = match &v.status {
        ViewStatus::Done { result } if !empty_result(result) => (crate::refs::cut(&short_hashes(&badge_outcome(result)), 28), false),
        _ => crate::outline::state(v),
    };
    (!w.is_empty()).then(|| Badge::new(w).variant(if owed { BadgeVariant::Accent } else { BadgeVariant::Subtle }).build(k))
}

/// "Broken: why" for a ticket the machine could not run on, in red: this did not happen.
pub(crate) fn broken_line(v: &TicketView, k: &Kit) -> Option<El> {
    v.broken.as_ref().map(|b| text(format!("Broken: {b}")).font(k.ty.small).color(k.c.danger.fg).wrap())
}

/// A result with nothing in it (a note's, `[]`): its chip says the state instead.
fn empty_result(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::String(s) => s.is_empty(),
        _ => false,
    }
}

/// Stills of what N-799 touched (`STILL_OUT=DIR cargo test ticket::stills -- --ignored`), made-up tickets, no coordd:
/// a check's page (its chip says its result, a Broken line, the tree, what happened); a thread's details (its
/// deliveries, not its posts again); a thread with an agent's question its step asked (the one question card); and the
/// check's page in a narrow pane.
#[cfg(test)]
mod stills {
    use serde_json::json;

    fn at(m: u32) -> String {
        format!("2026-11-19T09:{m:02}:00Z")
    }

    fn tickets() -> Vec<serde_json::Value> {
        vec![
            json!({"id": "N-20", "n": 20, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"}, "title": "Runner rows",
                "status": {"is": "resting"}, "children": ["A-21", "K-7"], "program": "", "bound": 1, "spent": 1, "opened_at": at(1),
                "waits_on": [{"ticket": "A-21", "agent": "owner", "call": "answer"}],
                "given": [{"to": "paper-research", "delivery": "N-20.w1", "upto": 1, "at": at(2)}],
                "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the runner's rows: one per job, newest first", "names": ["paper-research"], "told": ["paper-research"]},
                          {"n": 2, "at": at(3), "by": "paper-research", "text": "Started; one question first.", "told": ["owner"]}]}),
            json!({"id": "A-21", "n": 21, "kind": "ask", "parent": "N-20", "opener": "paper-research", "ctx": {}, "form": {"text": "Should a job that waits on the runner show its place in the queue, or only that it waits?"},
                "title": "Should a job that waits on the runner show its place in the queue?",
                "status": {"is": "awaits", "agent": "owner", "call": "answer", "step": 1, "since": at(4), "taken": false, "refusals": 0, "forming": false,
                           "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}},
                "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(4), "given": [], "posts": []}),
            json!({"id": "K-7", "n": 7, "kind": "check", "parent": "N-20", "opener": "paper-research", "ctx": {}, "form": {"commit": "6217ce8247e10fd6d81defe2718c46424c900b51"},
                "title": "check the runner rows at 6217ce8", "status": {"is": "done", "result": {"head": "Green", "commit": "6217ce8247e10fd6d81defe2718c46424c900b51"}},
                "broken": "its kind's program changed while it ran", "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(5), "closed_at": at(8),
                "given": [{"to": "runner", "delivery": "K-7.d1", "upto": 0, "at": at(5)}],
                "posts": [{"n": 1, "at": at(7), "by": "runner", "text": "241 passed, 0 failed", "step": 1, "head": {"head": "Green", "commit": "6217ce8247e10fd6d81defe2718c46424c900b51"}}]}),
        ]
    }

    fn snap(name: &str, size: (f32, f32), show: u64, details: bool, chat: bool) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        thread_local! { static SHOW: std::cell::Cell<(u64, bool, bool)> = const { std::cell::Cell::new((0, false, false)) }; }
        SHOW.with(|c| c.set((show, details, chat)));
        fn init() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "standin".into(), id: "standin".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            for (i, v) in tickets().into_iter().enumerate() {
                p.live.apply(coord_types::Event { seq: i as u64 + 1, body: coord_types::Body::Ticket(serde_json::from_value(v).unwrap()) });
            }
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Tickets;
            let (show, details, chat) = SHOW.with(|c| c.get());
            if chat {
                // the agent's chat, its stream with him (streams.rs `Which::You`, the default)
                let a: coord_types::AgentView = serde_json::from_value(json!({"id": "", "name": "paper-research", "parent": "", "kind": "", "model": "", "state": "idle",
                    "since": at(0), "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
                s.projects[0].live.apply(coord_types::Event { seq: 10, body: coord_types::Body::Agent(a) });
                s.sel = crate::Sel::Agent("paper-research".into());
                return s;
            }
            s.tickets.shown = Some(show);
            if details {
                s.tickets.details.insert(show);
                s.tickets.more.insert(show);
            } else {
                s.tickets.more.insert(show);
            }
            s
        }
        fn poll(s: &mut crate::App, _: f64, _: &mut rune::Tx) -> bool {
            crate::texts::poll(s);
            false
        }
        let size_arg = format!("{}x{}", size.0 as u32, size.1 as u32);
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), size_arg, "--at".into(), "2.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), poll: Some(poll), ..rune::AppSpec::new("yaaihh", size, init, crate::view) }).args(args).try_run().unwrap();
    }

    #[test]
    #[ignore]
    fn stills_check_page() {
        snap("check-page.png", (1400.0, 900.0), 7, false, false);
    }

    #[test]
    #[ignore]
    fn stills_thread_details() {
        snap("thread-details.png", (1400.0, 900.0), 20, true, false);
    }

    #[test]
    #[ignore]
    fn stills_thread_question() {
        snap("thread-question.png", (1400.0, 900.0), 20, false, false);
    }

    /// The owner's post 1 of N-20 in paper-research's chat: named "you" as on the thread (thread-question.png).
    #[test]
    #[ignore]
    fn stills_chat_you() {
        snap("chat-you.png", (1400.0, 900.0), 20, false, true);
    }

    #[test]
    #[ignore]
    fn stills_check_page_narrow() {
        snap("check-page-narrow.png", (640.0, 900.0), 7, false, false);
    }
}
