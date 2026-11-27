//! How open work hangs together (the owner, N-186.p27–p28; paper-research's layouts N-186 posts 29 and 30): a thread
//! that came out of another is a subthread, a full thread of its own; its parent shows one line for it where it split
//! off (its label and id, who has it, one state word), and the subthread says what it is part of at its top. With an
//! agent, the open pieces it holds stand under their goals, what waits on the owner filled and counted; on Threads the
//! same outline for the whole project.
//!
//! What a thread's line says comes from coordd's `outline` on its view (docket-core5, N-397 post 3): its label, its
//! parent (the latest `coord tie`, else the ticket it was opened under) and the post it stands at there, who has it, its
//! state word, and its children in the order of those posts. A coordd from before the field gives none; then the app
//! reads the same from the view as it did (opened under, `form.to`, the waits).

use crate::texts::plain;
use crate::{App, Proj};
use coord_types::TicketView;
use docket_machine::ViewStatus;
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

/// The thread V was split off from, if it is one.
pub fn parent<'a>(p: &'a Proj, v: &TicketView) -> Option<&'a TicketView> {
    if let Some(o) = &v.outline {
        let id = o.parent.as_deref()?;
        return by_id(p, id);
    }
    let id = v.parent.as_deref().or(v.under.as_deref())?;
    by_id(p, id).filter(|t| t.kind == "thread")
}

/// Ticket ID of project P: by the number its id ends in, which is its key in `tickets` (coordd numbers every kind from
/// one count: N-1446, I-1457, H-1705), else, an id that does not end in its key, looked for among them all. Found by
/// a walk of them all, `parent` cost each ticket a walk of every ticket and `goal` one a level, so `pieces` grew with
/// the square of the project's tickets, and ran on every key typed in With you (N-1793).
pub fn by_id<'a>(p: &'a Proj, id: &str) -> Option<&'a TicketView> {
    let n = id.rsplit_once('-').and_then(|(_, n)| n.parse::<u64>().ok());
    match n.and_then(|n| p.live.tickets.get(&n)) {
        Some(t) if t.id == id => Some(t),
        _ => p.live.tickets.values().find(|t| t.id == id),
    }
}

/// The threads split off from V, in the order they were opened: its children drawn as lines (`drawn`), not its marks.
pub fn subthreads<'a>(p: &'a Proj, v: &TicketView) -> Vec<&'a TicketView> {
    if let Some(o) = &v.outline {
        return o.children.iter().filter_map(|c| by_id(p, &c.id)).filter(|t| is_line(t)).collect();
    }
    let mut out: Vec<&TicketView> = p.live.tickets.values().filter(|t| t.kind == "thread" && t.id != v.id && parent(p, t).is_some_and(|q| q.id == v.id)).collect();
    out.sort_by_key(|t| t.opened_at.to_string());
    out
}

/// The goal V's work is for: the top of its parents (V itself when it has none).
pub fn goal<'a>(p: &'a Proj, v: &'a TicketView) -> &'a TicketView {
    let mut g = v;
    let mut seen = 0;
    while let Some(q) = parent(p, g) {
        g = q;
        seen += 1;
        if seen > 32 {
            break;
        }
    }
    g
}

pub fn open(v: &TicketView) -> bool {
    if let Some(o) = &v.outline {
        return o.open;
    }
    v.closed_at.is_none() && !matches!(v.status, ViewStatus::Done { .. } | ViewStatus::Failed { .. })
}

/// Its label: coordd's, else its first line, cut.
pub fn label(p: &Proj, v: &TicketView) -> String {
    // coordd's: the given label or title, else its words' first line that is not a quote (da660b7)
    if let Some(o) = v.outline.as_ref().filter(|o| !o.label.is_empty()) {
        return o.label.clone();
    }
    let l = p.labels.get(&v.id).map(str::to_string).filter(|l| !l.is_empty()).unwrap_or_else(|| crate::refs::cut(v.title.lines().next().unwrap_or("").trim(), 60));
    if l.is_empty() { v.id.clone() } else { l }
}

/// Who has V: the agent it was written to, else the first agent on it.
pub fn holder(v: &TicketView) -> Option<String> {
    if let Some(o) = &v.outline {
        return o.who.clone().filter(|w| w != "owner");
    }
    v.form.get("to").and_then(|t| t.as_str()).filter(|t| !t.is_empty() && *t != "owner").map(str::to_string).or_else(|| agents_on(v).into_iter().next())
}

/// V's one state word, and whether it waits on the owner.
pub fn state(v: &TicketView) -> (String, bool) {
    // whether it owes him: the one answer (waits.rs)
    if crate::waits::owes(v) {
        return ("waits on you".into(), true);
    }
    if let Some(o) = &v.outline {
        return (o.state.clone(), false);
    }
    // a thread that ended says so, whatever its last waits still name
    match &v.status {
        _ if v.closed_at.is_some() => return ("done".into(), false),
        ViewStatus::Done { .. } => return ("done".into(), false),
        ViewStatus::Failed { .. } => return ("failed".into(), false),
        _ => {}
    }
    let waits = crate::threads::waits_for(v);
    if !waits.is_empty() {
        return (format!("waits on {}", waits.join(", ")), false);
    }
    let word = match &v.status {
        _ if v.closed_at.is_some() => "done",
        ViewStatus::Done { .. } => "done",
        ViewStatus::Failed { .. } => "failed",
        ViewStatus::Resting => "quiet",
        _ => "at work",
    };
    (word.into(), false)
}

/// V's line in thread P: V's own outline when V is the ticket shown, else P's line for it (a mark's: it is never
/// shown in a list). Where it stands (`place`) and what a mark says (`mark_line`) are read from it.
fn node_in<'a>(p: &'a TicketView, v: &'a TicketView) -> Option<&'a docket_machine::ViewOutline> {
    v.outline.as_ref().filter(|o| o.parent.as_deref() == Some(p.id.as_str()) && o.at_post.is_some()).or_else(|| p.outline.as_ref().and_then(|o| o.children.iter().find(|c| c.id == v.id)))
}

/// Where V's line stands in its parent's page, as a time: just after the post it is tied at, else when it was opened.
pub fn place(p: &TicketView, v: &TicketView) -> String {
    let at = node_in(p, v).and_then(|o| o.at_post).and_then(|n| p.posts.iter().find(|q| q.n == n));
    match at {
        Some(q) => format!("{}~", q.at),
        None => v.opened_at.to_string(),
    }
}

fn open_thread(s: &mut App, v_n: u64, id: &str) {
    open_from_thread(s, s.cur, v_n, id);
}

/// Where a click on a line of the list lands (window-rules §8: a list of threads is one thing wherever it stands): on
/// Threads (WHO None) the thread opens on the page, at POST when given; beside an agent's chat (WHO its name) it opens
/// beside the list, and is seen.
fn press(who: Option<&str>, pi: usize, n: u64, at: String, post: Option<u32>) -> impl Fn(&mut App) + 'static {
    let who = who.map(str::to_string);
    move |s: &mut App| match &who {
        None => crate::find::open_at(s, n, &at, post),
        Some(a) => open_beside(s, pi, a, n),
    }
}

/// Thread N (ID) pressed from inside a thread (threads.rs: "continues N-x ›", "Taken as · N-x ›", "opened N-x ›"):
/// it opens where it was pressed, as a line of the list does ([`press`]): beside the agent's list on its With you when
/// that list holds it, else on Threads (his N-977 p37: a link took him off his agent's page onto Threads; item 15,
/// paper-research N-1446 p2603).
pub fn open_from_thread(s: &mut App, pi: usize, n: u64, id: &str) {
    let who = s.sel.with_you().filter(|a| s.projects.get(pi).is_some_and(|p| in_list(p, a, n, false))).map(str::to_string);
    press(who.as_deref(), pi, n, id.to_string(), None)(s)
}

/// Thread N opened beside agent A's list in With you, and seen: a press on its line, or on a search's hit there
/// (find.rs `agent_results`: a hit opens in the page he is in, the owner N-1104 post 512).
pub fn open_beside(s: &mut App, pi: usize, a: &str, n: u64) {
    s.withyou.inside.insert((pi, a.to_string()), n);
    if let Some((id, upto)) = s.projects.get(pi).and_then(|p| p.live.tickets.get(&n)).map(|v| (v.id.clone(), v.posts.iter().map(|x| x.n).max().unwrap_or(0))) {
        s.tickets.seen.upto.insert((pi, id), upto);
    }
}

/// A subthread's line in its parent, where it split off: "↳ label", the id dim; a state word only when it needs him
/// (the accent) or it ended ("ended: done · why", dim) (N-466).
pub fn subthread_line(p: &Proj, t: &TicketView, k: &Kit) -> El {
    let (n, id) = (t.n, t.id.clone());
    subthread_line_to(p, t, k, move |s: &mut App| open_thread(s, n, &id))
}

/// The same line, a click doing PRESS (With you's list shows the subthread beside it, withyou.rs).
pub fn subthread_line_to(p: &Proj, t: &TicketView, k: &Kit, press: impl Fn(&mut App) + 'static) -> El {
    // an ended subthread keeps its line, marked ended, with its why (docket-core4, N-412 post 157)
    let right = match crate::ending::ended(t) {
        Some((how, why)) => Some((crate::refs::cut(&format!("ended: {how} · {why}"), 60), k.c.text_muted)),
        None if state(t).1 => Some(("needs you".to_string(), k.c.text_accent)),
        None => None,
    };
    let name = row().gap(6.0).center_y().grow(1.0).shrink().w(0.0).child(text("↳").font(k.ty.small).color(k.c.text_faint)).child(crate::refs::el(&crate::refs::Ref { label: label(p, t), id: t.id.clone() }, k.ty.small_medium, k.c.text_base, k));
    let mut r = row().key(format!("sub-{}", t.id)).w_full().gap(12.0).center_y().py(2.0).child(name);
    if let Some((w, c)) = right {
        r = r.child(text(w).font(k.ty.small).color(c));
    }
    // the words of the post it was opened on, when it was opened on a passage (docket-core4's `quote`, N-447 p117)
    let quote = t.outline.as_ref().and_then(|o| o.quote.as_deref()).map(|q| q.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|q| !q.is_empty());
    let c = col().w_full().child(r).children(quote.map(|q| text(format!("“{}”", crate::refs::cut(&q, 80))).font(k.ty.small).color(k.c.text_faint).pl(18.0)));
    k.interactive(c).on_press(press).on_secondary_press(acts_at(t))
}

/// A secondary press on a thread's row: the acts there of the agent who has it (acts.rs), in the project shown; a
/// thread no agent has opens none.
fn acts_at(v: &TicketView) -> impl Fn(&mut App, rune::PointerPress, &mut rune::Tx) + 'static {
    acts_for(v.id.clone(), holder(v))
}

/// The same, for thread ID that WHO has.
fn acts_for(id: String, who: Option<String>) -> impl Fn(&mut App, rune::PointerPress, &mut rune::Tx) + 'static {
    move |s: &mut App, at, _| {
        if let Some(who) = &who {
            let c = s.cur;
            crate::acts::open(s, c, vec![(who.clone(), Some(id.clone()))], at);
        }
    }
}

/// "label ›", one dim line above a subthread's title: what it is part of; a click goes there.
pub fn part_of(p: &Proj, v: &TicketView, k: &Kit) -> Option<El> {
    let q = parent(p, v)?;
    let (n, id) = (q.n, q.id.clone());
    Some(k.interactive(row().key("part-of").gap(6.0).center_y().child(text(format!("{} ›", crate::refs::cut(&label(p, q), 60))).font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| open_thread(s, n, &id)))
}

/// A line of the outline (coordd's `outlined`), not archived.
pub(crate) fn outlined(v: &TicketView) -> bool {
    !v.archived && v.archived_under.is_none() && is_line(v)
}

/// Drawn as a line, as its kind says (`drawn line`; proto-lead N-1446 p17); from a coordd before the clause, which
/// says nothing, a thread or an item, as before.
pub fn is_line(v: &TicketView) -> bool {
    match v.drawn {
        Some(d) => d == docket_machine::syntax::Drawn::Line,
        None => v.kind == "thread" || v.kind == "item",
    }
}

/// V's marks: the tickets standing at its posts whose kind says `drawn mark` (a holder's, a cut) or `drawn note` (a
/// brain's pass), in the order of the posts they stand at; each opens to its ticket. A mark that came to nothing stands
/// nowhere in his thread, neither as its line nor in the margin (`came_to_nothing`); its ticket and its run stay in the
/// Docket and in Events. Nor does a note that has said nothing yet (`not_yet`): a pass at work stands once it has
/// spoken.
pub fn marks<'a>(p: &'a Proj, v: &TicketView) -> Vec<&'a TicketView> {
    standing(p, v, |c| !came_to_nothing(c)).into_iter().filter(|(c, t)| !not_yet(c, t)).map(|(_, t)| t).collect()
}

/// The tickets standing at V's posts whose kind says `drawn mark` or `drawn note`, each with its line C (its `at_post`
/// the post it stands at), of the lines that KEEP takes, in the order of V's line's children (`children`). KEEP is
/// asked before the ticket is looked for. [`marks`] takes those that stand in his thread; the Map takes every pass at a post, at
/// work or come to nothing as well, and draws it inside that post (map/deep.rs `passes`).
pub fn standing<'a, 'v>(p: &'a Proj, v: &'v TicketView, keep: impl Fn(&docket_machine::ViewOutline) -> bool) -> Vec<(&'v docket_machine::ViewOutline, &'a TicketView)> {
    let Some(o) = &v.outline else { return Vec::new() };
    o.children
        .iter()
        .filter(|c| keep(c))
        .filter_map(|c| Some((c, p.live.tickets.values().find(|t| t.id == c.id)?)))
        .filter(|(_, t)| t.drawn.is_some_and(docket_machine::syntax::Drawn::is_mark))
        .collect()
}

/// Whether note T (C its line) has said nothing yet: its kind says `drawn note`, it is open, and it has no post of its
/// own, as a brain's pass at work is (consult-3 N-1446 p890: drawn from its open, a line came under nearly every post of
/// his for the length of two calls, and went at its Quiet end). The kind says it, not who opened it (proto-reader,
/// proto-window, consult-3 p901): a mark (a cut, an aside) at work with no post stands as before.
fn not_yet(c: &docket_machine::ViewOutline, t: &TicketView) -> bool {
    t.drawn == Some(docket_machine::syntax::Drawn::Note) && c.open && t.posts.is_empty()
}

/// Whether the line C of a mark says it came to nothing for him: dropped (he pressed Not this, docket-edge2 N-1446
/// p117, proto-lead p131), failed (raised, or a call of a brain's pass failed), or said nothing (a brain's Quiet pass;
/// proto-mind p270: only a pass that spoke stands). coordd's word for it, `ViewOutline::of`'s state. The margin's
/// own test of it before (margin.rs `of`), moved here.
pub fn came_to_nothing(c: &docket_machine::ViewOutline) -> bool {
    matches!(c.state.as_str(), "dropped" | "failed" | "said nothing")
}

/// What mark T says on V's line for it: its own words first, its last post's first line (the brain's thought,
/// proto-reader N-1446 p197, consult-3 p205); else what it came to, in words as coordd says it (`view::said_in_words`,
/// V's line for it, once it is closed: Quiet, Said); else its label.
pub fn mark_says(s: &App, p: &Proj, v: &TicketView, t: &TicketView) -> String {
    // a last post still being read says nothing yet: the mark falls back as for one with no words
    let last = t.posts.last().and_then(|q| match crate::threads::state(&s.tickets, q) {
        crate::texts::TextState::Ready(w) => Some(crate::texts::first_line_cut(&w)),
        _ => None,
    });
    let said = || node_in(v, t).and_then(|o| o.said.clone());
    last.or_else(said).unwrap_or_else(|| label(p, t))
}

/// Mark T's one line where it stands on its thread: its kind's label ([`mark_kind`]), what it says (`mark_says`), and
/// a press that opens it.
pub fn mark_line(s: &App, p: &Proj, v: &TicketView, t: &TicketView, k: &Kit) -> El {
    let (to, id) = (t.n, t.id.clone());
    let kind = mark_kind(crate::picture::labels(s, p).kinds.get(&t.kind).map_or(&t.kind, |l| l));
    let says = mark_says(s, p, v, t);
    // what it says and its id, as every ticket reference is drawn (refs::el)
    let what = crate::refs::el(&crate::refs::Ref { label: crate::refs::cut(&says, 100), id: t.id.clone() }, k.ty.small, k.c.text_base, k);
    let r = row().key(format!("mark-{}", t.id)).w_full().gap(6.0).center_y().py(2.0)
        .child(text("◆").font(k.ty.small).color(k.c.text_accent))
        // the kind's words never take the line from what it says
        .child(text(kind).font(k.ty.small).color(k.c.text_muted).truncate().max_w(240.0))
        .child(text("·").font(k.ty.small).color(k.c.text_faint))
        // what it says is cut to the room left, so its id and its ⓘ stay clear of it
        .child(row().grow(1.0).shrink().w(0.0).child(what.shrink().min_w(0.0)));
    // a rule made it: what is behind it, a press away
    let r = if t.binding.is_some() { r.child(behind_press(p, v, t, format!("behind-press-{}", t.id), k)) } else { r };
    // it opens the way every line of a thread opens (find::open_at), wherever the thread is drawn (consult-3 N-1446 p106)
    k.interactive(r).on_press(move |s: &mut App| open_thread(s, to, &id)).on_secondary_press(acts_at(t))
}

/// ⓘ: a circle with an i, as small as the line it ends.
static INFO: rune::Icon = rune::Icon { name: "outline-behind", view: 16.0, body: r#"<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M8 7.25v4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="8" cy="4.9" r="0.95" fill="currentColor"/>"# };

/// The small ⓘ at the end of a line a rule or an agent's brain made, T's on thread V (the owner, N-1790 part 3), keyed
/// KEY: what is behind it, apart ([`behind`]).
pub(crate) fn behind_press(p: &Proj, v: &TicketView, t: &TicketView, key: String, k: &Kit) -> El {
    let (n, on) = (t.n, v.n);
    let title = crate::popup::title(&crate::refs::ticket(Some(p), t).label, &t.id, "behind it");
    IconButton::new(&INFO)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .label("Behind it")
        .help("Its rule, result and wire")
        .on_press(move |s: &mut App| crate::popup::open(s, title.clone(), crate::popup::What::Behind { n, on }))
        .build(k)
        .key(key)
}

/// What is behind ticket N of the current project, a line a rule or an agent's brain made on thread ON (the owner,
/// N-1790 part 3; popup.rs `What::Behind`): its verdict (what it came to, what it says, the rule that made it, each
/// decision held for him and the rule behind it); what went in (what it was opened with) and what came out (its posts,
/// what it opened, its steps); and its calls on the wire (lens.rs at Wire) and its Docket picture (picture/docket.rs),
/// each a press that leaves the popup for them. None when it is no longer held.
pub(crate) fn behind(s: &App, n: u64, on: u64, k: &Kit) -> Option<El> {
    let p = s.projects.get(s.cur)?;
    let t = crate::find::ticket(s, p, n)?;
    let on = crate::find::ticket(s, p, on).unwrap_or(t);
    let pg = &s.tickets;
    let words = |w: String| text(w).font(k.ty.small).color(k.c.text_base).wrap();
    let faint = |w: String| text(w).font(k.ty.small).color(k.c.text_faint).wrap();
    let line = |label: &str, body: El| row().w_full().gap(12.0).items_start().child(text(label.to_string()).font(k.ty.small).color(k.c.text_muted).w(96.0)).child(col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(body));
    let head = |w: &str| text(w.to_string()).font(k.ty.small_medium).color(k.c.text_muted);
    let mut c = col().key(format!("behind-body-{}", t.id)).w_full().gap(8.0);

    // the verdict: what it came to, what it says, by which rule
    let came = match (&t.ended, crate::ending::ended(t)) {
        (Some(e), _) => format!("{} · {}", e.how, e.why),
        (None, Some((how, why))) => format!("{how} · {why}"),
        // a mark's state is its line's in the thread it stands on
        (None, None) => node_in(on, t).map_or_else(|| state(t).0, |o| o.state.clone()),
    };
    c = c.child(head("Verdict")).child(line("came to", words(came))).child(line("says", words(mark_says(s, p, on, t))));
    c = c.child(line("rule", match &t.binding {
        Some(b) => text(b.clone()).font(k.ty.mono).color(k.c.text_base).wrap(),
        None => faint(format!("no rule: opened by {}", t.opener.as_deref().unwrap_or("its thread"))),
    }));
    for h in &t.halts {
        if let docket_machine::ViewHalt::Decide { question, answer, provenance, .. } = h {
            c = c.child(line("decided", words(format!("{question}: {} · {}", crate::texts::words(pg, answer), crate::slow::provenance(provenance)))));
        }
    }

    // in and out: what it was opened with, and what came of it
    let given: Vec<String> = t.form.as_object().into_iter().flatten().filter(|(_, x)| !x.is_null() && x.as_str() != Some("")).map(|(f, x)| format!("{}: {}", f.replace('_', " "), crate::texts::words(pg, x))).filter(|w| !w.trim().ends_with(':')).collect();
    let came_in = if given.is_empty() { faint("nothing beyond what its rule names".into()) } else { col().w_full().gap(2.0).children(given.into_iter().map(|w| words(w))) };
    let posts = match t.posts.len() {
        0 => "no post yet".to_string(),
        1 => "1 post".to_string(),
        n => format!("{n} posts"),
    };
    let mut out = col().w_full().gap(2.0).child(words(posts));
    if !t.children.is_empty() {
        out = out.child(words(format!("opened {}", t.children.join(", "))));
    }
    out = out.child(words(format!("{} of {} steps", t.spent, t.bound)));
    c = c.child(head("In → out")).child(line("in", came_in)).child(line("out", out));

    // its wire and its Docket, the popup left for them
    let (wire, docket) = (t.id.clone(), t.n);
    let raw = Button::new("Raw Wire ›")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .help("Its calls on the Wire")
        .on_press(move |s: &mut App| {
            crate::popup::close(s);
            crate::lens::zoom(s, crate::lens::Depth::Wire, &wire);
        })
        .build(k)
        .key(format!("behind-wire-{}", t.id));
    let pic = Button::new("Docket picture ›")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .help("Its program in the Docket")
        .on_press(move |s: &mut App| {
            crate::popup::close(s);
            crate::picture::docket::open_thread(s, docket);
        })
        .build(k)
        .key(format!("behind-docket-{}", t.id));
    Some(c.child(row().gap(8.0).child(raw).child(pic)))
}

/// A mark's kind as its line names it: LABEL, the first line of the kind's doc (else its name), up to its colon, where
/// a doc's line says what the kind is before how it works: brain.kind's "one pass of an agent's brain: its parts read,
/// …" is "one pass of an agent's brain", so its thought keeps the line.
pub(crate) fn mark_kind(label: &str) -> String {
    label.split(':').next().unwrap_or(label).trim().to_string()
}

/// How many lines of goal G's whole tree (G and every open piece under it) wait on the owner.
fn owed_in(g: &TicketView, ps: &[&TicketView]) -> usize {
    usize::from(state(g).1) + ps.iter().filter(|t| state(t).1).count()
}

/// The outline's topics with their open pieces (N-412, the owner N-446.p1): every open top thread, the project's (WHO
/// None) or the ones WHO keeps or holds a piece under, each with its open pieces in order. A thread that ended is not
/// in it. The topics that wait on the owner come first, most first; then the newest.
pub(crate) fn pieces<'a>(p: &'a Proj, who: Option<&str>) -> Vec<(&'a TicketView, Vec<&'a TicketView>)> {
    pieces_now(p, who)
}

thread_local! {
    /// [`pieces`] of each project and agent, as ticket numbers, with when its tickets last changed (waits.rs `tickets_at`).
    static DRAWN: std::cell::RefCell<std::collections::HashMap<(String, Option<String>), (crate::waits::TicketsAt, std::rc::Rc<Vec<(u64, Vec<u64>)>>)>> = Default::default();
}

/// [`pieces`] for what a view builds: read again when the project's tickets change, not on each view (N-1793, as
/// waits.rs `on_owner_drawn`). A handler asks [`pieces`].
pub(crate) fn pieces_drawn<'a>(p: &'a Proj, who: Option<&str>) -> Vec<(&'a TicketView, Vec<&'a TicketView>)> {
    let at = crate::waits::tickets_at(p);
    let key = (p.id.clone(), who.map(str::to_string));
    let back = |kept: &[(u64, Vec<u64>)]| -> Option<Vec<(&'a TicketView, Vec<&'a TicketView>)>> {
        kept.iter().map(|(g, ps)| Some((p.live.tickets.get(g)?, ps.iter().map(|n| p.live.tickets.get(n)).collect::<Option<Vec<_>>>()?))).collect()
    };
    let held = DRAWN.with_borrow(|m| m.get(&key).filter(|(when, _)| *when == at).map(|(_, kept)| kept.clone()));
    if let Some(found) = held.and_then(|kept| back(&kept)) {
        return found;
    }
    let now = pieces_now(p, who);
    let kept: Vec<(u64, Vec<u64>)> = now.iter().map(|(g, ps)| (g.n, ps.iter().map(|t| t.n).collect())).collect();
    if back(&kept).is_some_and(|b| b.len() == now.len()) {
        DRAWN.with_borrow_mut(|m| m.insert(key, (at, std::rc::Rc::new(kept))));
    }
    now
}

/// [`waits_in`] for what a view builds ([`pieces_drawn`]).
pub(crate) fn waits_in_drawn<'a>(p: &'a Proj, who: Option<&str>, goals: &[(&'a TicketView, Vec<&'a TicketView>)]) -> Vec<crate::waits::Wait<'a>> {
    crate::waits::on_owner_drawn(p).into_iter().filter(|w| who.is_none() || goals.iter().any(|(g, _)| g.id == w.goal.id)).collect()
}

fn pieces_now<'a>(p: &'a Proj, who: Option<&str>) -> Vec<(&'a TicketView, Vec<&'a TicketView>)> {
    let mine = |v: &TicketView| who.is_none_or(|w| is_with(v, w));
    let mut all: Vec<&TicketView> = p.live.tickets.values().filter(|t| outlined(t) && open(t)).collect();
    all.sort_by(|a, b| a.opened_at.cmp(&b.opened_at));
    let mut goals: Vec<(&TicketView, Vec<&TicketView>)> = Vec::new();
    // every open top thread is a topic: the project's all, WHO's those it keeps
    for t in &all {
        if parent(p, t).is_none() && mine(t) {
            goals.push((t, Vec::new()));
        }
    }
    // every open piece, under its topic: with WHO, a piece it keeps brings its topic in
    for t in all {
        let g = goal(p, t);
        if g.id == t.id || !open(g) || !(mine(t) || mine(g)) {
            continue;
        }
        match goals.iter_mut().find(|x| x.0.id == g.id) {
            Some(x) => x.1.push(t),
            None => goals.push((g, vec![t])),
        }
    }
    goals.sort_by(|(g, ps), (h, qs)| owed_in(h, qs).cmp(&owed_in(g, ps)).then_with(|| h.opened_at.cmp(&g.opened_at)));
    goals
}

/// How many topics [`outline`] lists under All active in P's Threads: its open ones ([`pieces`]) his Archive does not
/// hold (archive.rs `hides`), his own, not those between agents alone ([`apart`]). The left bar's Threads says it
/// (sidebar.rs; docket-edge7's walk N-1446 p3050, 7: it said "Threads 0" with the project's one thread open in it,
/// counting only what waits on him; paper-research p3051; yaaihh/fresh's Go, N-1751 p2).
pub fn active(s: &App, p: &Proj) -> usize {
    pieces_drawn(p, None).into_iter().filter(|(g, ps)| !crate::archive::hides(s, p, g) && !apart(g, ps)).count()
}

/// The fold key of goal G in the list of PI: toggled from its default. One for Threads and every agent's list, so a
/// topic unfolded in one is unfolded in the other.
fn fold_key(pi: usize, g: &str) -> String {
    format!("outline|{pi}|{g}")
}

/// One line of "Needs you": what asks him (its first line), the topic it is in, where a click lands.
#[derive(Clone)]
struct Need {
    /// The thread it is on and the agent who has it, for the menu of acts (acts.rs).
    id: String,
    who: Option<String>,
    words: String,
    topic: String,
    n: u64,
    at: String,
    post: Option<u32>,
    /// In his Archive (archive.rs): kept in sight, and saying so.
    from_archive: bool,
}

/// The words of W's line under Waits on you ([`need`]): the Archive's card lists what is still open with them.
pub(crate) fn need_words(s: &App, p: &Proj, w: &crate::waits::Wait) -> String {
    with_topic(&need(s, p, w))
}

/// X's words led by its topic, where they don't name it: a row whose right side says something else ("from the
/// Archive"), or a list across topics (the Archive's card), still says which topic waits ("The lexer · waits on you").
fn with_topic(x: &Need) -> String {
    if x.topic.is_empty() || x.words.contains(&x.topic) { x.words.clone() } else { format!("{} · {}", crate::refs::cut(&x.topic, 40), x.words) }
}

/// The line of "Needs you" for W (waits.rs): the first line of the question, else what waits on him: an agent's
/// question to him at his step (a `question` the lead passed to him, `ask_owner`) as an ask box's row says it, "AGENT
/// asks you: …" (73 said only "waits on you" beside the topic, four rows alike: Q-464, Q-572, Q-741 and Q-777 at
/// ask_owner; docket-edge8's walk, N-1446 p2353).
fn need(s: &App, p: &Proj, w: &crate::waits::Wait) -> Need {
    let (t, g) = (w.t, w.goal);
    let topic = label(p, g);
    let words = match w.kind {
        crate::waits::Kind::Question { post } => {
            let q = t.posts.iter().find(|q| q.n == post);
            // an ask typed to him marks no passage: the first line of its words
            let asked = |q: &docket_machine::Post| -> Vec<String> { if crate::threads::typed_ask(q) { crate::threads::read(&s.tickets, q).into_iter().collect() } else { crate::threads::ask_texts(&s.tickets, q) } };
            let first = q.and_then(|q| asked(q).into_iter().map(|a| a.lines().next().unwrap_or("").trim().to_string()).find(|a| !a.is_empty()));
            crate::refs::cut(&plain(&first.unwrap_or_else(|| format!("{}'s question in post {post}", w.agent.as_deref().unwrap_or("an agent")))), 160)
        }
        crate::waits::Kind::Ask => format!("{} asks you: {}", w.agent.as_deref().unwrap_or("an agent"), crate::refs::cut(&plain(&asked(p, t)), 160)),
        crate::waits::Kind::Step if t.kind == "question" => format!("{} asks you: {}", t.opener.as_deref().or(w.agent.as_deref()).unwrap_or("an agent"), crate::refs::cut(&plain(&asked(p, t)), 160)),
        crate::waits::Kind::Step if t.id == g.id => "waits on you".to_string(),
        crate::waits::Kind::Step => format!("{} waits on you", label(p, t)),
    };
    let post = match w.kind {
        crate::waits::Kind::Question { post } => Some(post),
        _ => None,
    };
    let at = post.map_or_else(|| t.id.clone(), |n| format!("{}.p{n}", t.id));
    Need { id: t.id.clone(), who: holder(t), words, topic, n: t.n, at, post, from_archive: crate::archive::wait_from_archive(s, p, w) }
}

/// What T asks him, whole: its title's first line, which its label cuts at 60 characters; else its label.
fn asked(p: &Proj, t: &TicketView) -> String {
    Some(t.title.lines().next().unwrap_or("").trim()).filter(|l| !l.is_empty()).map_or_else(|| label(p, t), str::to_string)
}

/// His messages in goal G's tree that no answer names yet (paper-research N-471.p1), as coordd's view holds them
/// (`unanswered_owner`: his posts to an agent that no later post answers): the message, and who has it. Who has it is
/// the first agent that still owes it (`streams::owing`, I-1455 (B)): his message to w2 and w3 that w2 has answered is
/// w3's, and no longer in w2's list (consult-3 N-1479 p141). Before `ASKED_PROTOCOL`, the first it was written to.
fn unanswered(s: &App, p: &Proj, g: &TicketView, ps: &[&TicketView], who: Option<&str>) -> Vec<(Need, String)> {
    let topic = label(p, g);
    let mut out = Vec::new();
    for t in std::iter::once(g).chain(ps.iter().copied()) {
        for m in t.posts.iter().filter(|m| t.unanswered_owner.contains(&m.n)) {
            let to: Vec<String> = match crate::streams::owing(p, t, m.n) {
                Some(owing) => owing.into_iter().map(String::from).collect(),
                None => m.names.iter().chain(&m.told).filter(|n| *n != "owner").cloned().collect(),
            };
            let has = to.first().cloned().or_else(|| holder(t)).unwrap_or_default();
            if who.is_some_and(|w| !to.iter().any(|n| n == w) && w != has) {
                continue;
            }
            // not asked of him while each agent that owes it is at work on it now (the owner, N-1790 part 3)
            if !to.is_empty() && to.iter().all(|a| crate::streams::working_on(p, t, m.n, a)) {
                continue;
            }
            let all = crate::threads::words(&s.tickets, m);
            let body = all.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('>')).unwrap_or("").to_string();
            let words = if body.is_empty() || body.starts_with("(not read") { format!("your message of {}", crate::when::at(&m.at.to_string())) } else { crate::refs::cut(&plain(&body), 90) };
            out.push((Need { id: t.id.clone(), who: holder(t), words, topic: topic.clone(), n: t.n, at: format!("{}.p{}", t.id, m.n), post: Some(m.n), from_archive: false }, has));
        }
    }
    out
}

/// Whether anything in goal G's tree is moving: an agent at work on it, or waited on.
fn moving(g: &TicketView, ps: &[&TicketView]) -> bool {
    std::iter::once(g).chain(ps.iter().copied()).any(|t| {
        let (w, owed) = state(t);
        !owed && w != "quiet" && w != "done" && w != "resting"
    })
}

/// A group's heading: "Waits on you · 3" (`refs::count`), in the one accent (window-rules §2) when it asks him; a
/// template's group's (templates.rs).
pub(crate) fn heading(words: &str, n: usize, accent: bool, k: &Kit) -> El {
    text(crate::refs::count(words, n)).font(k.ty.small).color(if accent { k.c.danger.fg } else { k.c.text_muted })
}

/// A line that lands on what it names: its words, the topic dim at the right (and who has it).
fn need_line(x: &Need, right: String, to: impl Fn(&mut App) + 'static, k: &Kit) -> El {
    k.interactive(
        row()
            .key(format!("need-{}", x.at))
            .w_full()
            .gap(10.0)
            .center_y()
            .py(2.0)
            .child(text(x.words.clone()).font(k.ty.small).color(k.c.text_base).wrap().max_lines(3).grow(1.0).shrink().w(0.0))
            .child(text(right).font(k.ty.small).color(k.c.text_faint).truncate().shrink().max_w(140.0)),
    )
    .on_press(to)
    .on_secondary_press(acts_for(x.id.clone(), x.who.clone()))
}

/// What waits on him (waits.rs `on_owner`) as [`outline`] lists it under Waits on you: all of it on Threads; with an
/// agent WHO, what stands in its topics (GOALS, [`pieces`] of WHO). An agent's count of it is this list's, so the count
/// is the list its press opens (chat.rs `facts`: 73 counted only what the agent asked, "Waits on you · 7" over a list of
/// 9 on paper-research's page; docket-edge8's walk, N-1446 p2353).
pub(crate) fn waits_in<'a>(p: &'a Proj, who: Option<&str>, goals: &[(&'a TicketView, Vec<&'a TicketView>)]) -> Vec<crate::waits::Wait<'a>> {
    crate::waits::on_owner(p).into_iter().filter(|w| who.is_none() || goals.iter().any(|(g, _)| g.id == w.goal.id)).collect()
}

/// The thread With you shows for WHO when he has picked none: what waits on him, else the newest topic of his at work
/// (its row ringed, [`row_mark`]), else his newest topic; its row is drawn picked where it stands. A topic between WHO
/// and other agents alone ([`apart`]) is never shown unasked: his talk with WHO is the page (his walk item 15).
pub fn first(p: &Proj, who: &str) -> Option<u64> {
    let goals = pieces(p, Some(who));
    if let Some(w) = waits_in(p, Some(who), &goals).first() {
        return Some(w.t.n);
    }
    let his: Vec<_> = goals.iter().filter(|(g, ps)| !apart(g, ps)).collect();
    his.iter().filter(|(g, ps)| moving(g, ps)).map(|(g, _)| g.n).max().or_else(|| his.iter().map(|(g, _)| g.n).max())
}

/// Topic G with its open pieces PS is between agents alone: the owner takes part in none of them (threads.rs `owners`:
/// he did not open it nor write on it, and no post there names him), and none waits on him. Such a topic stands behind
/// one row, Between agents, folded, on Threads and in With you, where it is the agent's errand with other agents and his
/// talk with the agent is the page; the Threads count and the unseen dot leave it out (his walk items 6 and 15;
/// yaaihh/fresh N-1724 p22 and its Go, N-1751 p2).
pub(crate) fn apart(g: &TicketView, ps: &[&TicketView]) -> bool {
    std::iter::once(g).chain(ps.iter().copied()).all(|t| !crate::threads::owners(t) && !crate::waits::owes(t))
}

/// Whether thread N stands in WHO's list as [`outline`] draws it: an open topic or piece of WHO's, or (not OPEN_ONLY)
/// one ended today.
pub fn in_list(p: &Proj, who: &str, n: u64, open_only: bool) -> bool {
    pieces(p, Some(who)).iter().any(|(g, ps)| g.n == n || ps.iter().any(|t| t.n == n)) || (!open_only && ended(p, Some(who)).iter().any(|t| t.n == n))
}

/// The list of threads (window-rules §8), one drawing wherever it stands: the project's open work on Threads (WHO
/// None), or the threads with agent WHO beside its chat, which is the same list narrowed to that agent. PICKED: the
/// thread shown beside it, drawn as picked. None when it has none.
///
/// Its parts (paper-research N-466.p1, N-471.p1; named in window-rules §8): **Waits on you** lists what asks him, one
/// line each, a click on the post that asks; **Asked, no answer yet** his messages no answer names; then **All active**,
/// every open topic in the order it started, newest first, so a thread is always where it was (the owner N-281.p9;
/// 0.2's C13): one at work is marked where it stands, never moved to a group of its own (Moving went); then
/// **Ended today**. Nothing he ever saw starts folded (the owner N-883, his inbox; window-rules §8). A topic is its
/// label and its id dim, a fold for its open pieces, and its mark ([`row_mark`]).
pub fn outline(s: &App, pi: usize, who: Option<&str>, picked: Option<u64>, k: &Kit) -> Option<El> {
    let p = s.projects.get(pi)?;
    let all = pieces_drawn(p, who);
    // what waits on him, with an agent what stands in its topics, before the Archive leaves any topic out
    let need: Vec<Need> = waits_in_drawn(p, who, &all).iter().map(|w| need(s, p, w)).collect();
    // his Archive (archive.rs): the topics it holds leave the lists under Waits on you, which keeps all it had, those
    // of the Archive marked, and his open goals in it under it (paper-research N-1446 p2350, 2)
    let goals: Vec<(&TicketView, Vec<&TicketView>)> = all.into_iter().filter(|(g, _)| !crate::archive::hides(s, p, g)).collect();
    let kept = if who.is_none() { crate::archive::goals_from_archive(s, p) } else { Vec::new() };
    let ended: Vec<&TicketView> = ended(p, who).into_iter().filter(|t| !crate::archive::hides(s, p, t)).collect();
    // the agents' reads on Threads; with an agent its card is on the same page, said once there
    let reads = if who.is_none() { crate::read_ask::lines(p, k) } else { Vec::new() };
    if goals.is_empty() && need.is_empty() && ended.is_empty() && kept.is_empty() && reads.is_empty() {
        return None;
    }
    let mut c = col().key(format!("outline-{}", who.unwrap_or("*"))).w_full().gap(4.0);
    if !need.is_empty() || !reads.is_empty() {
        c = c.child(row().w_full().pb(2.0).child(heading(&crate::look::say(s, "waits_on_you", &[]), need.len() + reads.len(), true, k)));
        c = c.children(reads);
        for x in &need {
            // marked "from the Archive" at the right, where the topic stands otherwise: the topic leads its words
            let (x, right) = if x.from_archive { (Need { words: with_topic(x), ..x.clone() }, "from the Archive".to_string()) } else { (x.clone(), crate::refs::cut(&x.topic, 24)) };
            c = c.child(need_line(&x, right, press(who, pi, x.n, x.at.clone(), x.post), k));
        }
    }
    if !kept.is_empty() {
        c = c.child(row().w_full().pt(8.0).pb(2.0).child(heading("Your goals from the Archive", kept.len(), false, k)));
        for g in &kept {
            let x = Need { id: g.id.clone(), who: holder(g), words: crate::refs::cut(&plain(&label(p, g)), 90), topic: String::new(), n: g.n, at: g.id.clone(), post: None, from_archive: true };
            c = c.child(need_line(&x, "from the Archive".into(), press(who, pi, x.n, x.at.clone(), x.post), k));
        }
    }
    let open_ones: Vec<(Need, String)> = goals.iter().flat_map(|(g, ps)| unanswered(s, p, g, ps, who)).collect();
    if !open_ones.is_empty() {
        c = c.child(row().w_full().pt(8.0).pb(2.0).child(heading(&crate::look::say(s, "asked_no_answer", &[]), open_ones.len(), false, k)));
        for (x, has) in &open_ones {
            let right = if has.is_empty() { crate::refs::cut(&x.topic, 24) } else { format!("{} · {has}", crate::refs::cut(&x.topic, 20)) };
            c = c.child(need_line(x, right, press(who, pi, x.n, x.at.clone(), x.post), k));
        }
    }
    // one group, by the date the thread started, newest first: no jump as posts arrive or work starts and stops (the
    // owner N-281.p9, "order them by date started so they are always in the same order"; 0.2's C13)
    let mut goals = goals;
    goals.sort_by_key(|(g, _)| std::cmp::Reverse(g.n));
    // the threads he takes no part in ([`apart`]) stand behind one row at the foot, not beside his: on Threads, and with
    // an agent its errands with other agents
    let (goals, others): (Vec<_>, Vec<_>) = goals.into_iter().partition(|(g, ps)| !apart(g, ps));
    let (mut ended, others_ended): (Vec<_>, Vec<_>) = ended.into_iter().partition(|t| !apart(t, &[]));
    if !goals.is_empty() {
        let name = "All active";
        let gkey = fold_key(pi, &format!("group:{name}"));
        // it folds on a click and opens on the next; it starts open
        let shown = !s.streams.open.contains(&gkey);
        let mark = if shown { "▾" } else { "▸" };
        let toggle = k.interactive(row().key(format!("group-{name}")).w_full().pt(8.0).pb(2.0).gap(6.0).center_y().child(heading(&crate::look::say(s, "all_active", &[]), goals.len(), false, k)).child(text(mark).font(k.ty.small).color(k.c.text_faint))).on_press(move |s: &mut App| {
            if !s.streams.open.remove(&gkey) {
                s.streams.open.insert(gkey.clone());
            }
        });
        c = c.child(toggle);
        if shown {
            // one at work in the text's level and ringed; the rest in the support level
            for (g, ps) in goals {
                let quiet = !moving(g, &ps);
                c = topic(s, p, pi, who, g, &ps, quiet, picked, c, k);
            }
        }
    }
    // the threads that ended, at the foot, in the same rows: coordd's snapshot holds those closed in the last day, so
    // a thread does not vanish the moment it ends (consult-3, N-412 post 1158; window-rules §8); Find has the older
    ended.sort_by_key(|t| std::cmp::Reverse(t.n));
    if !ended.is_empty() {
        let gkey = fold_key(pi, "group:Ended today");
        let shown = !s.streams.open.contains(&gkey);
        let mark = if shown { "▾" } else { "▸" };
        let toggle = k.interactive(row().key("group-Ended today").w_full().pt(8.0).pb(2.0).gap(6.0).center_y().child(heading(&crate::look::say(s, "ended_today", &[]), ended.len(), false, k)).child(text(mark).font(k.ty.small).color(k.c.text_faint))).on_press(move |s: &mut App| {
            if !s.streams.open.remove(&gkey) {
                s.streams.open.insert(gkey.clone());
            }
        });
        c = c.child(toggle);
        if shown {
            for g in ended {
                c = topic(s, p, pi, who, g, &[], true, picked, c, k);
            }
        }
    }
    if !others.is_empty() || !others_ended.is_empty() {
        c = between_agents(s, p, pi, who, others, others_ended, picked, c, k);
    }
    Some(c)
}

/// The threads he takes no part in ([`apart`]; with agent WHO, its errands with other agents) behind one row at the
/// foot of the list, "Between agents · N", folded until he opens it (`tickets.between`, the list by time's too): OTHERS
/// the open topics with their pieces, then ENDED, in the rows of the list; open while the thread beside the list is one
/// of them (a search's hit, a link), so its row is where he can see it.
#[allow(clippy::too_many_arguments)]
fn between_agents(s: &App, p: &Proj, pi: usize, who: Option<&str>, others: Vec<(&TicketView, Vec<&TicketView>)>, ended: Vec<&TicketView>, picked: Option<u64>, mut c: El, k: &Kit) -> El {
    let inside = |n: u64| others.iter().any(|(g, ps)| g.n == n || ps.iter().any(|t| t.n == n)) || ended.iter().any(|t| t.n == n);
    let shown = s.tickets.between || picked.is_some_and(inside);
    let mark = if shown { "▾" } else { "▸" };
    let n = others.len() + ended.len();
    let toggle = k
        .interactive(row().key("group-between-agents").w_full().pt(8.0).pb(2.0).gap(6.0).center_y().child(heading("Between agents", n, false, k)).child(text(mark).font(k.ty.small).color(k.c.text_faint)))
        .on_press(|s: &mut App| s.tickets.between = !s.tickets.between);
    c = c.child(toggle);
    if shown {
        for (g, ps) in others {
            let quiet = !moving(g, &ps);
            c = topic(s, p, pi, who, g, &ps, quiet, picked, c, k);
        }
        for g in ended {
            c = topic(s, p, pi, who, g, &[], true, picked, c, k);
        }
    }
    c
}

/// The top threads that ended which the snapshot still holds (closed in the last day), the project's or WHO's.
fn ended<'a>(p: &'a Proj, who: Option<&str>) -> Vec<&'a TicketView> {
    let mine = |v: &TicketView| who.is_none_or(|w| is_with(v, w));
    p.live.tickets.values().filter(|t| outlined(t) && !open(t) && parent(p, t).is_none() && mine(t)).collect()
}

/// Topic G's row in the list (its label, the id quiet at the right end, a fold for its open pieces PS), and its pieces
/// when unfolded, added to C. A mark says its state without words (window-rules §3, §5; [`row_mark`]): the accent dot
/// where it waits on him, a dot in the text's colour where something on it is new to him (seen.rs), a ring where it
/// is at work (not QUIET); nothing else.
#[allow(clippy::too_many_arguments)]
fn topic(s: &App, p: &Proj, pi: usize, who: Option<&str>, g: &TicketView, ps: &[&TicketView], quiet: bool, picked: Option<u64>, mut c: rune::El, k: &Kit) -> El {
    let key = fold_key(pi, &g.id);
    // a topic opens on a press on its ▸ and stays as he left it (N-412, the owner N-446.p1)
    let shown = s.streams.open.contains(&key) && !ps.is_empty();
    let fresh = |t: &TicketView| crate::seen::is_new(s, pi, t);
    let new = fresh(g) || ps.iter().any(|t| fresh(t));
    let mark = if ps.is_empty() { " " } else if shown { "▾" } else { "▸" };
    let fold = k.interactive(row().key(format!("fold-{}", g.id)).w(14.0).child(text(mark).font(k.ty.small).color(k.c.text_faint))).on_press(move |s: &mut App| {
        if !s.streams.open.remove(&key) {
            s.streams.open.insert(key.clone());
        }
    });
    let label_el = text(label(p, g)).font(k.ty.body).color(if quiet && !new { k.c.text_muted } else { k.c.text_base }).truncate().grow(1.0).shrink().w(0.0);
    // its fields in the look's order (`[list] row`)
    let fields = crate::look::row_fields(s, row_mark(&g.id, state(g).1, new, !quiet, k), label_el, text(g.id.clone()).font(k.ty.small).color(k.c.text_faint));
    let name = k.interactive(row().grow(1.0).shrink().w(0.0).gap(8.0).center_y().children(fields)).on_press(press(who, pi, g.n, g.id.clone(), None));
    c = c.child(pick(row().key(format!("goal-{}", g.id)).on_secondary_press(acts_at(g)).w_full().gap(6.0).py(2.0).center_y().child(fold).child(name), picked == Some(g.n), k));
    if shown {
        for t in ps {
            let depth = {
                let mut d = 0;
                let mut x = *t;
                while let Some(q) = parent(p, x) {
                    if q.id == g.id {
                        break;
                    }
                    d += 1;
                    x = q;
                }
                d as f32
            };
            let fields = crate::look::row_fields(s, row_mark(&t.id, state(t).1, fresh(t), moving(t, &[]), k), text(label(p, t)).font(k.ty.small).color(k.c.text_base).truncate().grow(1.0).shrink().w(0.0), text(t.id.clone()).font(k.ty.small).color(k.c.text_faint));
            let r = row().w_full().gap(8.0).center_y().children(fields);
            let line = row().key(format!("piece-{}", t.id)).w_full().pl(20.0 + 16.0 * depth).py(2.0).child(r);
            c = c.child(k.interactive(pick(line, picked == Some(t.n), k)).on_press(press(who, pi, t.n, t.id.clone(), None)).on_secondary_press(acts_at(t)));
        }
    }
    c
}

/// A row's mark: the accent dot where it waits on him (OWED), a dot in the text's colour where something is NEW.
pub fn dot(owed: bool, new: bool, k: &Kit) -> El {
    let fill = if owed { k.c.danger.fg } else if new { k.c.text_base } else { rune::Color::TRANSPARENT };
    rune::el().w(6.0).h(6.0).radius(3.0).fill(fill)
}

/// Thread ID's row's mark: [`dot`] where it waits on him (OWED) or something on it is NEW; else, where it is ACTIVE
/// (an agent at work on it, or something under it waited on), a ring in the support level, keyed `active-ID`: the
/// row stays where it started and says it moves (0.2's C13, "active marked").
fn row_mark(id: &str, owed: bool, new: bool, active: bool, k: &Kit) -> El {
    if owed || new || !active {
        return dot(owed, new, k);
    }
    rune::el().key(format!("active-{id}")).w(6.0).h(6.0).radius(3.0).border(1.5, k.c.text_muted)
}

/// The row of the thread shown beside the list, filled.
fn pick(r: El, on: bool, k: &Kit) -> El {
    if on { r.radius(k.radius.md).fill(k.c.layer_02) } else { r }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, parent: Option<&str>, to: &str, owner_waits: bool) -> TicketView {
        let waits = if owner_waits { serde_json::json!([{"agent": "owner", "ticket": id, "call": "decide"}]) } else { serde_json::json!([]) };
        serde_json::from_value(serde_json::json!({"id": id, "n": id[2..].parse::<u64>().unwrap(), "kind": "thread", "parent": parent, "opener": "owner", "ctx": {},
            "form": {"to": to}, "title": format!("about {id}"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": format!("2026-11-17T0{}:00:00Z", id.len() % 9), "given": [], "posts": [], "waits_on": waits}))
        .unwrap()
    }

    #[test]
    fn a_subthread_stands_under_its_goal_and_what_waits_on_him_is_counted() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        for v in [t("N-186", None, "paper-research", false), t("N-379", Some("N-186"), "docket-core", false), t("N-381", Some("N-379"), "docket-core3", false), t("N-9001", Some("N-186"), "paper-research", true)] {
            p.live.tickets.insert(v.n, v);
        }
        let n379 = p.live.tickets[&379].clone();
        assert_eq!(parent(&p, &n379).map(|v| v.id.as_str()), Some("N-186"));
        assert_eq!(goal(&p, &p.live.tickets[&381]).id, "N-186");
        let subs: Vec<&str> = subthreads(&p, &p.live.tickets[&186]).iter().map(|v| v.id.as_str()).collect();
        assert_eq!(subs.len(), 2);
        // with docket-core3: its piece under the goal; with paper-research: the goal's pieces it is on
        let mine = pieces(&p, Some("docket-core3"));
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].1.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["N-381"]);
        assert_eq!(state(&p.live.tickets[&9001]), ("waits on you".to_string(), true));
    }

    /// A question the lead passed to him, waiting at his step (`ask_owner`), says what it asks, as an ask box's row
    /// does: "pr asks you: …", not "waits on you" (73: four rows alike, Q-464, Q-572, Q-741, Q-777; docket-edge8's walk,
    /// N-1446 p2353).
    #[test]
    fn a_question_at_his_step_says_who_asks_and_what() {
        let (_held, s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut q = t("Q-777", None, "", true);
        (q.kind, q.opener, q.title) = ("question".into(), Some("pr".into()), "May a name change an answer?".into());
        q.form = serde_json::json!({"ask": "May a name change an answer?"});
        q.waits_on = serde_json::from_value(serde_json::json!([{"agent": "owner", "ticket": "Q-777", "call": "ask_owner"}])).unwrap();
        q.status = serde_json::from_value(serde_json::json!({"is": "awaits", "agent": "owner", "call": "ask_owner", "step": 1, "since": "2026-11-20T08:00:00Z",
            "taken": false, "refusals": 0, "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}})).unwrap();
        p.live.tickets.insert(q.n, q);
        let waits = crate::waits::on_owner(&p);
        assert_eq!(waits.len(), 1, "it waits on him");
        assert_eq!(need(&s, &p, &waits[0]).words, "pr asks you: May a name change an answer?");
    }

    /// Two of consult-lead's questions to him, alike in their first words.
    const ASKS: [&str; 2] = ["Shall the follower read the record before the lock is let go, or after it?", "Shall the follower read the record from the old file, or the new one it was moved to?"];

    /// Project research's Threads, [`ASKS`] waiting on him.
    fn alike() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        s.notice = None;
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        for (i, title) in ASKS.iter().enumerate() {
            let mut q = t(&format!("Q-{}", 700 + i), None, "", true);
            (q.kind, q.opener, q.title) = ("question".into(), Some("consult-lead".into()), title.to_string());
            q.waits_on = serde_json::from_value(serde_json::json!([{"agent": "owner", "ticket": q.id, "call": "ask_owner"}])).unwrap();
            q.status = serde_json::from_value(serde_json::json!({"is": "awaits", "agent": "owner", "call": "ask_owner", "step": 1, "since": "2026-11-26T08:00:00Z",
                "taken": false, "refusals": 0, "forming": false, "head": {"Variant": [["Answer", [["text", "Text"]]]]}, "args": {}})).unwrap();
            p.live.tickets.insert(q.n, q);
        }
        p.live.tickets_rev += 1;
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        (held, s)
    }

    /// Threads in a narrow window, [`ASKS`] each read whole under Waits on you (waits-wrapped.png).
    /// `STILL_OUT=DIR yaaihh-tests outline::tests::waits_still --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn waits_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        std::fs::create_dir_all(&out).unwrap();
        let args = vec!["--snap".to_string(), std::path::Path::new(&out).join("waits-wrapped.png").display().to_string(), "--size".into(), "620x500".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (620.0, 500.0), || alike().1, crate::view) }).args(args).try_run().unwrap();
    }

    /// Two of consult-lead's questions alike in their first words (the QA walk's P1 #10, N-1890.p1): under Waits on you
    /// each row's words wrap to be read whole, in a narrow list as on a wide one, not cut to one line alike.
    #[test]
    fn waits_on_you_rows_wrap_so_two_alike_at_their_start_read_apart() {
        let (_held, s) = alike();
        // wide, each row's words have the room beside the topic; narrow, they wrap
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (900.0, 700.0), rune::Env::new(false, 1.0));
        for (wide, size) in [(true, (900.0, 700.0)), (false, (620.0, 700.0))] {
            rt.set_viewport(size, 1.0);
            for _ in 0..4 {
                rt.invalidate();
                rt.frame(0.05);
            }
            for title in ASKS {
                let words = format!("consult-lead asks you: {title}");
                let e = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(words.as_str())).unwrap_or_else(|| panic!("{title} whole"));
                assert!(if wide { e.rect[2] > 400.0 } else { e.rect[3] > 20.0 }, "{size:?}: {:?}", e.rect);
            }
        }
    }

    #[test]
    fn coordds_outline_decides_the_parent_the_state_and_where_the_line_stands() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let line = |id: &str, parent: Option<&str>, at: Option<u32>, state: &str, owed: bool| serde_json::json!({"id": id, "label": format!("about {id}"), "parent": parent,
            "at_post": at, "who": "docket-core", "state": state, "owed": owed, "open": true, "opened_at": "2026-11-17T09:00:00Z"});
        let mut g = t("N-186", None, "paper-research", false);
        g.outline = Some(serde_json::from_value(serde_json::json!({"id": "N-186", "label": "Tovrin", "state": "quiet", "open": true, "opened_at": "2026-11-16T07:00:00Z",
            "children": [line("N-379", Some("N-186"), Some(2), "at work", false)]})).unwrap());
        g.posts = serde_json::from_value(serde_json::json!([{"n": 1, "by": "owner", "at": "2026-11-16T07:00:00Z", "text": "a"}, {"n": 2, "by": "owner", "at": "2026-11-16T08:00:00Z", "text": "b"}])).unwrap();
        // opened on its own, tied later: no `parent` of its own, the outline's tie decides
        let mut c = t("N-379", None, "paper-research", true);
        c.outline = Some(serde_json::from_value(line("N-379", Some("N-186"), Some(2), "at work", false)).unwrap());
        for v in [g, c] {
            p.live.tickets.insert(v.n, v);
        }
        let (g, c) = (&p.live.tickets[&186], &p.live.tickets[&379]);
        assert_eq!(parent(&p, c).map(|v| v.id.as_str()), Some("N-186"));
        assert_eq!(subthreads(&p, g).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["N-379"]);
        // the engine's word, not the waits the view holds; its label as coordd gives it
        assert_eq!(state(c), ("at work".to_string(), false));
        assert_eq!(label(&p, c), "about N-379");
        // after post 2, before anything later
        assert!(place(g, c).as_str() > "2026-11-16T08:00:00Z" && place(g, c).as_str() < "2026-11-16T08:00:01Z");
    }

    /// The project's owner model is hidden (paper-research N-1446 p1121, proto-lead p1123): its standing ticket, of a
    /// kind that says no `drawn` (coordd's view gives it none), and an update of it, opened by coordd under it, are
    /// neither topics of the outline nor rows of the list by time or of what Texts reads (`threads::listed`), beside his
    /// thread, which is both. The Map draws the standing one at the project's level.
    #[test]
    fn the_owner_model_and_its_updates_are_in_no_list_of_his() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let his = t("N-1", None, "lead", false);
        let mut node = t("N-2", None, "lead", false);
        node.kind = coord_types::OWNER_MODEL.into();
        let mut update = t("N-3", None, "lead", false);
        (update.kind, update.under, update.opener) = (coord_types::OWNER_UPDATE.into(), Some("N-2".into()), Some("coordd".into()));
        assert!(node.drawn.is_none() && update.drawn.is_none());
        for v in [his, node, update] {
            p.live.tickets.insert(v.n, v);
        }
        let topics: Vec<&str> = pieces(&p, None).iter().map(|(g, _)| g.id.as_str()).collect();
        assert_eq!(topics, ["N-1"], "the outline");
        let rows: Vec<&str> = p.live.tickets.values().filter(|v| crate::threads::listed(v)).map(|v| v.id.as_str()).collect();
        assert_eq!(rows, ["N-1"], "the list by time");
    }

    /// The brain's passes standing at one post of his, as coordd's machine makes them (map/deep.rs `brain_passes`,
    /// made up): one that spoke, one that ended Quiet, one at work with no post. `standing` holds all three, each with
    /// its line, in the order of the chat's line's children; `marks`, what his thread draws, only the one that spoke
    /// (proto-window N-1446 p1144). The Map reads `standing`, and draws each pass inside the post.
    #[test]
    fn standing_holds_every_pass_at_a_post_and_marks_only_the_one_that_spoke() {
        let (line, passes) = crate::map::deep::tests::brain_passes();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut g = t(&line.id, None, "pr", false);
        g.outline = Some(line);
        let ids: Vec<String> = passes.iter().map(|v| v.id.clone()).collect();
        for v in passes {
            p.live.tickets.insert(v.n, v);
        }
        let n = g.n;
        p.live.tickets.insert(n, g);
        let g = &p.live.tickets[&n];
        let all = standing(&p, g, |_| true);
        assert_eq!(all.iter().map(|(c, t)| (t.id.clone(), c.at_post)).collect::<Vec<_>>(), ids.iter().map(|i| (i.clone(), Some(1))).collect::<Vec<_>>(), "all three, at post 1");
        assert_eq!(marks(&p, g).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), [ids[0].as_str()], "his thread: only the pass that spoke");
        // the two it leaves out, by their lines: the Quiet one came to nothing, the one at work has said nothing yet
        assert!(came_to_nothing(all[1].0) && !not_yet(all[1].0, all[1].1), "Quiet");
        assert!(!came_to_nothing(all[2].0) && not_yet(all[2].0, all[2].1), "at work, no post");
        assert!(standing(&p, g, |c| c.at_post == Some(2)).is_empty(), "KEEP is asked of each line");
    }

    /// N-1104 as tonight: subthread N-1445 opened at his #593, the brain's mark B-1 at his #619, and N-9 from a coordd
    /// before the clause. B-1 is a pass that spoke: its thought is its post, and it came to `Said(…)`, which coordd says
    /// in words as "Said" (`view::said_in_words`).
    fn n1104() -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let line = |id: &str, at: u32| serde_json::json!({"id": id, "label": format!("about {id}"), "parent": "N-1104", "at_post": at, "state": "done", "open": false, "opened_at": "2026-11-20T19:01:20Z"});
        let mut g = t("N-1104", None, "paper-research", false);
        let mut b1 = line("B-1", 619);
        b1["said"] = serde_json::json!("Said");
        g.drawn = Some(docket_machine::Drawn::Line);
        g.outline = Some(serde_json::from_value(serde_json::json!({"id": "N-1104", "label": "Your one group chat", "state": "quiet", "open": true, "opened_at": "2026-11-16T07:00:00Z",
            "children": [line("N-1445", 593), b1]})).unwrap());
        g.posts = serde_json::from_value(serde_json::json!([{"n": 593, "by": "owner", "at": "2026-11-20T18:25:49Z", "text": "a"}, {"n": 619, "by": "owner", "at": "2026-11-20T19:01:07Z", "text": "b"}, {"n": 620, "by": "paper-research", "at": "2026-11-20T19:01:46Z", "text": "c"}])).unwrap();
        let sub = t("N-1445", None, "paper-research-brain", false);
        let mut mark = t("B-1", None, "paper-research", false);
        (mark.kind, mark.drawn) = ("brain".into(), Some(docket_machine::Drawn::Note));
        mark.posts = serde_json::from_value(serde_json::json!([{"n": 1, "by": "watcher", "at": "2026-11-20T19:01:20Z", "text": "He asked for the stills twice; send them before anything else.\nThe rest can wait."}])).unwrap();
        let mut old = t("N-9", None, "x", false);
        old.drawn = None;
        for v in [g, sub, mark, old] {
            p.live.tickets.insert(v.n, v);
        }
        p
    }

    /// A mark's line says its own words first (proto-reader N-1446 p197, consult-3 p205), in brain.kind's and
    /// aside.kind's real shapes: a pass that spoke says its thought, its post, not what it came to; an aside joined
    /// back, with no post, says "Joined" as coordd words it; one waiting for his press, nothing come to yet, says its
    /// label. No line holds a value as the engine prints it. A mark that came to nothing (a pass that said nothing, one
    /// whose call failed, an aside he dropped) stands nowhere: no line among the thread's rows, no sign in the margin
    /// (proto-mind p270).
    #[test]
    fn a_marks_line_says_its_post_else_what_it_came_to_in_words_else_its_label() {
        let s = crate::init();
        let mut p = n1104();
        let node = |id: &str, state: &str, said: Option<&str>| serde_json::json!({"id": id, "label": format!("about {id}"), "parent": "N-1104", "at_post": 620, "state": state, "open": state == "waits on you", "opened_at": "2026-11-20T19:02:00Z", "said": said});
        let nodes = [("A-2", "done", Some("Joined"), "aside"), ("A-3", "waits on you", None, "aside"), ("B-4", "said nothing", Some("Quiet"), "brain"), ("B-5", "failed", Some("the attention call failed: Codex at its limit"), "brain"), ("A-6", "dropped", Some("he pressed Not this"), "aside")];
        let g = p.live.tickets.get_mut(&1104).unwrap();
        g.outline.as_mut().unwrap().children.extend(nodes.iter().map(|(id, state, said, _)| serde_json::from_value(node(id, state, *said)).unwrap()));
        for (id, state, _, kind) in nodes {
            let mut m = t(id, None, "paper-research", state == "waits on you");
            (m.kind, m.drawn) = (kind.into(), Some(if kind == "brain" { docket_machine::Drawn::Note } else { docket_machine::Drawn::Mark }));
            p.live.tickets.insert(m.n, m);
        }
        let g = &p.live.tickets[&1104];
        let says: Vec<String> = marks(&p, g).into_iter().map(|t| mark_says(&s, &p, g, t)).collect();
        assert_eq!(says, ["He asked for the stills twice; send them before anything else. …", "Joined", "about A-3"]);
        assert!(says.iter().all(|w| !w.contains('(') && !w.contains('"')), "{says:?}");
        // what came to nothing: no row of the thread, no sign in the margin
        let rows = crate::threads::seq(&p, g);
        assert!([4, 5, 6].iter().all(|n| !rows.contains(&crate::threads::TRow::Opened(*n))) && rows.contains(&crate::threads::TRow::Opened(2)), "{rows:?}");
        assert_eq!(crate::margin::of(&p, g).iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["B-1", "A-2", "A-3"]);
    }

    /// A note stands at his post only once it has spoken (consult-3 N-1446 p890, the kind's word p901): B-7, a brain's
    /// pass (brain.kind's `drawn note`), at work with no post is no row of the thread and no sign in the margin; its
    /// thought posted, still at work, it stands and says the thought; ended Quiet, it stands nowhere again. At no point
    /// does a pass that comes to Quiet stand. The same pass drawn as a mark stands at work: the kind says it, not who
    /// opened it (margin.rs's bracket test has a cut at work, a mark, standing).
    #[test]
    fn a_note_at_work_stands_only_once_it_has_spoken() {
        let s = crate::init();
        let mut p = n1104();
        let node = |state: &str, open: bool, said: Option<&str>| -> docket_machine::ViewOutline {
            serde_json::from_value(serde_json::json!({"id": "B-7", "label": "one pass of an agent's brain", "parent": "N-1104", "at_post": 620, "state": state, "open": open, "opened_at": "2026-11-20T19:02:00Z", "said": said})).unwrap()
        };
        let mut b7 = t("B-7", None, "paper-research", false);
        (b7.kind, b7.drawn) = ("brain".into(), Some(docket_machine::Drawn::Note));
        let n = b7.n;
        p.live.tickets.insert(n, b7);
        let set = |p: &mut crate::Proj, line: docket_machine::ViewOutline| {
            let g = p.live.tickets.get_mut(&1104).unwrap();
            let kids = &mut g.outline.as_mut().unwrap().children;
            kids.retain(|c| c.id != "B-7");
            kids.push(line);
        };
        let stands = |p: &crate::Proj| {
            let g = &p.live.tickets[&1104];
            let drawn = marks(p, g).iter().any(|v| v.id == "B-7");
            let rows = crate::threads::seq(p, g);
            assert_eq!(rows.contains(&crate::threads::TRow::Opened(n)), drawn, "its row as its mark: {rows:?}");
            assert_eq!(crate::margin::of(p, g).iter().any(|x| x.id == "B-7"), drawn, "its margin sign as its mark");
            drawn
        };
        // at work, its calls running: nothing drawn
        set(&mut p, node("at work", true, None));
        assert!(!stands(&p), "a note at work that has said nothing");
        // the same at work drawn as a mark: it stands
        p.live.tickets.get_mut(&n).unwrap().drawn = Some(docket_machine::Drawn::Mark);
        assert!(stands(&p), "a mark at work stands");
        p.live.tickets.get_mut(&n).unwrap().drawn = Some(docket_machine::Drawn::Note);
        // its thought posted, the pass not yet closed: it stands and says the thought
        p.live.tickets.get_mut(&n).unwrap().posts = serde_json::from_value(serde_json::json!([{"n": 1, "by": "B-7", "at": "2026-11-20T19:02:20Z", "text": "He is on the brain; I answer that first."}])).unwrap();
        assert!(stands(&p));
        let g = &p.live.tickets[&1104];
        assert_eq!(mark_says(&s, &p, g, &p.live.tickets[&n]), "He is on the brain; I answer that first.");
        // a pass that ends Quiet: no post at work, and nothing at its end
        p.live.tickets.get_mut(&n).unwrap().posts.clear();
        set(&mut p, node("at work", true, None));
        assert!(!stands(&p));
        set(&mut p, node("said nothing", false, Some("Quiet")));
        assert!(!stands(&p));
    }

    /// A mark's line names its kind by its doc's head, so its thought keeps the line (proto-window N-1446, the stills of
    /// p906: brain.kind's whole first line took the line, its thought cut at the window's edge): brain.kind's line as
    /// written; a label with no colon, and a kind with no doc (its name), as they are.
    #[test]
    fn a_marks_line_names_its_kind_by_its_docs_head() {
        let brain = include_str!("../../../agent-base/kinds/brain.kind").lines().find_map(|l| l.strip_prefix("/// ")).unwrap();
        assert!(brain.starts_with("one pass of an agent's brain: "), "{brain}");
        assert_eq!((mark_kind(brain), mark_kind("an aside"), mark_kind("brain")), ("one pass of an agent's brain".into(), "an aside".into(), "brain".into()));
    }

    /// A kind says how its tickets stand in a thread (`drawn`, proto-lead N-1446 p17): a mark at his post is one of the
    /// thread's rows there, never a subthread or a line of the outline; a coordd before the clause says nothing, and a
    /// thread or item is a line as before.
    #[test]
    fn a_mark_stands_at_its_post_and_is_no_subthread() {
        let p = n1104();
        let g = &p.live.tickets[&1104];
        assert_eq!(subthreads(&p, g).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["N-1445"]);
        assert_eq!(marks(&p, g).iter().map(|v| v.id.as_str()).collect::<Vec<_>>(), ["B-1"]);
        // at his #619, before #620: the parent's line for it says where (its own view has no outline)
        let at = place(g, &p.live.tickets[&1]);
        assert!(at.as_str() > "2026-11-20T19:01:07Z" && at.as_str() < "2026-11-20T19:01:08Z", "{at}");
        assert!(!is_line(&p.live.tickets[&1]) && is_line(&p.live.tickets[&1445]) && is_line(&p.live.tickets[&9]), "a thread from a coordd before the clause is a line");
        // the thread's rows: the subthread's line at #593 and the mark at #619, in place
        let rows = crate::threads::seq(&p, g);
        let at_of = |r: &crate::threads::TRow| rows.iter().position(|x| x == r).unwrap();
        use crate::threads::TRow;
        assert!(at_of(&TRow::Opened(1445)) < at_of(&TRow::Post(619)) && at_of(&TRow::Post(619)) < at_of(&TRow::Opened(1)) && at_of(&TRow::Opened(1)) < at_of(&TRow::Post(620)), "{rows:?}");
    }

    /// A mark's line opens it the way every line of a thread opens (find::open_at, consult-3 N-1446 p106): pressed in
    /// N-1104 beside paper-research's chat, the mark opens on Threads, where setting Threads' ticket alone showed
    /// nothing.
    #[test]
    fn a_marks_line_opens_it_beside_an_agents_chat_too() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = n1104();
        p.down = None;
        p.live.agents = vec![serde_json::from_value(serde_json::json!({"id": "AGT-1", "name": "paper-research", "parent": "owner", "kind": "claude",
            "model": "m", "state": "idle", "since": "2026-11-20T18:00:00Z", "tools": [], "items": []})).unwrap()];
        s.projects = vec![p];
        s.cur = 0;
        let who = "paper-research".to_string();
        s.sel = crate::Sel::Agent(who.clone());
        s.streams.which.insert((0, who.clone()), crate::streams::Which::You);
        s.withyou.inside.insert((0, who), 1104);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..3 {
            rt.frame(0.016);
        }
        let (x, y) = rt.find("mark-B-1").expect("the mark is drawn in N-1104 beside the chat");
        // its words are its own post's first line, the brain's thought (`mark_says`), with its id
        let says: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == "mark-B-1")).filter_map(|e| e.text).collect();
        assert!(says.iter().any(|t| t.contains("He asked for the stills twice")) && says.iter().any(|t| t == "B-1"), "{says:?}");
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        assert!(matches!(rt.state.sel, crate::Sel::Tickets), "it opens on Threads");
        assert_eq!(rt.state.tickets.shown, Some(1), "the mark B-1 is the ticket shown");
    }

    #[test]
    fn needs_you_lists_the_asks_and_no_answer_yet_his_unanswered_messages() {
        let s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut g = t("N-186", None, "paper-research", true);
        g.posts = serde_json::from_value(serde_json::json!([
            {"n": 1, "by": "owner", "at": "2026-11-18T07:00:00Z", "text": "how is it going", "names": ["paper-research"]},
            {"n": 2, "by": "paper-research", "at": "2026-11-18T08:00:00Z", "text": "Shall I merge it?", "told": ["owner"], "asks_open": true, "asks": [{"at": [0, 17], "options": []}]},
            {"n": 3, "by": "owner", "at": "2026-11-18T09:00:00Z", "text": "and the stills?", "names": ["paper-research"]}])).unwrap();
        g.unanswered_owner = vec![3];
        let quiet = t("N-200", None, "docket-core", false);
        for v in [g, quiet] {
            p.live.tickets.insert(v.n, v);
        }
        let g = &p.live.tickets[&186];
        let need: Vec<Need> = crate::waits::on_owner(&p).iter().map(|w| need(&s, &p, w)).collect();
        assert_eq!(need.iter().map(|x| (x.at.as_str(), x.words.as_str())).collect::<Vec<_>>(), [("N-186.p2", "Shall I merge it?")]);
        let open = unanswered(&s, &p, g, &[], None);
        assert_eq!(open.iter().map(|(x, has)| (x.at.as_str(), x.words.as_str(), has.as_str())).collect::<Vec<_>>(), [("N-186.p3", "and the stills?", "paper-research")]);
        assert_eq!(plain("1. **Token figures** with `x`"), "Token figures with x");
        // a topic nobody is at work on is quiet
        assert!(!moving(&p.live.tickets[&200], &[]));
        // post 2 typed an ask to him (I-1480), marking no passage: the first line of its words
        let v = p.live.tickets.get_mut(&186).unwrap();
        v.posts[1].asks.clear();
        v.posts[1].kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
        v.posts[1].text = serde_json::from_value(serde_json::json!("**Shall I merge it** tonight?\nIts line is green.")).unwrap();
        let typed: Vec<(String, String)> = crate::waits::on_owner(&p).iter().map(|w| super::need(&s, &p, w)).map(|x| (x.at, x.words)).collect();
        assert_eq!(typed, [("N-186.p2".to_string(), "Shall I merge it tonight?".to_string())]);
    }

    /// His N-186.p1 asks w2 and w3, and w2 answers it at p2 (consult-3 N-1479 p141, I-1455 (B)). "Asked, no answer
    /// yet" says w3 has it, not w2, the first name he typed; it is in w3's list and no longer in w2's. From a coordd
    /// before `ASKED_PROTOCOL`, which sends no asks, it is w2's, as before.
    #[test]
    fn asked_no_answer_yet_names_the_agent_that_still_owes_it() {
        let s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::ASKED_PROTOCOL;
        let mut g = t("N-186", None, "w2", false);
        g.posts = serde_json::from_value(serde_json::json!([
            {"n": 1, "by": "owner", "at": "2026-11-21T16:00:00Z", "text": "@w2 @w3 can you both check the stills?", "names": ["w2", "w3"], "told": ["w2", "w3"]},
            {"n": 2, "by": "w2", "at": "2026-11-21T16:05:00Z", "text": "Mine are right: 4 of 4.", "names": ["owner"], "answering": ["owner"]}])).unwrap();
        g.unanswered_owner = vec![1];
        g.asked = serde_json::from_value(serde_json::json!([{"post": 1, "agent": "w2", "answer": "N-186.p2"}, {"post": 1, "agent": "w3"}])).unwrap();
        p.live.tickets.insert(g.n, g.clone());
        let has = |p: &Proj, who: Option<&str>| unanswered(&s, p, &p.live.tickets[&186], &[], who).into_iter().map(|(x, has)| (x.at, has)).collect::<Vec<_>>();
        let w3 = vec![("N-186.p1".to_string(), "w3".to_string())];
        assert_eq!((has(&p, None), has(&p, Some("w3")), has(&p, Some("w2"))), (w3.clone(), w3, vec![]), "w3 owes it; w2 answered it");
        p.protocol = coord_types::ASKED_PROTOCOL - 1;
        g.asked.clear();
        p.live.tickets.insert(g.n, g);
        let w2 = vec![("N-186.p1".to_string(), "w2".to_string())];
        assert_eq!((has(&p, None), has(&p, Some("w2"))), (w2.clone(), w2), "before 39, the first it was written to");
    }

    /// The owner (N-1790 part 3): no urgency while the agent is at work on his message. His N-186.p1 to w3 stands in
    /// "Asked, no answer yet" while w3 is idle; it leaves the list while w3 is in the turn it was handed in (or into),
    /// and comes back when that turn ends with no answer, and stays in a later turn it was not handed in.
    #[test]
    fn a_message_its_agent_is_at_work_on_is_not_asked_of_him() {
        let s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.protocol = coord_types::ASKED_PROTOCOL;
        let mut g = t("N-186", None, "w3", false);
        g.posts = serde_json::from_value(serde_json::json!([
            {"n": 1, "by": "owner", "at": "2026-11-21T16:00:00Z", "text": "@w3 can you check the stills?", "names": ["w3"], "told": ["w3"]}])).unwrap();
        g.unanswered_owner = vec![1];
        g.asked = serde_json::from_value(serde_json::json!([{"post": 1, "agent": "w3"}])).unwrap();
        g.given = serde_json::from_value(serde_json::json!([{"to": "w3", "delivery": "N-186.d1", "upto": 1, "at": "2026-11-21T16:00:05Z"}])).unwrap();
        p.live.tickets.insert(g.n, g);
        let mut w3 = crate::view_cost::project_for_tests().live.agents[0].clone();
        w3.name = "w3".into();
        let listed = |p: &Proj| unanswered(&s, p, &p.live.tickets[&186], &[], None).len();
        let set = |p: &mut Proj, state: &str, since: &str| {
            let mut a = w3.clone();
            (a.state, a.since) = (state.into(), since.into());
            p.live.agents = vec![a];
        };
        set(&mut p, "idle", "2026-11-21T15:00:00Z");
        assert_eq!(listed(&p), 1, "idle: asked of him");
        set(&mut p, "working", "2026-11-21T15:59:00Z");
        assert_eq!(listed(&p), 0, "at work in the turn it was handed into: not asked of him");
        assert!(crate::streams::working_on(&p, &p.live.tickets[&186], 1, "w3"));
        set(&mut p, "working", "2026-11-21T16:00:05Z");
        assert_eq!(listed(&p), 0, "at work in the turn it began");
        set(&mut p, "idle", "2026-11-21T16:09:00Z");
        assert_eq!(listed(&p), 1, "that turn ended with no answer: asked again");
        set(&mut p, "working", "2026-11-21T16:20:00Z");
        assert_eq!(listed(&p), 1, "at work in a later turn it was not handed in: still asked");
    }

    /// 0.2's C13 (the owner N-281.p9: "order them by date started so they are always in the same order"): on Threads
    /// the topics stand in the order they started, newest first, one at work among them where it started, ringed;
    /// work starting on another moves nothing, it is ringed where it stands. No group of its own for what moves.
    #[test]
    fn a_thread_stays_where_it_started_and_one_at_work_is_ringed_there() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        p.down = None;
        let at_work = |id: &str| -> Option<docket_machine::ViewOutline> {
            Some(serde_json::from_value(serde_json::json!({"id": id, "label": format!("about {id}"), "state": "at work", "open": true, "opened_at": "2026-11-17T09:00:00Z"})).unwrap())
        };
        for id in ["N-10", "N-11", "N-12"] {
            let mut v = t(id, None, "paper-research", false);
            if id == "N-11" {
                v.outline = at_work(id);
            }
            p.live.tickets.insert(v.n, v);
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        rt.frame(0.016);
        let y = |rt: &rune::runtime::Runtime<App>, key: &str| rt.find(key).unwrap_or_else(|| panic!("{key} is drawn")).1;
        let order = |rt: &rune::runtime::Runtime<App>| (y(rt, "goal-N-12"), y(rt, "goal-N-11"), y(rt, "goal-N-10"));
        let (a, b, c) = order(&rt);
        assert!(a < b && b < c, "newest first, the one at work in its place: {:?}", (a, b, c));
        assert!(rt.find("active-N-11").is_some() && rt.find("active-N-10").is_none(), "only the one at work is ringed");
        assert!(rt.find("group-Moving").is_none() && rt.find("group-All active").is_some(), "one group");
        // work starts on the oldest: it is ringed where it stands, and nothing moves
        rt.update(|s, _| s.projects[0].live.tickets.get_mut(&10).unwrap().outline = at_work("N-10"));
        rt.invalidate();
        rt.frame(0.016);
        rt.frame(0.016);
        assert_eq!(order(&rt), (a, b, c), "no row moved");
        assert!(rt.find("active-N-10").is_some(), "the oldest ringed where it stands");
    }
}

/// Whether thread V is in WHO's list: WHO keeps it or is on it (an agent's With you narrows the list to these, and its
/// search's hits to these threads, find.rs `agent_results`).
pub fn is_with(v: &TicketView, who: &str) -> bool {
    holder(v).as_deref() == Some(who) || agents_on(v).iter().any(|a| a == who)
}

/// The agents on V, in the order they came: its opener, whom its posts named or told or informed, who posted, who was
/// given something. The owner and coordd's own (the watcher, the clerk, the minder) are not agents here.
pub fn agents_on(v: &TicketView) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        if !["owner", "watcher", "clerk", "minder", ""].contains(&n) && !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    };
    if let Some(o) = v.opener.as_deref() {
        add(o);
    }
    for q in &v.posts {
        add(&q.by);
        q.names.iter().chain(&q.told).chain(&q.inform).for_each(|n| add(n));
    }
    for g in &v.given {
        add(&g.to);
    }
    out
}
