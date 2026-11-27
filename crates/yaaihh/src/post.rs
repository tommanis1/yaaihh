//! One drawing of a post (paper-research N-800 step 2). Threads, a run's thread lines, an agent's With you and With
//! agents, and With you's blocks and lists all draw a post here; what differs is where it stands ([`Form`]), not how
//! it is drawn. Before, threads.rs (`message`, `post_body`) and streams.rs (`message`) each drew it, and they had
//! drifted: "you" on one, "You" on the other; actions, folds and boxed asks only on Threads.
//!
//! The parts, each once: who (`refs::person`), the head (who, when, the actions under the pointer), the words (his in
//! the info fill at the right; an agent's in a card, its asks boxed), what became of it behind `⋯`, and the folded
//! line.

use crate::streams::Which;
use crate::{App, Proj};
use coord_types::TicketView;
use docket_machine::Post;
use rune::{El, col, row, text};
use rune_kit::Kit;
use serde_json::Value;

/// Where a post stands, which decides what is drawn around its words.
pub enum Form<'a> {
    /// On its thread (Threads; With you's thread shown whole): its head always, with Take back and the actions.
    Thread,
    /// A line of a run's thread (ran.rs): the line above it says who and when; what became of it, its star, and
    /// under it the comments and who has not read it.
    Line,
    /// In agent AGENT's stream WHICH: a head where a group begins (HEAD) or under the pointer, the message it answers
    /// quoted (TO), a long message cut, and at a group's end (LAST) his message's delivery; ID: its thread's id on
    /// hover; OPEN: his message has no answer yet.
    Stream { agent: &'a str, which: Which, head: bool, to: Option<(u64, u32)>, id: bool, last: bool, open: bool },
}

/// Its words; a long message in a stream shows its start and opens whole on a click.
const LONG: usize = 1400;
use crate::streams::MEASURE;

/// Post X of V in project P, drawn where FORM says; his message while what runs on it runs, as [`sending`].
pub fn post(s: &App, p: &Proj, v: &TicketView, x: &Post, form: &Form, k: &Kit) -> El {
    let el = posted(s, p, v, x, form, k);
    let held = (x.held.is_some() && !x.dropped) || still_sending().is_some_and(|id| id == format!("{}.p{}", v.id, x.n));
    if x.by == "owner" && held { sending(el, k) } else { el }
}

/// YAAIHH_SENDING=N-1.p2: that post of his drawn as Sending, for stills.
fn still_sending() -> Option<&'static str> {
    static ID: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    ID.get_or_init(|| std::env::var("YAAIHH_SENDING").ok()).as_deref()
}

/// Sending (window-rules §5, paper-research N-815 post 9): his message stands where it will stand, faded, with one small
/// ring turning beside it; its hover says "Sending". No buttons, no timer, no second surface: when it has run it
/// stands as sent.
pub fn sending(el: El, k: &Kit) -> El {
    use rune_kit::Spinner;
    row().w_full().gap(6.0).justify_end().items_end().help("Sending").child(col().grow(1.0).shrink().min_w(0.0).opacity(0.55).child(el)).child(row().pb(10.0).child(Spinner::new().size(10.0).build(k)))
}

/// His words in his bubble, on the right of a conversation (window rules §6: him on the right in a bubble, the agent
/// on the left, plain): a post of his on a thread, his prompt or his message in an agent's session, the one drawing.
/// FOUND rings it when a search brought it into view.
pub fn bubble(words: El, found: bool, k: &Kit) -> El {
    col().max_w(620.0).px(14.0).py(10.0).radius(k.radius.md).fill(k.c.info.bg).border(if found { 1.5 } else { 0.0 }, k.c.border_focus).child(words)
}

/// His bubble for TEXT, drawn as WORDS: as wide as its words up to the reading width, and at the reading width when
/// they hold a block (a quote, a list, a table, code), which gives the bubble no width of its own. Without it, his
/// comment's quote drew one or two words wide and ran out below the bubble (docket-edge8, N-1418).
pub fn bubble_for(text: &str, words: El, found: bool, k: &Kit) -> El {
    let b = bubble(words, found, k);
    if holds_block(text) { b.w_full() } else { b }
}

/// Whether markdown TEXT holds a block: a line that starts a quote, a list item, a table row or a code fence.
pub fn holds_block(text: &str) -> bool {
    text.lines().map(str::trim_start).any(|l| {
        l.starts_with('>') || l.starts_with("```") || l.starts_with('|') || ["- ", "* ", "+ "].iter().any(|b| l.starts_with(b)) || l.split_once(". ").is_some_and(|(n, _)| !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// What ran on his message (paper-research N-815 post 9; docket-edge2 fills it from the words ticket and docket-edge4's
/// split): the passages changed, and the points it was split into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sending {
    pub changes: Vec<Change>,
    pub points: Vec<String>,
}

/// One changed passage: bytes AT of the text as sent, what he typed (or said) there.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub at: std::ops::Range<usize>,
    pub typed: String,
}

/// His words as sent, each changed passage drawn in place: what he typed struck in the support level, the words now
/// right after it (window-rules §5). One text, so it wraps as his message does.
pub fn changed_words(sent: &str, changes: &[Change], k: &Kit) -> El {
    let mut shown = String::new();
    let mut strikes = Vec::new();
    let mut at = 0;
    let mut cs: Vec<&Change> = changes.iter().filter(|c| c.at.start <= c.at.end && c.at.end <= sent.len() && sent.is_char_boundary(c.at.start) && sent.is_char_boundary(c.at.end)).collect();
    cs.sort_by_key(|c| c.at.start);
    for c in cs {
        if c.at.start < at {
            continue;
        }
        shown.push_str(&sent[at..c.at.start]);
        let from = shown.len();
        shown.push_str(&c.typed);
        strikes.push((from, shown.len(), Some(k.c.text_muted)));
        if !c.typed.is_empty() && !c.at.is_empty() {
            shown.push(' ');
        }
        shown.push_str(&sent[c.at.clone()]);
        at = c.at.end;
    }
    shown.push_str(&sent[at..]);
    text(shown).font(k.ty.body).color(k.c.text_base).wrap().strikes(strikes)
}

fn posted(s: &App, p: &Proj, v: &TicketView, x: &Post, form: &Form, k: &Kit) -> El {
    match form {
        Form::Line => drawn(s, p, v, x, form, k),
        // the actions show while the pointer is on it (N-466 rule 3)
        Form::Thread | Form::Stream { .. } => {
            let at = (v.n, x.n);
            let el = rune::el().key(format!("hover-{}-{}", v.id, x.n)).w_full().child(drawn(s, p, v, x, form, k)).on_hover_change(move |s: &mut App, inside: bool, _: &mut rune::Tx| {
                if inside {
                    s.tickets.hover_post = Some(at);
                } else if s.tickets.hover_post == Some(at) {
                    s.tickets.hover_post = None;
                }
            });
            match form {
                Form::Stream { head, last, id, .. } => {
                    let el = if *id { el.help(v.id.clone()) } else { el };
                    row().w_full().px(24.0).pt(if *head { 16.0 } else { 12.0 }).pb(if *last { 2.0 } else { 0.0 }).child(col().w_full().max_w(MEASURE).child(el))
                }
                _ => el,
            }
        }
    }
}

/// A message's words, drawn the one way every view draws them (window rules §7; the owner, N-1104 post 300: "on the
/// one message render eh single path"): TEXT as Markdown, tables and all, with the ids in it drawn as their labels
/// (`threads::mentioned`, when P is the project it is in) and the paths it names as links (`paths::linkify`), a click
/// opening either (a path in CWD). COMMENTS, the app and the words' source (`N-221.p5`, `AGENT@OFFSET`, a mail's id),
/// has his comments on them highlighted where they stand (`ask::marks`). A post on a thread, With you, With agents,
/// the work behind a reply, a ticket's posts, mail, a question's words, the summary an agent leaves at a compaction and
/// the whole of a step's long field all come here; no view prepares a message's text itself.
///
/// Built once and kept until what it is built from changes (Rune's memo, docket-edge5's N-1272): the words with their
/// links' labels, the folder, the marks on its source; the kit follows the system's dark mode, which a memo's version
/// holds already. A key in a box, or an update that leaves the words as they were, reuses it: built each frame, a
/// thread of 300 posts built every body again on each key (his N-1104). The words are in its id, so a post's quote and
/// the rest of it are two bodies; nothing is built per frame.
///
/// A table wider than the column keeps its columns and scrolls sideways inside the message, never the page (window
/// rules §6); a code block keeps its lines whole the same way (the kit's `scroll_wide` is the one switch for both).
pub fn message_body(p: Option<&Proj>, text: &str, comments: Option<(&App, &str)>, cwd: Option<String>, k: &Kit) -> El {
    message_part(p, text, comments, None, cwd, k)
}

thread_local! {
    /// [`as_drawn`] of the texts drawn lately, by all it reads: kept across views, so a key in a box, or a frame that
    /// changes nothing they show, does not make every post's words again (the owner's N-1793, item 1: 2.4 ms a key in
    /// With you on a thread of 300 posts, for the 36 posts built).
    static DRAWN: std::cell::RefCell<std::collections::HashMap<u64, std::rc::Rc<str>>> = Default::default();
}

/// The most texts [`DRAWN`] holds; past it, it starts again empty.
const DRAWN_MOST: usize = 4096;

/// TEXT's words as drawn: paths as links, ids as their labels. Kept by everything it reads ([`DRAWN`]): the words, the
/// project, the writer's folder, what coordd said of paths (`paths::known_rev`), its tickets and agents (labels,
/// names), the labels coordd gave, and whether the window draws as Windows does (`elsewhere::away`).
fn as_drawn(p: Option<&Proj>, text: &str, cwd: Option<&str>) -> String {
    let Some(pj) = p else { return as_drawn_now(p, text, cwd) };
    let key = crate::version::Version::plain().add((text, &pj.id, cwd, crate::paths::known_rev(), pj.live.tickets_rev, pj.live.agents_rev, pj.labels.rev(), crate::elsewhere::away())).finish();
    if let Some(d) = DRAWN.with_borrow(|m| m.get(&key).cloned()) {
        return d.to_string();
    }
    let d = as_drawn_now(p, text, cwd);
    DRAWN.with_borrow_mut(|m| {
        if m.len() >= DRAWN_MOST {
            m.clear();
        }
        m.insert(key, d.as_str().into());
    });
    d
}

#[cfg(test)]
thread_local! {
    /// The texts [`as_drawn_now`] made, for the tests that count them per key (view_cost.rs).
    pub(crate) static MADE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// [`as_drawn`] made now.
fn as_drawn_now(p: Option<&Proj>, text: &str, cwd: Option<&str>) -> String {
    #[cfg(test)]
    MADE.with(|n| n.set(n.get() + 1));
    // a file it names is a link once coordd has found it, else its words alone (paths.rs `linkify_found`)
    // and a picture it names, found, is drawn as the picture (thumbs.rs; the owner's walk item 16)
    let linked = match p {
        Some(p) => crate::thumbs::as_pictures(&crate::paths::linkify_found(text, &p.id, cwd)),
        None => crate::paths::linkify(text),
    };
    match p {
        Some(p) => crate::threads::mentioned(p, &linked),
        None => linked,
    }
}

/// One part of a source's words, as [`message_body`] draws them whole: EARLIER, the words of the same source drawn
/// before it (his reply's quoted line; an asking post's lines above), whose passages are lit there and not again here
/// (`ask::marks_in`; proto-window, N-1477 p8). While the comment box stands in the flow at a passage here, the words
/// split after it with the box's room between (`ask::split`; paper-research N-1477 p9): the lines after it move down
/// under the box, nothing covered.
pub fn message_part(p: Option<&Proj>, text: &str, comments: Option<(&App, &str)>, earlier: Option<&str>, cwd: Option<String>, k: &Kit) -> El {
    let src = as_drawn(p, text, cwd.as_deref());
    let earlier = match (comments, earlier) {
        (Some(_), Some(e)) if !e.is_empty() => as_drawn(p, e, cwd.as_deref()),
        _ => String::new(),
    };
    let source = comments.map_or("", |(_, source)| source);
    let mut v = crate::version::Version::default();
    v.add((&src, &cwd, k.dark, &earlier));
    // its pictures as they draw now: the post drawn again when one comes or decodes (thumbs.rs)
    if let Some(p) = p {
        v.add(crate::thumbs::state(&src, |l| crate::paths::found(&p.id, l, cwd.as_deref())));
    }
    if let (Some(p), Some((s, source))) = (p, comments) {
        crate::ask::marks_version(s, p, source, v.hasher());
    }
    // in the split, its pane too: a post drawn in two panes is two bodies, each its own memo, the comment box's
    // passage lit in one (`ask::ours`; proto-window N-1477 p29)
    let pane = crate::ask::in_split(comments.and_then(|(s, _)| s.notes.drawing.get()));
    let id = format!("body-{source}-{:x}{pane}", crate::version::Version::plain().add(text).finish());
    if let Some((s, _)) = comments {
        crate::ask::drawn(s, &id, &src);
    }
    let here = crate::ask::Here { body: Some(id.as_str()), earlier: &earlier };
    let pid = p.map(|p| p.id.clone());
    let build = |src: String, marks: Vec<crate::ask::Mark>| {
        let (cwd, pid) = (cwd.clone(), pid.clone());
        // a picture it names: drawn small where it names it, a click opening it in the desktop's own viewer (thumbs.rs)
        let (icwd, ipid) = (cwd.clone(), pid.clone());
        let full = move |l: &str| ipid.as_deref().and_then(|pid| crate::paths::found(pid, l, icwd.as_deref()));
        let full2 = full.clone();
        rune_kit::markdown::Markdown::new(src)
            .images(move |l| full(l).and_then(|f| crate::thumbs::picture(&f)))
            .image_frame(move |e, f| match full2(f.src) {
                Some(path) => e.max_w(crate::thumbs::THUMB_W).cursor(rune::Cursor::Pointer).help("Open in your picture viewer").on_press(move |_s: &mut App| crate::thumbs::open_outside(&path)),
                None => e,
            })
            .highlights(marks)
            .selectable(true)
            .scroll_wide(true)
            .link_style(crate::mentions::look(k))
            .on_link(move |s: &mut App, url| {
                // a file it names opens on the Files page (the owner's walk, item 9)
                if !crate::mentions::open(s, url) && !pid.as_deref().is_some_and(|pid| crate::paths::open_found(s, pid, url, cwd.as_deref())) {
                    crate::paths::open(url, cwd.clone())
                }
            })
            .build(k)
    };
    if let (Some(p), Some((s, source))) = (p, comments)
        && let Some(cut) = crate::ask::cut(s, source, &src, here)
    {
        // each part kept as a body is, the box's room built each frame between them
        let marks = crate::ask::marks_in(s, p, source, &src, here, k);
        let whole = v.add(cut).finish();
        return crate::ask::split(s, source, &src, &marks, cut, &id, |part, marks, id| rune::memo::memo_with_hash(id, whole, || build(part, marks)));
    }
    rune::memo::memo_with_hash(id.clone(), v.finish(), || {
        #[cfg(test)]
        crate::threads::BODIES.with_borrow_mut(|b| {
            if !source.is_empty() {
                b.push(source.to_string())
            }
        });
        let marks = match (p, comments) {
            (Some(p), Some((s, source))) => crate::ask::marks_in(s, p, source, &src, here, k),
            _ => Vec::new(),
        };
        build(src, marks)
    })
}

/// Markdown's marks off, the lines joined: a post's words as one line of a list or a block.
pub fn plain(md: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for l in md.lines() {
        let t = l.trim();
        // a table's rule line says nothing
        if t.is_empty() || t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }
        let t = t.trim_start_matches(['>', '#', '-', '*', '|', ' ']).trim_end_matches(['|', ' ']);
        out.push(t.replace("**", "").replace('`', "").replace(" | ", " · "));
    }
    out.join(" ")
}

/// What the engine says of X, behind `⋯` (N-466 rule 4): moved here, dropped, held, to whom; but on a run's line
/// (LINE), where its own lines say them, read with no reply and the step it answers.
fn notes(v: &TicketView, x: &Post, line: bool) -> Vec<String> {
    let mut n: Vec<String> = Vec::new();
    if let Some(f) = &x.from {
        n.push(format!("moved here from {f}"));
    }
    if x.dropped {
        n.push("dropped: no agent was given it".into());
    } else if x.held.is_some() {
        // drawn as Sending, said once (window-rules §5)
    } else if let Some(t) = crate::chat::told_elsewhere(x) {
        n.push(format!("to {t}"));
    }
    if line {
        return n;
    }
    let quiet = crate::threads::quiet_on(v, x);
    if !quiet.is_empty() {
        n.push(format!("read, no reply: {}", quiet.join(", ")));
    }
    if let (Some(s), Some(h)) = (&x.step, x.head.as_ref()) {
        n.push(format!("answer to step {s}: {}", h.get("head").and_then(Value::as_str).unwrap_or("")));
    }
    n
}

/// A message that begins with a quote (his comment on a passage, `> ...`): the quote and his words after it.
fn split_quote(body: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = body.lines().collect();
    let n = lines.iter().take_while(|l| l.trim_start().starts_with('>')).count();
    if n == 0 {
        return None;
    }
    let rest = lines[n..].join("\n").trim().to_string();
    if rest.is_empty() {
        return None;
    }
    Some((lines[..n].join("\n"), rest))
}

/// Quote Q cut to about three lines, its source "(from ...)" kept; whether it was cut.
fn cut_quote(q: &str) -> (String, bool) {
    const THREE: usize = 280;
    let plain: Vec<String> = q.lines().map(|l| l.trim_start().trim_start_matches('>').trim().to_string()).collect();
    let all = plain.iter().filter(|l| !l.is_empty()).cloned().collect::<Vec<_>>().join(" ");
    let lines = plain.iter().filter(|l| !l.is_empty()).count();
    if all.chars().count() <= THREE && lines <= 3 {
        return (q.to_string(), false);
    }
    let (words, from) = match all.rfind(" (from ") {
        Some(i) if all.ends_with(')') => (&all[..i], &all[i..]),
        _ => (all.as_str(), ""),
    };
    let mut cut: String = words.chars().take(THREE).collect();
    if let Some(i) = cut.rfind(' ') {
        cut.truncate(i);
    }
    (format!("> {}…{from}", cut.trim_end()), true)
}

/// BODY cut near LONG at a paragraph's end, unless it is open whole; whether it was cut.
fn cut_long(body: &str, whole: bool) -> (String, bool) {
    if body.len() <= LONG || whole {
        return (body.to_string(), false);
    }
    let mut i = LONG;
    while !body.is_char_boundary(i) {
        i -= 1;
    }
    let at = body[..i].rfind("\n\n").filter(|j| *j > LONG / 2).unwrap_or(i);
    (format!("{}…", &body[..at]), true)
}

/// A faint line that opens KEY in `streams.open` on a click.
fn opener(label: &str, key: String, k: &Kit) -> El {
    text(label.to_string()).font(k.ty.small).color(k.c.text_faint).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
        s.streams.open.insert(key.clone());
    })
}

/// The head of X in FORM: who and when, and at its right edge the actions while they show; in a stream, also the
/// message it answers and the work behind the agent's reply.
fn head(s: &App, p: &Proj, v: &TicketView, x: &Post, form: &Form, folded: bool, k: &Kit) -> Option<El> {
    let mark = if folded { " ▸" } else { "" };
    let who = crate::refs::person(&x.by);
    match form {
        Form::Thread => {
            let when = crate::when::at(&x.at.to_string());
            let h = row().gap(10.0).center_y();
            Some(if x.by == "owner" {
                // Take back before it (edge2-box), with the actions while the pointer is on it (reveal little; consult-3,
                // N-412 p2113); the actions after it, the head at the right
                h.children(crate::subthread::shown(s, v, x.n).then(|| crate::threads::take_back(s, p, v, x, k)).flatten()).child(crate::subthread::head(&who, &format!("{when}{mark}"), k)).children(crate::subthread::actions(s, v, x.n, &x.by, k))
            } else {
                // a post to other agents says so beside its sender, quiet (yaaihh/fresh's Go, N-1751 p2)
                let to = to_agents(x).map(|t| text(format!("to {t}")).key(format!("to-{}-{}", v.id, x.n)).font(k.ty.small).color(k.c.text_faint));
                h.w_full()
                    .child(crate::subthread::head(&who, &format!("{when}{mark}"), k))
                    .children(to)
                    .children(by_agent(p, x).then(|| work_toggle(s, p, v, x, k)))
                    .child(rune::el().grow(1.0))
                    .children(crate::subthread::actions(s, v, x.n, &x.by, k))
            })
        }
        Form::Stream { agent, head, to, .. } => {
            let acts = stream_actions(s, p, v, x, agent, k);
            if !head && acts.is_none() {
                return None;
            }
            let mut h = row().w_full().gap(8.0).center_y().child(text(who).font(k.ty.small_medium).color(k.c.text_base));
            // its time opens its thread
            let n = v.n;
            h = h.child(crate::when::el(&x.at.to_string()).font(k.ty.small).color(k.c.text_muted).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::threads::show(s, n)));
            if folded {
                h = h.child(text("▸").font(k.ty.small).color(k.c.text_muted));
            }
            if let Some((t, m)) = to
                && let Some(tv) = p.live.tickets.get(t)
            {
                let at = tv.posts.iter().find(|y| y.n == *m).map(|y| crate::when::at(&y.at.to_string())).unwrap_or_default();
                h = h.child(text(format!("↳ your {at} “{}”", crate::streams::first_words(s, tv, *m, 60))).font(k.ty.small).color(k.c.text_muted));
            }
            if x.by == *agent {
                h = h.child(work_toggle(s, p, v, x, k));
            }
            Some(h.child(rune::el().grow(1.0)).children(acts))
        }
        Form::Line => {
            // what became of it, its star, at the line's top (the line above says who and when)
            let n = notes(v, x, true).join(" · ");
            Some(row().w_full().gap(8.0).center_y().child(text(n).font(k.ty.small).color(k.c.text_muted).grow(1.0)).children(crate::threads::take_back(s, p, v, x, k)).child(crate::find::star(v, Some(x.n), k)))
        }
    }
}

/// Whom X is to when it is to other agents alone: the agents it names, he not among them (his walk item 15; yaaihh/
/// fresh's Go, N-1751 p2). A thread's page says it beside its sender and draws its words quieter. None for his own
/// post, one that names him, and one that names nobody.
pub(crate) fn to_agents(x: &Post) -> Option<String> {
    (x.by != "owner" && !x.names.is_empty() && !x.names.iter().any(|n| n == "owner")).then(|| x.names.join(", "))
}

/// "› the work behind it" on agent X's post X of V in P: the turn that led to it (turn.rs), opened apart over the chat
/// (popup.rs), which keeps its place.
fn work_toggle(s: &App, p: &Proj, v: &TicketView, x: &Post, k: &Kit) -> El {
    let pi = s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur);
    let post = format!("{}.p{}", v.id, x.n);
    let title = crate::popup::title(&crate::refs::ticket(Some(p), v).label, &post, &format!("the work behind {}'s post", x.by));
    let key = format!("work-{post}");
    let what = crate::popup::What::Work { pi, agent: x.by.clone(), post, at: x.at.to_string() };
    text("› the work behind it").key(key).font(k.ty.small).color(k.c.text_faint).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::popup::open(s, title.clone(), what.clone()))
}

/// Whether X is by an agent of P, whose session holds the turn behind it.
fn by_agent(p: &Proj, x: &Post) -> bool {
    p.live.agents.iter().any(|a| a.name == x.by)
}

/// In a stream, X's actions while the pointer is on it: `Reply` puts the agent's box on its thread (the box says so,
/// ✕ undoes it), `⋯` holds star, fold and the rest, as on Threads.
fn stream_actions(s: &App, p: &Proj, v: &TicketView, x: &Post, agent: &str, k: &Kit) -> Option<El> {
    let at = Some((v.n, x.n));
    if s.tickets.hover_post != at && s.tickets.post_more != at {
        return None;
    }
    let pi = s.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(s.cur);
    let on = crate::withyou::bound(s, pi, agent).as_deref() == Some(v.id.as_str()) && s.withyou.on.contains_key(&(pi, agent.to_string()));
    let (a, t) = (agent.to_string(), v.id.clone());
    let reply = k.interactive(row().key(format!("act-Reply-{}-{}", v.id, x.n)).child(text("Reply").font(k.ty.small).color(if on { k.c.text_base } else { k.c.text_muted }))).on_press(move |s: &mut App| crate::withyou::bind(s, pi, &a, &t));
    let pn = x.n;
    let vn = v.n;
    let more = k.interactive(row().key(format!("more-{}-{pn}", v.id)).child(text("⋯").font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| {
        s.tickets.post_more = if s.tickets.post_more == Some((vn, pn)) { None } else { Some((vn, pn)) };
    });
    Some(row().key(format!("acts-{}-{pn}", v.id)).gap(10.0).center_y().child(reply).child(more))
}

/// X's head, words and foot, as FORM places them.
fn drawn(s: &App, p: &Proj, v: &TicketView, x: &Post, form: &Form, k: &Kit) -> El {
    let pg = &s.tickets;
    let key = format!("post-{}-{}", v.id, x.n);
    let source = format!("{}.p{}", v.id, x.n);
    if let Some(to) = &x.moved {
        let when = crate::when::at(&x.at.to_string());
        return text(format!("{when} · {}'s message moved to {to}", crate::refs::person(&x.by))).key(key).font(k.ty.small).color(k.c.text_faint);
    }
    let line = matches!(form, Form::Line);
    let stream = match form {
        Form::Stream { agent, which, last, open, .. } => Some((*agent, *which, *last, *open)),
        _ => None,
    };
    let (vn, pn) = (v.n, x.n);
    let folded = !line && pg.folded.contains(&(vn, pn));
    let state = crate::threads::state(pg, x);
    let loading = state == crate::texts::TextState::Loading;
    let body = state.words();
    let mine = x.by == "owner";
    let found = pg.find.found == Some((vn, pn));
    let mut c = col().key(key.clone()).w_full().gap(if mine { 4.0 } else { 6.0 });
    if mine && !line {
        c = c.items_end();
    }
    if line && found {
        c = c.padding(8.0).radius(k.radius.md).border(1.5, k.c.border_focus);
    }
    c = c.children(head(s, p, v, x, form, folded, k));
    if !line {
        c = c.children(crate::subthread::more(s, v, x.n, &x.by, &notes(v, x, false).iter().map(|n| format!(" · {n}")).collect::<String>(), k));
    }
    // its words still being read: a quiet line where they will stand, never a bubble holding "…" (the owner, N-1790)
    if loading {
        return c.child(text(crate::texts::LOADING).font(k.ty.small).color(k.c.text_faint).key(format!("said-{}-{}", v.id, x.n)));
    }
    if folded {
        let fold = move |s: &mut App| {
            if !s.tickets.folded.remove(&(vn, pn)) {
                s.tickets.folded.insert((vn, pn));
            }
        };
        let r = if mine { row().max_w(560.0) } else { row().w_full() };
        return c.child(k.interactive(r.child(text(crate::sidebar::first_line(&body)).font(k.ty.small).color(k.c.text_muted).truncate())).on_press(fold));
    }
    // a span whose place could not be read, or read empty: one quiet line, not an empty card (paper-research N-101 p1758)
    if crate::threads::unread(&body) {
        return c.child(text("text not available").font(k.ty.small).color(k.c.text_faint));
    }
    let cwd = p.live.agents.iter().find(|a| a.name == x.by).and_then(|a| a.cwd.clone());
    // in a stream: a long message cut, and his comment's quote cut to three lines (paper-research N-101.p1363)
    let whole_key = source.clone();
    let quote_key = format!("{source}-quote");
    let (body, long_cut) = if stream.is_some() { cut_long(&body, s.streams.open.contains(&whole_key)) } else { (body, false) };
    // his answer to a post that asks him, in its own place in the thread, the same: the line it answers, then his
    // words (N-1396 p53)
    let words = |src: String| -> El {
        let quoted = if stream.is_some() || x.answers.is_some() { split_quote(&src) } else { None };
        match quoted {
            Some((q, rest)) => {
                let (q, qcut) = if s.streams.open.contains(&quote_key) { (q, false) } else { cut_quote(&q) };
                let mut qc = col().w_full().gap(2.0).child(message_body(Some(p), &q, Some((s, source.as_str())), cwd.clone(), k));
                if qcut {
                    qc = qc.child(opener("› the whole quote", quote_key.clone(), k));
                }
                col().w_full().gap(6.0).child(qc).child(message_part(Some(p), &rest, Some((s, source.as_str())), Some(&q), cwd.clone(), k))
            }
            None => message_body(Some(p), &src, Some((s, source.as_str())), cwd.clone(), k),
        }
    };
    if mine {
        let body = if x.dropped { crate::chat::struck(&body) } else { body };
        if line {
            c = c.child(words(body));
        } else {
            c = c.child(bubble_for(&body, words(body.clone()), found, k).key(format!("said-{}-{}", v.id, x.n)));
        }
        // his words cleaned: what he typed, behind one click (N-487)
        c = c.children(crate::words::as_typed(s, p, v, x, k));
        // the points of it he sent elsewhere, each with Bring back (split.rs)
        c = c.children(crate::split::lines(s, p, v, x, k).into_iter().map(|l| if line { l } else { l.max_w(620.0) }));
    } else {
        // the agent's words on the left, plain, at the column's width (window rules §6); its asks boxed where it asks
        // something; a chip clicked in a stream goes to the thread, where its box is
        let mut card = col().w_full().gap(8.0);
        if !line {
            card = card.key(format!("said-{}-{}", v.id, x.n));
        }
        if found && !line {
            card = card.padding(8.0).radius(k.radius.md).border(1.5, k.c.border_focus);
        }
        // an ask typed to him marks no passage: its words as they are shown here (cut in a stream), in one box
        let typed = crate::threads::typed_ask(x);
        match crate::threads::read(pg, x).filter(|_| !x.asks.is_empty()) {
            Some(t) => card = card.children(crate::threads::drawn(s, p, v, x, &t, cwd.clone(), stream.is_some(), k)),
            None if typed => card = card.children(crate::threads::drawn(s, p, v, x, &body, cwd.clone(), stream.is_some(), k)),
            None => card = card.child(words(body)),
        }
        if !x.asks.is_empty() || typed {
            card = card.child(crate::threads::card_foot(v, x, k));
        }
        // on a thread's page, a post to other agents alone is drawn quieter than what is his (its head says whom it
        // is to; yaaihh/fresh's Go, N-1751 p2)
        if matches!(form, Form::Thread) && to_agents(x).is_some() {
            card = card.opacity(0.6);
        }
        c = c.child(card);
    }
    if long_cut {
        c = c.child(opener("› the whole message", whole_key, k));
    }
    // the comment box's room at its end, when he commented on it where no passage of it is lit (I-1476 p9)
    c = c.children(crate::ask::room_at(s, &key));
    if line {
        return c.children(crate::also::on_line(p, x, k)).children(crate::passage::under(s, p, v, x, k)).children(crate::whereabouts::line(s, p, v, x, k).or_else(|| crate::threads::quiet_line(v, x, k))).children(crate::threads::not_reached_line(s, p, v, x, k));
    }
    if let Some((agent, which, last, open)) = stream {
        // the foot, only where it says something: a star for him; his message's delivery where the group ends or it
        // stands apart
        let mut foot: Vec<El> = Vec::new();
        if let Some(star) = crate::stars::mark(p, v, x.n) {
            foot.push(text(star).font(k.ty.small).color(k.c.text_accent));
        }
        if mine && which == Which::You {
            let ag = p.live.agents.iter().find(|ag| ag.name == agent);
            let mut d = crate::streams::delivery(ag.map(|ag| ag.state.as_str()), v, x, agent);
            let given = d.starts_with("given");
            // waiting for a running turn's end: the working row under the stream says it, once
            let on_row = ag.is_some_and(|ag| ag.state == "working" && crate::streams::now(ag).is_some()) && d.starts_with("waiting: ");
            // no urgency while the agent is at work on it (the owner, N-1790 part 3)
            if open && !crate::streams::working_on(p, v, x.n, agent) {
                d.push_str(" · no answer yet");
            }
            if (last || !given || open) && !on_row {
                foot.push(text(d).font(k.ty.small).color(k.c.text_faint));
            }
        }
        if !foot.is_empty() {
            c = c.child(row().gap(10.0).center_y().children(foot));
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::{cut_quote, split_quote};

    #[test]
    fn his_comment_quotes_three_lines_and_keeps_its_source() {
        let body = "> @docket-core4: in the live run on Tovrin-dev the assign prompt reads, literally:\n>\n> The agents of this project: {}\n>\n> {{ agents }} in plan/library/assign.txt prints the Agents value as its JSON, not the agents. The same value is the argument of take. Read where an Agents argument is rendered into a prompt and say on N-101 whether this is the engine's rendering or the program's (from N-101.p1256, paper-research's message at 21:41)\n\nthis too why this in my thing";
        let (q, rest) = split_quote(body).unwrap();
        assert_eq!(rest, "this too why this in my thing");
        let (c, cut) = cut_quote(&q);
        assert!(cut && c.chars().count() < 380 && c.ends_with("(from N-101.p1256, paper-research's message at 21:41)"), "{c}");
        assert_eq!(cut_quote("> short"), ("> short".to_string(), false));
        assert!(split_quote("no quote here").is_none());
    }

    #[test]
    fn plain_drops_markdown_marks_and_joins_the_lines() {
        assert_eq!(super::plain("> **E6** is in\n\n| a | b |\n|---|---|\n- `x` done"), "E6 is in a · b x done");
    }

    #[test]
    fn a_long_message_is_cut_at_a_paragraph_and_opens_whole() {
        let body = format!("{}\n\n{}", "a".repeat(1000), "b".repeat(1000));
        let (cut, was) = super::cut_long(&body, false);
        assert!(was && cut.ends_with('…') && !cut.contains('b'));
        assert_eq!(super::cut_long(&body, true), (body, false));
    }

    /// The one drawing in both places (N-800 step 2): his post and the agent's answer drawn by `post` on Threads and in
    /// the agent's With you, each with its actions under the pointer (they were on Threads only).
    #[test]
    fn threads_and_with_you_draw_a_post_the_same_way_with_its_actions() {
        use crate::Sel;
        use serde_json::json;
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-7", "n": 7, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "paper-research"}, "title": "the look", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-19T08:00:00Z", "given": [{"to": "paper-research", "delivery": "N-7.p1", "upto": 1, "at": "2026-11-19T08:00:01Z"}],
            "posts": [{"n": 1, "at": "2026-11-19T08:00:00Z", "by": "owner", "text": "is it in?", "names": ["paper-research"], "told": ["paper-research"]},
                      {"n": 2, "at": "2026-11-19T08:01:00Z", "by": "paper-research", "text": "Yes, since 08:00.", "names": [], "told": ["owner"]}]})).unwrap();
        p.live.tickets.insert(7, v);
        s.projects = vec![p];
        s.cur = 0;
        let mut rt = crate::view_cost::window(s);
        for (place, sel) in [("Threads", Sel::Tickets), ("With you", Sel::Agent("paper-research".into()))] {
            for n in [1u32, 2] {
                rt.update(|s, _| {
                    s.sel = sel.clone();
                    s.tickets.shown = Some(7);
                    s.tickets.hover_post = Some((7, n));
                });
                rt.frame(0.016);
                rt.frame(0.016);
                assert!(rt.find(&format!("post-N-7-{n}")).is_some(), "{place}: post {n} drawn");
                assert!(rt.find(&format!("act-Reply-N-7-{n}")).is_some(), "{place}: post {n}'s Reply under the pointer");
                assert!(rt.find(&format!("more-N-7-{n}")).is_some(), "{place}: post {n}'s ⋯ under the pointer");
            }
        }
    }

    /// Post 299 of the owner's chat N-1104 (paper-research, 19 Nov 20:05) as it was posted: two paragraphs, then a
    /// table of five rows; its first paragraph's last sentence is an ask with three choices (the watcher's marks).
    const POST_299: &str = r#"**Yes, we have the picture:** consult-lead's page just above, five parts, built from your own words and today's record. Your answers to its three questions (three at a time; the second answerer; joining) are the only things still open in it.

**Staffed, as of now.** One owner per part, each part goes to your window alone:

| part | what you get | builder | state |
|---|---|---|---|
| 5 | nothing of yours is ever withdrawn or re-addressed | docket-core5 | written, tests on the tower now, tonight's engine build |
| 1 | every turn's end is recorded, its last message always lands on a thread | docket-core5 | two fixes written; timer and update turns next |
| 2 | `@all` reaches everyone and one claims; an answer wakes only who waits | docket-core4 | started this afternoon, continues now |
| 3 | "3 of 11 waiting, more when each is answered": the handful and the counter | docket-core2 | I start it now (it was stopped) |
| 4 | each post of yours shows where it is; a second answerer when yours waits; joining and splitting | docket-edge4 (window), docket-core6 (engine) | I start both now (they were stopped) |

consult-lead leads the design, consult-3 and docket-core3 read every piece before it reaches you.

**The walkthrough:** consult-lead brings it here next, made by consult-2: one message of yours followed step by step through the new runtime, with the literal screens and texts at each step, including the case under load. Not a second design page.

The builds for this go through the tower like everything else, behind tonight's fixes for you."#;

    /// Words with a table whose cells hold an agent (`@paper-research`), ids (one whose thread's label has a `|`) and
    /// paths.
    const CELLS: &str = "Where each is:\n\n| who | what | where |\n|---|---|---|\n| @paper-research | N-1104 | docs/window-rules.md |\n| docket-core5 | N-412.p1497 | yaaihh/crates/yaaihh/src/post.rs:70 |\n";

    /// A project with paper-research and the owner's chat N-1104: his post 298 and its answer 299 (POST_299, with its
    /// ask); N-412, whose label holds a `|`; and paper-research's session, whose one run of work is the reply call
    /// that posted 299 and its result. The agent's page is shown, at With you.
    fn table_app() -> (std::sync::MutexGuard<'static, ()>, crate::App) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "paper-research", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T19:00:00Z", "tools": [], "items": []})).unwrap()];
        let thread = |id: &str, n: u64, title: &str, posts: serde_json::Value| -> coord_types::TicketView {
            serde_json::from_value(json!({"id": id, "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
                "title": title, "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-19T08:00:00Z",
                "given": [{"to": "paper-research", "delivery": format!("{id}.p1"), "upto": 1, "at": "2026-11-19T08:00:01Z"}], "posts": posts}))
            .unwrap()
        };
        p.live.tickets.insert(1104, thread("N-1104", 1104, "the owner's chat", json!([
            {"n": 298, "at": "2026-11-19T19:04:00Z", "by": "owner", "text": "so do we have a picture of what is staffed?", "names": ["paper-research"], "told": ["paper-research"]},
            {"n": 299, "at": "2026-11-19T19:05:00Z", "by": "paper-research", "text": POST_299, "names": [], "told": ["owner"],
             "asks": [{"at": [120, 241], "options": [[157, 172], [174, 193], [195, 202]]}], "asks_open": true}])));
        p.live.tickets.insert(412, thread("N-412", 412, "window | engine pieces", json!([])));
        // one answer to two messages, posted under the first (the reply tool's `to`)
        let call = json!({"to": "N-1104.p298, N-412.p1636", "text": POST_299}).to_string();
        let entries = serde_json::from_value(json!([
            {"offset": 100, "at": "2026-11-19T19:04:58Z", "kind": "assistant", "text": "The picture, on the owner's chat."},
            {"offset": 200, "at": "2026-11-19T19:04:59Z", "kind": "tool", "tool": "mcp__coord__reply", "text": call},
            {"offset": 300, "at": "2026-11-19T19:05:00Z", "kind": "result", "text": "posted N-1104.p299"}]))
        .unwrap();
        p.convs.entry("paper-research".into()).or_default().merge(coord_types::Transcript { entries, start: 0, next: 400, session: "s".into(), ..Default::default() }, false);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agent("paper-research".into());
        (held, s)
    }

    /// The table drawn inside the element keyed KEY, its parts in the order drawn: the table, each row, each header
    /// cell and cell by its role, a cell with the words drawn in it. Inside is by descent, not by place: a table wider
    /// than its message runs past the message's edge in its sideways scroll.
    fn table_in(rt: &rune::runtime::Runtime<crate::App>, key: &str) -> Vec<(&'static str, String)> {
        let all = rt.elements();
        let under = |i: usize, top: usize| std::iter::successors(Some(i), |&j| all[j].parent).any(|j| j == top);
        let top = all.iter().position(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("{key} is drawn"));
        (0..all.len())
            .filter(|&i| matches!(all[i].role, "table" | "row" | "columnheader" | "cell") && under(i, top))
            .map(|i| {
                let words = match all[i].role {
                    "columnheader" | "cell" => (0..all.len()).filter(|&t| under(t, i)).filter_map(|t| all[t].text.clone()).collect::<Vec<_>>().join(" "),
                    _ => String::new(),
                };
                (all[i].role, words)
            })
            .collect()
    }

    fn count(t: &[(&'static str, String)], role: &str) -> usize {
        t.iter().filter(|(r, _)| *r == role).count()
    }

    /// The table VIEW draws in the element keyed `alone`, in a window of its own over S (a `fn` view takes no
    /// captures, so each test gives the words it draws in its own).
    fn alone(s: crate::App, view: fn(&crate::App, &mut rune::Cx<crate::App>) -> rune::El) -> Vec<(&'static str, String)> {
        let mut rt = rune::runtime::Runtime::new(s, view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        rt.frame(0.0);
        rt.frame(0.016);
        table_in(&rt, "alone")
    }

    /// The owner, N-1104 post 300: "the thread ui for agent does not render markdown tables"; post 299's own words,
    /// through the one function every view draws a message's words with, are a table: a header of four and five rows.
    #[test]
    fn post_299_is_a_table_of_five_rows() {
        fn view(s: &crate::App, cx: &mut rune::Cx<crate::App>) -> rune::El {
            let k = rune_kit::Kit::neutral(cx.system_dark());
            rune::col().key("alone").w(900.0).child(super::message_body(s.projects.first(), POST_299, None, None, &k))
        }
        let (_held, s) = table_app();
        let t = alone(s, view);
        assert!(count(&t, "table") >= 1, "a table: {t:?}");
        assert_eq!((count(&t, "row"), count(&t, "columnheader"), count(&t, "cell")), (6, 4, 20), "the header and five rows of four: {t:?}");
        let cells: Vec<&str> = t.iter().filter(|(r, _)| *r == "cell").map(|(_, w)| w.as_str()).collect();
        assert!(cells[2].contains("docket-core5") && cells[16].starts_with('4'), "the rows as written: {cells:?}");
    }

    /// An agent, an id and a path in a table's cells are links in their cells, and the table stays as written: three
    /// columns, two rows, an id whose label holds a `|` not splitting its cell.
    #[test]
    fn a_table_cell_with_a_path_an_agent_and_an_id_keeps_the_table() {
        fn view(s: &crate::App, cx: &mut rune::Cx<crate::App>) -> rune::El {
            let k = rune_kit::Kit::neutral(cx.system_dark());
            rune::col().key("alone").w(900.0).child(super::message_body(s.projects.first(), CELLS, None, None, &k))
        }
        let (_held, s) = table_app();
        let linked = crate::threads::mentioned(&s.projects[0], &crate::paths::linkify(CELLS));
        assert!(linked.contains("(yaaihh-ref:@paper-research)") && linked.contains("window \\| engine pieces"), "the agent a link, the label's `|` escaped: {linked}");
        let t = alone(s, view);
        assert_eq!((count(&t, "row"), count(&t, "columnheader"), count(&t, "cell")), (3, 3, 6), "three columns, two rows: {t:?}");
        let cells: Vec<&str> = t.iter().filter(|(r, _)| *r == "cell").map(|(_, w)| w.as_str()).collect();
        assert!(cells[0].contains("paper-research") && cells[2].contains("window-rules.md"), "{cells:?}");
        assert!(cells[4].contains("engine pieces") && cells[4].contains("N-412.p1497") && cells[5].contains("post.rs"), "the id's cell whole, the path in the next: {cells:?}");
    }

    /// The one drawing (N-1271): post 299 on the Threads page and in paper-research's With you is the same table,
    /// element for element.
    #[test]
    fn a_table_post_draws_the_same_on_threads_and_in_with_you() {
        let (_held, s) = table_app();
        let mut rt = crate::view_cost::window(s);
        let mut drawn = Vec::new();
        for sel in [crate::Sel::Tickets, crate::Sel::Agent("paper-research".into())] {
            rt.update(|s, _| {
                s.sel = sel.clone();
                s.tickets.shown = Some(1104);
                s.withyou.inside.insert((0, "paper-research".into()), 1104);
            });
            rt.frame(0.016);
            rt.frame(0.016);
            drawn.push(table_in(&rt, "post-N-1104-299"));
        }
        assert_eq!((count(&drawn[0], "row"), count(&drawn[0], "cell")), (6, 20), "Threads: post 299's table: {:?}", drawn[0]);
        assert_eq!(drawn[0], drawn[1], "With you draws it as Threads does");
    }

    /// The work behind a reply (the agent's Work view): the reply call that posted 299, opened, is the post it made,
    /// table and all, not the call's arguments in code.
    #[test]
    fn the_work_views_reply_call_is_drawn_as_its_post() {
        let (_held, mut s) = table_app();
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.expanded.extend(["w-e200-0".to_string(), "c-e200-0".to_string()]);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        rt.frame(0.016);
        let t = table_in(&rt, "e200-0");
        assert_eq!((count(&t, "row"), count(&t, "columnheader"), count(&t, "cell")), (6, 4, 20), "the call's post, its table: {t:?}");
        assert!(!rt.elements().iter().any(|e| e.text.as_deref().is_some_and(|w| w.contains("|---|"))), "not its arguments as written");
        // a reply to two messages is referred to by the first, where it was posted, not by the two ids as one
        let all = rt.elements();
        let texts: Vec<&str> = all.iter().filter_map(|e| e.text.as_deref()).collect();
        assert!(texts.contains(&"N-1104.p298"), "the reference's id: {texts:?}");
        assert!(!texts.contains(&"N-1104.p298, N-412.p1636"), "not the two ids as one: {texts:?}");
    }

    /// Window rules §6 (consult-lead, N-1271 post 8): a table wider than the message keeps its columns, one line a
    /// row, and scrolls sideways inside the message; the scrolling part keeps to the message's column.
    #[test]
    fn a_wide_table_keeps_one_line_a_row_and_scrolls_inside_its_column() {
        const WIDE: &str = "| x | one | two | three | four | five | six | seven |\n|---|---|---|---|---|---|---|---|\n\
            | a | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words |\n\
            | b | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words | a cell of several words |\n";
        fn view(s: &crate::App, cx: &mut rune::Cx<crate::App>) -> rune::El {
            let k = rune_kit::Kit::neutral(cx.system_dark());
            // the window sizes its root to itself, so the narrow pane is a child of it
            rune::col().child(rune::col().key("alone").w(360.0).child(super::message_body(s.projects.first(), WIDE, None, None, &k)))
        }
        let (_held, s) = table_app();
        let mut rt = rune::runtime::Runtime::new(s, view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        rt.frame(0.0);
        rt.frame(0.016);
        let all = rt.elements();
        let rects = |t: &str| -> Vec<[f32; 4]> { all.iter().filter(|e| e.text.as_deref() == Some(t)).map(|e| e.rect).collect() };
        let line = rects("a").first().expect("the short cell")[3];
        let long = rects("a cell of several words");
        assert_eq!(long.len(), 14, "eight columns, two rows");
        assert!(long.iter().all(|r| (r[3] - line).abs() < 0.5), "each cell on one line, as tall as `a`: {line} {long:?}");
        let th = all.iter().position(|e| e.key.as_deref() == Some("th")).expect("the header row");
        let frame = all[th].parent.expect("the table");
        let viewport = all[all[frame].parent.expect("its viewport")].rect;
        assert!(all[frame].rect[2] > 360.0, "the table is wider than the column: {:?}", all[frame].rect);
        assert!(viewport[2] <= 360.5, "what scrolls keeps to the column: {viewport:?}");
    }

    /// consult-lead, N-1271 post 8: a comment stored by its place on an old mail, before a path in the mail became a
    /// link, is marked on its passage, found by its words; never on the words now standing at the old place. The comment
    /// is his mail's quote (a comment is sent at its press, N-1396 p329).
    #[test]
    fn a_comment_on_an_old_mail_after_a_path_is_marked_on_its_passage() {
        let (_held, mut s) = table_app();
        let body = "See docs/window-rules.md first.\n\nThe circles go under the tiers.\n";
        let passage = "The circles go under the tiers.";
        let at = body.find(passage).unwrap();
        let comment = crate::ask::Comment { source: "m-7".into(), label: "a mail".into(), quote: passage.into(), note: "yes".into(), at: vec![at..at + passage.len()] };
        let (said, _) = crate::ask::compose(&[comment], "");
        let quotes = serde_json::json!([{"source": "m-7", "text": passage, "at": [[at, at + passage.len()]]}]);
        let m: coord_types::MailView = serde_json::from_value(serde_json::json!({"id": "m-8", "at": "2026-11-20T09:00:00Z", "from": "owner", "to": ["lead"], "subject": "", "body": said, "esc": false, "delivered": [], "re": "m-7", "quotes": quotes})).unwrap();
        s.projects[0].live.mail.put(m);
        let p = &s.projects[0];
        let src = crate::threads::mentioned(p, &crate::paths::linkify(body));
        assert_ne!(src.get(at..at + passage.len()), Some(passage), "the path is a link, the stored place holds other words: {src}");
        let marks = crate::ask::marks(&s, p, "m-7", &src, &rune_kit::Kit::neutral(false));
        let marked: Vec<String> = marks.iter().map(|(at, _)| at.iter().map(|r| &src[r.clone()]).collect()).collect();
        assert_eq!(marked, vec![passage.to_string()], "one mark, on the passage");
    }

    /// N-1271's stills, from a stand-in window (this made-up project, no coordd): `STILL_OUT=DIR cargo test -p yaaihh
    /// --lib table_still -- --ignored --exact post::tests::table_still_with_you` (and `..._work`), one a process:
    /// paper-research's With you with post 299, and its Work view with the reply call that posted it opened.
    fn still(name: &str, init: fn() -> crate::App) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    #[test]
    #[ignore]
    fn table_still_with_you() {
        fn init() -> crate::App {
            let (_held, mut s) = table_app();
            s.withyou.inside.insert((0, "paper-research".into()), 1104);
            s
        }
        still("with-you.png", init);
    }

    #[test]
    #[ignore]
    fn table_still_work() {
        fn init() -> crate::App {
            let (_held, mut s) = table_app();
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
            s.expanded.extend(["w-e200-0".to_string(), "c-e200-0".to_string()]);
            s
        }
        still("work.png", init);
    }

    /// The rect of the first element drawn under the one keyed UNDER whose text holds WORDS.
    fn at(rt: &rune::runtime::Runtime<crate::App>, under: &str, words: &str) -> [f32; 4] {
        let all = rt.elements();
        all.iter()
            .find(|e| !e.exiting && e.path.split('/').any(|k| k == under) && e.text.as_deref().is_some_and(|t| t.contains(words)))
            .map(|e| e.rect)
            .unwrap_or_else(|| panic!("{words:?} is drawn under {under}"))
    }

    /// The rect of the element keyed KEY.
    fn keyed(rt: &rune::runtime::Runtime<crate::App>, key: &str) -> [f32; 4] {
        let all = rt.elements();
        all.iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect).unwrap_or_else(|| panic!("{key} is drawn"))
    }

    /// 0.2's C14, the owner's list: "a thread looks like a chat: him right, the agent left" (window rules §6). On the
    /// Threads page his post 298 stands in his bubble at the column's right edge and paper-research's answer 299 at
    /// its left, plain; in paper-research's Work view his prompt and his message stand on the right, another agent's
    /// mail and the agent's own words on the left.
    #[test]
    fn him_on_the_right_the_agent_on_the_left() {
        let (_held, mut s) = table_app();
        let owner = "<channel source=\"coord\" from=\"owner\" ticket=\"N-1104\" id=\"N-1104.p300\" kind=\"owner\">\nand the tables too\n</channel>";
        let core = "<channel source=\"coord\" from=\"docket-core\" ticket=\"N-412\" id=\"N-412.p9\" kind=\"post\">\nthe build is cut\n</channel>";
        let entries = serde_json::from_value(serde_json::json!([
            {"offset": 10, "at": "2026-11-19T19:00:00Z", "kind": "user", "text": "keep the tables whole"},
            {"offset": 20, "at": "2026-11-19T19:00:10Z", "kind": "mail", "text": owner},
            {"offset": 30, "at": "2026-11-19T19:00:20Z", "kind": "mail", "text": core},
            {"offset": 100, "at": "2026-11-19T19:04:58Z", "kind": "assistant", "text": "The picture, on the owner's chat."}]))
        .unwrap();
        let p = &mut s.projects[0];
        p.convs.remove("paper-research");
        p.convs.entry("paper-research".into()).or_default().merge(coord_types::Transcript { entries, start: 0, next: 400, session: "s".into(), ..Default::default() }, false);
        let mut rt = crate::view_cost::window(s);
        rt.update(|s, _| {
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(1104);
        });
        rt.frame(0.016);
        rt.frame(0.016);
        let (his, ag) = (keyed(&rt, "said-N-1104-298"), keyed(&rt, "said-N-1104-299"));
        assert!((his[0] + his[2] - (ag[0] + ag[2])).abs() < 1.0, "his bubble ends at the column's right edge, where the agent's words end: {his:?} {ag:?}");
        assert!(his[0] > ag[0] + 100.0, "his short post on the right, the agent's from the left: {his:?} {ag:?}");
        let words = at(&rt, "said-N-1104-298", "so do we have a picture");
        assert!(words[0] >= his[0] && words[0] + words[2] <= his[0] + his[2] + 0.5, "his words inside his bubble: {words:?} {his:?}");
        rt.update(|s, _| {
            s.sel = crate::Sel::Agent("paper-research".into());
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        });
        rt.frame(0.016);
        rt.frame(0.016);
        let agent = at(&rt, "e100-0", "The picture, on the owner");
        for (key, w) in [("e10-0", "keep the tables whole"), ("e20-0", "and the tables too")] {
            let r = at(&rt, key, w);
            assert!(r[0] > agent[0] + 300.0, "{w:?}, his, on the right: {r:?}, the agent's words at {agent:?}");
        }
        let mail = at(&rt, "e30-0", "the build is cut");
        assert!(mail[0] < agent[0] + 60.0, "docket-core's mail on the left: {mail:?}, the agent's words at {agent:?}");
    }

    /// docket-edge8, N-1418: each comment of his begins with its quote. His bubble then takes the reading width, and the
    /// quote and his words stand inside it, not one or two words wide and running out below it; a short plain post
    /// stays as wide as its words.
    #[test]
    fn his_comment_with_a_quote_stands_inside_a_bubble_of_reading_width() {
        let (_held, mut s) = table_app();
        let post: docket_machine::Post = serde_json::from_value(serde_json::json!({"n": 300, "at": "2026-11-19T19:06:00Z", "by": "owner",
            "text": "> the lock is held 7-12 s while the follower reads the record\n> (from paper-research's message at 10:11)\n\nby whom?",
            "names": ["paper-research"], "told": ["paper-research"]}))
        .unwrap();
        s.projects[0].live.tickets.get_mut(&1104).unwrap().posts.push(post);
        let mut rt = crate::view_cost::window(s);
        rt.update(|s, _| {
            s.sel = crate::Sel::Tickets;
            s.tickets.shown = Some(1104);
        });
        rt.frame(0.016);
        rt.frame(0.016);
        let his = keyed(&rt, "said-N-1104-300");
        assert!(his[2] > 400.0, "his bubble at the reading width: {his:?}");
        for w in ["the lock is held", "by whom?"] {
            let r = at(&rt, "said-N-1104-300", w);
            assert!(r[0] >= his[0] && r[0] + r[2] <= his[0] + his[2] + 0.5 && r[1] + r[3] <= his[1] + his[3] + 0.5, "{w:?} inside his bubble: {r:?} {his:?}");
        }
        let plain = keyed(&rt, "said-N-1104-298");
        assert!(plain[2] + 100.0 < his[2], "a short plain post as wide as its words: {plain:?} {his:?}");
        assert!(super::holds_block("- one\n- two") && super::holds_block("1. first") && super::holds_block("| a | b |") && super::holds_block("```\nx\n```"));
        assert!(!super::holds_block("so do we have a picture? 3. no") && !super::holds_block("2026. the year"));
    }
}
