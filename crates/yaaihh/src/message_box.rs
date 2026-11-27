//! The one message box (the owner, N-450.p4: "we need 1 single message box … reuse a single component"; paper-research
//! N-449.p2, N-460.p1): every place the owner sends words to agents draws this box, and only this one. It is the agent
//! page's composer made general: the same field, spelling, @mentions, comments on passages, keys (Cmd/Ctrl+Enter
//! sends) and Send. What differs by place is its target, said in one line above it (`→ Windows window › post 15`), and
//! the actions the target brings beside Send.
//!
//! The target (agreed on N-449 posts 5 and 6) says where the words go; the place's buttons only set it (a post's Reply
//! and Subthread), and × puts the place's default back. Each target is one request; nothing guesses where words go
//! once he has chosen. The words are kept per place, in `App::composers` under the place's key (an agent's name,
//! `thread:ID`, `project`), so a target changed keeps what he wrote, and spelling, mentions and the saved state work
//! the same in every place.

use crate::App;
use coord_types::Request;
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Menu, MenuItem, Placement, Popover};

/// Where the box's words go.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Target {
    /// An agent's page: to that agent, as the composer always did.
    Agent { project: usize, name: String },
    /// No thread open and no choice made: the clerk files it (intake).
    Intake { project: usize },
    /// Chosen: a top thread of its own, under nothing; no intake.
    NewThread { project: usize },
    /// On thread T (its id, `N-450`), at post N (None: the thread's foot), one of the ways.
    Thread { project: usize, t: String, post: Option<u32>, way: Way },
    /// His comment on a selected passage, being written (ask.rs `Picked`, one at a time): its Send, `Add comment`,
    /// sends it at once to the passage's place, quoting the passage, as that place's Send sends (ask.rs `send`); the
    /// box stands at the passage (ask.rs `layer`).
    Comment { project: usize },
    /// A reply in mail thread ID (a question's or a mail's, in an agent's chat), TO whom it answers: mail to its To
    /// row (ask.rs `reply_box`, `reply`). Its words are kept under the thread's id.
    Reply { project: usize, id: String, to: String },
    /// The why of ending thread T (its number N), in its end box (ending.rs): Send ends it.
    End { project: usize, n: u64, t: String },
    /// His answer to question ASK of post POST of thread T (its id), written in place under the question (the owner,
    /// N-1104 post 405; consult-lead N-1335 p3, 1): kept apart per question; Send sends the post's answers as one
    /// message, the thread's own box's words first (threads.rs `send_answers`).
    Answer { project: usize, t: String, post: u32, ask: usize },
}

/// The place of the box of question ASK of post POST of thread T, written in place under it.
pub fn answer_place(t: &str, post: u32, ask: usize) -> String {
    format!("answer:{t}.p{post}.{ask}")
}

/// The place of the comment box: one, as one comment is written at a time; no agent's name has a colon.
pub const COMMENT_PLACE: &str = "comment:";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Way {
    /// Reply here: a post under that post, or at the foot.
    Here,
    /// A new thread tied at that post from its first line.
    Subthread,
    /// The answer to his step S open on the thread; the step's results are the box's actions.
    Step { s: String },
}

impl Target {
    pub fn project(&self) -> usize {
        match self {
            Target::Agent { project, .. } | Target::Intake { project } | Target::NewThread { project } | Target::Thread { project, .. } | Target::Comment { project } | Target::Reply { project, .. } | Target::End { project, .. } | Target::Answer { project, .. } => *project,
        }
    }

    /// The same target in project I (state.rs: a project's place in the list may differ in the window that reads it).
    pub fn in_project(mut self, i: usize) -> Target {
        match &mut self {
            Target::Agent { project, .. } | Target::Intake { project } | Target::NewThread { project } | Target::Thread { project, .. } | Target::Comment { project } | Target::Reply { project, .. } | Target::End { project, .. } | Target::Answer { project, .. } => *project = i,
        }
        self
    }

    /// The place whose box this is: its words are kept under this key, whatever the target in it.
    pub fn place(&self) -> String {
        match self {
            Target::Agent { name, .. } => name.clone(),
            Target::Intake { .. } | Target::NewThread { .. } => "project".into(),
            Target::Thread { t, .. } => thread_place(t),
            Target::Comment { .. } => COMMENT_PLACE.into(),
            Target::Reply { id, .. } => id.clone(),
            Target::End { n, .. } => crate::ending::place(*n),
            Target::Answer { t, post, ask, .. } => answer_place(t, *post, *ask),
        }
    }
}

/// The place key of thread T (as `Target::place`): the key a comment on its posts goes with too (threads.rs `target`).
pub fn thread_place(t: &str) -> String {
    format!("thread:{t}")
}

/// Ticket T (its id) in PROJECT.
fn ticket<'a>(s: &'a App, project: usize, t: &str) -> Option<&'a docket_machine::TicketView> {
    s.projects.get(project).and_then(|p| p.live.tickets.values().find(|v| v.id == t))
}

/// What a target needs said of it: the ticket's label for the line.
fn ticket_label(s: &App, project: usize, t: &str) -> String {
    let Some(v) = ticket(s, project, t) else { return t.to_string() };
    let first = v.title.lines().next().unwrap_or("").trim();
    if first.is_empty() { v.id.clone() } else { first.to_string() }
}

/// The line above the box: where the words go, said once (N-446 post 4). Its second part is true when the words answer
/// something he owes (the accent, N-466 rule 1).
pub fn line(s: &App, target: &Target) -> (String, bool) {
    match target {
        // With you as threads: the thread his words go on, or a new one (withyou.rs)
        Target::Agent { project, name } => (crate::withyou::line(s, *project, name).0, false),
        Target::Intake { .. } => ("→ the project".into(), false),
        Target::NewThread { .. } => ("→ a new thread".into(), false),
        // the passage stands above its box instead (ask.rs `layer`)
        Target::Comment { .. } => (String::new(), false),
        // its To row says whom (ask.rs `to_row`)
        Target::Reply { .. } => (String::new(), false),
        // the end box says it above (ending.rs `panel`)
        Target::End { .. } => (String::new(), false),
        // the question it answers stands right above it (threads.rs `drawn`)
        Target::Answer { .. } => (String::new(), false),
        Target::Thread { project, t, post, way } => {
            // With you as threads: the label clipped (paper-research N-800.p4)
            let label = crate::streams::clip(&ticket_label(s, *project, t), 60);
            match (post, way) {
                // at the foot: the asks his post there would close, by the view (`would_close`, N-449.p6 change 2)
                (None, Way::Here) => match ticket(s, *project, t).and_then(|v| v.posts.iter().filter(|q| crate::threads::open_to_owner(v, q)).find_map(|q| closes(q, |w| &w.foot).map(|how| (q, how)))) {
                    Some((q, how)) => (format!("→ {label}; {how} {}'s {} in post {}", q.by, questions(q), q.n), true),
                    None => (format!("→ {label}"), false),
                },
                (Some(n), Way::Here) => match ticket(s, *project, t).and_then(|v| v.posts.iter().find(|q| q.n == *n).filter(|q| crate::threads::open_to_owner(v, q))) {
                    // his words answer what the post asks him: said, in the accent (N-449.p6)
                    Some(q) => (format!("→ {label} › answers {}'s {} in post {n}", q.by, questions(q)), true),
                    None => (format!("→ {label} › post {n}"), false),
                },
                (Some(n), Way::Subthread) => match ticket(s, *project, t).and_then(|v| v.posts.iter().find(|q| q.n == *n).filter(|q| crate::threads::open_to_owner(v, q))) {
                    // the subthread's first post goes to the asker: whether it closes the asks is the view's
                    // (`would_close`, N-447 p18), not worked out here
                    Some(q) => match closes(q, |w| &w.subthread) {
                        Some(how) => (format!("→ {label} › post {n}, as a subthread; {how} {}'s {} there", q.by, questions(q)), true),
                        None => (format!("→ {label} › post {n}, as a subthread"), false),
                    },
                    None => (format!("→ {label} › post {n}, as a subthread"), false),
                },
                (None, Way::Subthread) => (format!("→ {label}, as a subthread"), false),
                (_, Way::Step { s: step }) => (format!("→ {label} › answers {}", step_said(s, *project, t, step)), true),
            }
        }
    }
}

/// The line SAID above TARGET's box with whom TEXT's tags reach (0.2 B10): each agent by name, each group by its name
/// and count (`→ Runner rows › post 3, as a subthread + the team (19)`). A box that says no line says none for them
/// either. An agent's words with others tagged are a new thread; the project's box and a new thread go to whom they
/// tag, not to the clerk nor to whoever leads (consult-3, N-1402.p2, 2); a post on a thread whose coord reads no group
/// in it says nothing of them ([`request`] refuses a group there).
pub fn with_groups(s: &App, target: &Target, said: String, text: &str) -> String {
    let Some(p) = s.projects.get(target.project()) else { return said };
    let whom = crate::ask::whom(p, text);
    if said.is_empty() || whom.is_empty() {
        return said;
    }
    match target {
        Target::Thread { way: Way::Here, .. } if !crate::ask::post_takes_groups(p) => said,
        Target::Agent { name, .. } => {
            // others tagged make an agent's words a new thread (App::composed), not a post on the one it is on
            let others = crate::ask::tagged(p, &[text]).is_ok_and(|n| n.iter().any(|n| n != name));
            let besides: Vec<&String> = whom.iter().filter(|w| *w != name).collect();
            if others && !besides.is_empty() {
                let with = format!("{name} + {}", besides.iter().map(|w| w.as_str()).collect::<Vec<_>>().join(" + "));
                crate::look::say(s, "new_thread_with", &[("agent", &with)])
            } else {
                said
            }
        }
        Target::Intake { .. } => format!("→ {}", whom.join(" + ")),
        Target::NewThread { .. } => format!("→ a new thread with {}", whom.join(" + ")),
        _ => format!("{said} + {}", whom.join(" + ")),
    }
}

/// "question" or "questions", by how many post Q asks (a typed ask is one, threads.rs `asks_count`).
fn questions(q: &docket_machine::Post) -> &'static str {
    if crate::threads::asks_count(q) == 1 { "question" } else { "questions" }
}

/// Whether his post of one way (WAY of the view's `would_close` on Q) closes Q's asks, said: "answers", "may answer"
/// (only if the routing takes it to the asker); none when it does not, or the view does not say.
fn closes(q: &docket_machine::Post, way: impl Fn(&docket_machine::line::WouldClose) -> &docket_machine::line::Closes) -> Option<&'static str> {
    match q.would_close.as_ref().map(way)? {
        docket_machine::line::Closes::Yes => Some("answers"),
        docket_machine::line::Closes::Routed => Some("may answer"),
        docket_machine::line::Closes::No => None,
    }
}

/// His step STEP (a ticket's id) on thread T, as the line names it: a stop as whether it runs again, else the step's
/// label with its answers ("your step: Go or Hold").
fn step_said(s: &App, project: usize, t: &str, step: &str) -> String {
    let Some(h) = ticket(s, project, step) else { return format!("your step on {step}") };
    let docket_machine::ViewStatus::Awaits { call, head, .. } = &h.status else { return format!("your step on {step}") };
    // a stop: whether it runs again, not the hidden ticket's results
    if call == "stopped" {
        return "whether it runs again".to_string();
    }
    let what = if h.id == t {
        "your step".to_string()
    } else {
        s.projects.get(project).map_or_else(|| h.id.clone(), |p| crate::refs::ticket(Some(p), h).label)
    };
    match head {
        docket_machine::Type::Variant(cs) if !cs.is_empty() => format!("{what}: {}", cs.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(" or ")),
        _ => what,
    }
}

/// Whether project PI's coord takes the requests of threads piece 1 (`thread`, `under`, end, take-back): its hello
/// says ASKS_CLOSED_PROTOCOL or more; else why not, said under the box.
pub fn threads_taken(s: &App, pi: usize, what: &str) -> Result<(), String> {
    let at = s.projects.get(pi).map_or(0, |p| p.protocol);
    if at >= coord_types::ASKS_CLOSED_PROTOCOL {
        Ok(())
    } else {
        Err(format!("not sent: {what} needs a newer engine ({} > {at}); Reply here", coord_types::ASKS_CLOSED_PROTOCOL))
    }
}

/// What the place's box holds now.
pub fn written<'a>(s: &'a App, target: &Target) -> Option<&'a TextEdit> {
    s.composers.get(&(target.project(), target.place()))
}

/// The target chosen in the place of DEFAULT, else DEFAULT itself.
pub fn chosen(s: &App, default: Target) -> Target {
    s.box_targets.get(&(default.project(), default.place())).cloned().unwrap_or(default)
}

/// Set the target of its place: what a post's Reply and Subthread call (docket-edge4's buttons, N-449).
#[allow(dead_code)]
pub fn choose(s: &mut App, target: Target) {
    s.box_targets.insert((target.project(), target.place()), target);
}

/// What the box's actions beside Send are, by place: built by the caller (an agent's Esc; a post's
/// "nothing to answer"; a step's results).
pub struct Actions {
    /// Left of the row: a state switch, a line of the place's own.
    pub left: Option<El>,
    /// Right, before Send.
    pub right: Vec<El>,
    /// Send's label and whether it is drawn ("Send N answers" in place of Send, or no Send where results answer).
    pub send: Option<String>,
    pub placeholder: String,
}

/// The words of the one box on a thread's own page, whichever page draws it (a thread his hello opens, threads.rs
/// `foot_composer`; a page of another kind, ticket.rs `post_box`): one placeholder, one button word, Send (paper-research
/// N-1446 p3240; the words p3207, 6).
pub(crate) const THREAD_PLACEHOLDER: &str = "Write to this thread (@NAME reaches NAME now)";

/// The one box's actions on a thread's own page: Send, and [`THREAD_PLACEHOLDER`].
pub(crate) fn thread_actions() -> Actions {
    Actions { left: None, right: Vec::new(), send: Some("Send".into()), placeholder: THREAD_PLACEHOLDER.into() }
}

/// The height of the spelling row under the box: one regular button's.

/// The box for TARGET, its default DEFAULT (the × puts it back), with ACTIONS.
pub fn build(s: &App, target: &Target, default: &Target, actions: Actions, k: &Kit) -> El {
    let (pi, place) = (target.project(), target.place());
    let key = (pi, place.clone());
    // drawn: his writing in it is on only while it is (lib.rs `writing_changed`)
    s.boxes_drawn.borrow_mut().insert(key.clone());
    let empty = TextEdit::new("");
    let edit = s.composers.get(&key).unwrap_or(&empty);
    let field_id = match target {
        Target::Reply { id, .. } => crate::ask::Field::Reply(pi, id.clone()),
        _ => crate::ask::Field::Composer(pi, place.clone()),
    };
    let (f1, f2) = (field_id.clone(), field_id.clone());
    let (ek, sk, ck) = (key.clone(), target.clone(), key.clone());
    let (spoken, keyed) = (key.clone(), key.clone());
    let split_at = target.clone();
    let esc = target.clone();
    let mut marks = if s.spell.on && crate::spell::available() { crate::spell::marked_box(s, &key, edit) } else { Vec::new() };
    // his words changed (by the cleaning or his choice in a card): the dotted mark and `Undo` (N-487.p90)
    let changes = crate::spell::changed_at(s, &key, edit);
    // the ghost at the caret (suggest.rs, 0.2 line 27): a `Complete` drawn by rune right after the caret; a `Fix` under
    // its word (`card_layer`), which stands under the word's highlight, also for a word nothing marks (the cleaning's)
    let ghost = crate::suggest::ghost(s, &key);
    if let Some(g) = ghost.as_ref().filter(|g| g.kind == crate::suggest::Kind::Fix)
        && !marks.contains(&g.range)
        && !changes.iter().any(|(r, _)| *r == g.range)
    {
        marks.push(g.range.clone());
    }
    let (ck2, f2k) = (key.clone(), f2);
    let field = field(
        edit,
        &actions.placeholder,
        &marks.iter().map(|r| (r.clone(), crate::spell::mark_key(&key, r))).chain(changes.iter().map(|(r, _)| (r.clone(), crate::spell::changed_key(&key, r)))).collect::<Vec<_>>(),
        // the point whose chip the pointer rests on, tinted (split.rs); no line of its own, so spelling's and changed
        // words' marks are the only lines under his text
        [crate::split::spans(s, &key, crate::split::tint(k)), crate::spell::box_spans(s, &key, edit, k.c.danger.fg)].concat(),
        k,
    )
    .ghost(ghost.as_ref().map_or(String::new(), |g| g.rest().to_string()), k.c.text_faint)
    .on_edit(move |s: &mut App| s.composers.entry(ek.clone()).or_insert_with(|| TextEdit::new("")))
    .on_input_change(move |s: &mut App, _| {
        crate::spell::changed(s, &ck);
        // the word changed under its card: the card goes (N-487.p88)
        if let (Some(c), Some(e)) = (s.spell.card.clone(), s.composers.get(&ck)) && c.key == ck && e.text.get(c.range.clone()) != Some(c.word.as_str()) {
            crate::spell::close_card(s);
        }
        crate::ask::follow_mention(s, &f1);
        // his writing there, to an agent or on a thread (`writing_key`)
        if let crate::ask::Field::Composer(pi, place) = &f1 {
            s.writing_changed(*pi, place);
        }
        crate::speech_box::changed(s, &spoken);
        crate::split::changed(s, &split_at);
    })
    // Ctrl+Shift+Space: the mic (speech_box.rs)
    .on_input_key(move |s: &mut App, ev, tx| crate::speech_box::key(s, &keyed, ev) || crate::spell::card_key(s, &ck2, ev, tx) || crate::ask::mention_key(s, &f2k, ev) || crate::suggest::key(s, &ck2, ev) || crate::threads::answer_key(s, &esc, ev) || crate::ask::comment_key(s, &esc, ev) || crate::outgoing::esc_key(s, &esc, ev))
    .on_submit(move |s: &mut App, _| send(s, &sk));
    let field = field_shell(k, field, edit.focused);
    // a marked word the pointer rests on, for the system's hover time: its card opens under it (paper-research
    // N-487.p88). Each mark has a region over it, placed from the word's highlight in the box; the region takes no
    // press, so a click still puts the caret in the word.
    let fk = field_id.key();
    let origin = rune::memo::layout(&fk);
    let faint = k.c.text_muted;
    let regions: Vec<El> = marks
        .iter()
        .map(|r| (r.clone(), crate::spell::mark_key(&key, r), false))
        .chain(changes.iter().map(|(r, _)| (r.clone(), crate::spell::changed_key(&key, r), true)))
        .filter_map(|(r, hk, changed)| {
            let (o, w) = (origin?, rune::memo::layout(&hk)?);
            let card = crate::spell::Card { key: key.clone(), range: r.clone(), word: edit.text.get(r.clone())?.to_string(), keys: false };
            let mut region = rune::el()
                .key(format!("{hk}-at"))
                .absolute(w[0] - o[0], w[1] - o[1])
                .w(w[2])
                .h(w[3])
                .hover_delays(crate::spell::hover_time(), std::time::Duration::from_millis(300))
                .on_hover_change(move |s: &mut App, inside, _| crate::spell::word_hover(s, card.clone(), inside));
            // the changed mark: the thin line under the word, dotted (rune draws no dotted underline)
            if changed {
                let n = ((w[2] / 3.0) as usize).max(1);
                region = region.child(row().absolute(0.0, w[3] - 1.5).gap(1.5).children((0..n).map(|_| rune::el().w(1.5).h(1.0).fill(faint))));
            }
            Some(region)
        })
        .collect();
    let field = col().key(fk).w_full().child(field).children(regions);
    // the line: where the words go; × when a choice of his replaced the place's default
    let (said, owed) = line(s, target);
    // on the thread's own page a line that only names that thread says nothing his title does not (paper-research
    // N-1446 p3207, 6): drawn only when it says more (whom his tags reach, what his words answer)
    let names_only = matches!(target, Target::Thread { post: None, way: Way::Here, .. }) && !owed;
    let base = said.clone();
    let said = with_groups(s, target, said, &edit.text);
    let said = if names_only && said == base { String::new() } else { said };
    let shows_line = !said.is_empty();
    let mut head = row().key(format!("box-line-{pi}-{place}")).w_full().gap(6.0).center_y().child(text(said).font(k.ty.small).color(if owed { k.c.text_accent } else { k.c.text_muted }));
    // With you as threads: × on the thread his words go on makes them a new thread
    if let Target::Agent { project, name } = target
        && crate::withyou::line(s, *project, name).1
    {
        let (pi, who) = (*project, name.clone());
        head = head.child(Button::new("×").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::withyou::unbind(s, pi, &who)).build(k));
    }
    if target != default {
        let back = default.clone();
        head = head.child(Button::new("×").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
            s.box_targets.remove(&(back.project(), back.place()));
        }).build(k));
    }
    // Spelling and the box's other switches are the items of the menu behind "⋯", over the chat, so nothing under the
    // pointer moves (consult-2, N-815.p23; paper-research p24): in sight only the mic and Send (docket-edge6's rules
    // page, N-815.p10; the owner, 11:13: "only whati need t see direclty")
    let dots = more(s, target, k).build(k, &mut Vec::new());
    let mut buttons = row().gap(8.0).center_y().children(actions.right).child(dots).children(crate::speech_box::button(s, &key, k));
    if let Some(label) = actions.send {
        let to = target.clone();
        // Schedule, smaller, right before Send: his words held instead of sent (schedule.rs)
        buttons = buttons.children(crate::schedule::button(s, target, k)).child(Button::new(label).variant(ButtonVariant::Contrast).on_press(move |s: &mut App| send(s, &to)).build(k));
    }
    let left = actions.left.unwrap_or_else(|| row());
    // what comes and goes in it (the candidates, the × on the line) appears where it stands: each part keyed, so a
    // row that comes is not taken for the row it pushes down and slid from there, and its changes land at once (the
    // owner, N-481.p1: they "come in from the end of the screen")
    col()
        .key(format!("composer-{pi}-{place}"))
        .instant_changes()
        .w_full()
        .gap(8.0)
        // a cleaning's look; his messages on their way stand above the box's frame (outgoing.rs `bubbles`, drawn by
        // each place that holds a box he sends from)
        .children(crate::words::shown(s, pi, &place, k))
        // his words held from this box, waking no one until they go (schedule.rs)
        .children(crate::schedule::cards(s, pi, &place, k))
        .children(shows_line.then_some(head))
        // the comments held for its place, which its Send takes with his words (ask.rs `Notes::held`)
        .children(crate::ask::held_lines(s, pi, &place, k))
        .child(field)
        .children(crate::split::chips(s, target, k))
        // speech's line, only for the model's offer, its fetch, or why there was no take (speech_box.rs)
        .children(crate::speech_box::line(s, &key, k).map(|l| l.key(format!("speech-line-{pi}-{place}")).w_full().gap(4.0).center_y()))
        .child(row().key(format!("composer-actions-{pi}-{place}")).w_full().gaps(8.0, 16.0).flex_wrap().justify_between().center_y().child(left).child(buttons))
        .children(crate::schedule::typing(s, target, k))
}

/// What stands open over a box: its "⋯" menu, or the line of its "?". In the window's layer, over the chat, so nothing
/// in the chat moves when it opens (consult-2, N-815.p23; paper-research p24).
#[derive(Clone, Debug, PartialEq)]
pub enum Open {
    More,
    Help(String),
}

fn opened<'a>(s: &'a App, target: &Target) -> Option<&'a Open> {
    s.box_open.as_ref().filter(|(t, _)| t == target).map(|(_, o)| o)
}

/// The box's "⋯": Spelling and the box's other switches as the items of a menu, the top bar's kind (zoom.rs).
fn more(s: &App, target: &Target, k: &Kit) -> Menu {
    let (pi, place) = (target.project(), target.place());
    let to = target.clone();
    let trigger = Button::new("⋯")
        .variant(ButtonVariant::Ghost)
        .on_press(move |s: &mut App| {
            let shown = s.box_open.as_ref().is_some_and(|(t, o)| *t == to && *o == Open::More);
            s.box_open = (!shown).then(|| (to.clone(), Open::More));
        })
        .build(k)
        .help("Spelling and the box's other settings");
    let mut m = Menu::new(format!("box-menu-{pi}-{place}")).trigger(trigger).open(opened(s, target) == Some(&Open::More)).placement(Placement::TopEnd).on_close(|s: &mut App| s.box_open = None);
    let avail = crate::spell::available();
    m = m.item(
        MenuItem::new(if avail { "Spelling" } else { "Spelling (no dictionary on this machine)" })
            .checked(s.spell.on && avail)
            // the keyboard's way to a marked word's candidates, said where he looks for spelling (consult-4, N-584.p11)
            .description("Hover an underlined word for suggestions")
            .on_select(|s: &mut App| {
                s.spell.toggle();
                s.box_open = None;
            }),
    );
    // the same marks over other programs' boxes, off until he turns it on (desk_marks.rs): in the tray's menu, here
    // only when there is no tray icon
    if crate::desk_marks::supported() && !crate::tray::shown() {
        m = m.item(MenuItem::new("Spelling marks in other programs").checked(s.desk.on).on_select(|s: &mut App| {
            crate::desk_marks::toggle(s);
            s.box_open = None;
        }));
    }
    // his messages split by point (split.rs): in the tray's menu, here only when there is no tray icon (N-788)
    if crate::split::switch_here(s, target) {
        m = m.item(MenuItem::new(crate::split::BOX_LABEL).checked(s.split.on).description("Show points for other threads").on_select(|s: &mut App| {
            s.split.toggle();
            s.box_open = None;
        }));
    }
    // no "When done": his message to a working agent waits for its turn's end, as every post to it does (the owner,
    // N-1104 p716 "only the turn end"; coordd's delivery.kind, a2ab7686; paper-research N-1446 p2112, 1). Esc stays
    // its own action at the row's left end, and nothing joins a stop to a message
    m
}

/// "?" at the box's end, for TARGET: LINE on hover, and on a press too (consult-2, N-815.p23).
pub fn help(s: &App, target: &Target, line: &str, k: &Kit) -> El {
    help_popover(s, target, line, k).build(k, &mut Vec::new())
}

fn help_popover(s: &App, target: &Target, line: &str, k: &Kit) -> Popover {
    let (pi, place) = (target.project(), target.place());
    let (to, l) = (target.clone(), line.to_string());
    let trigger = Button::new("?")
        .variant(ButtonVariant::Ghost)
        .on_press(move |s: &mut App| {
            let shown = s.box_open.as_ref().is_some_and(|(t, o)| *t == to && matches!(o, Open::Help(_)));
            s.box_open = (!shown).then(|| (to.clone(), Open::Help(l.clone())));
        })
        .build(k)
        .help(line.to_string());
    Popover::new(format!("box-help-{pi}-{place}"))
        .trigger(trigger)
        .open(matches!(opened(s, target), Some(Open::Help(_))))
        .placement(Placement::TopEnd)
        .label(line.to_string())
        .padding(8.0)
        .child(text(line.to_string()).font(k.ty.small).color(k.c.text_base))
        .on_close(|s: &mut App| s.box_open = None)
}

/// The open "⋯" menu or "?" line of a box, or its Schedule menu, into LAYER.
pub fn open_layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    // Schedule's menu, open over its box (schedule.rs)
    if let Some(target) = s.schedule.menu.clone() {
        crate::schedule::menu(s, &target, k).build(k, layer);
    }
    let Some((target, open)) = s.box_open.clone() else { return };
    match open {
        Open::More => {
            more(s, &target, k).build(k, layer);
        }
        Open::Help(line) => {
            help_popover(s, &target, &line, k).build(k, layer);
        }
    }
}

/// The box's text field: rune-kit's TextArea as it draws (text-input.rs, auto-grown 2 to 10 rows), built here so each
/// marked word (MARKS: its range and key) carries a keyed highlight, the place its card is read
/// from. Rune-kit's TextArea takes no highlights.
fn field(edit: &TextEdit, placeholder: &str, marks: &[(std::ops::Range<usize>, String)], spans: Vec<rune::Span>, k: &Kit) -> El {
    let c = &k.c;
    let style = rune::TextStyle { line_height: (13.0f32 * 1.35 * 2.0).round() / 2.0, ..k.ty.ui };
    let rich = rune::RichStyle { link: c.text_accent, code_fg: None, code_bg: c.layer_03, code_block_bg: c.layer_02, quote_bar: c.border_strong, marker: Some(c.text_muted) };
    let mut f = rune::text_input(edit, style).key("input").w_full().color(c.text_base).rich_style(rich).placeholder(placeholder.to_string(), c.text_faint).caret(c.text_base, k.selection());
    if !placeholder.is_empty() {
        f = f.label(placeholder.to_string());
    }
    f = f.spans(spans);
    for (r, key) in marks {
        f = f.text_highlight(rune::TextHighlight::new(r.clone(), rune::Color::TRANSPARENT).key(key.clone()));
    }
    f
}

/// The shell of rune-kit's text fields around FIELD: the base fill, the input shadow and hover, the focus ring.
fn field_shell(k: &Kit, field: El, focused: bool) -> El {
    let c = &k.c;
    let mut e = col().role("textbox").padding(8.0).w_full().child(rune::auto_grow(field, 2, 10)).radius(k.radius.md).fill(c.bg_base).token_motion(k.motion.control);
    if focused {
        e = e.outline(1.0, -1.0, c.border_focus);
    } else {
        for sh in k.shadow_layers(k.elevation.input()) {
            e = e.box_shadow(sh);
        }
        e = e.hover(c.hover);
    }
    e
}

/// What stands under a word of a box, into LAYER (paper-research N-815.p13, 1: "when i hover the word ghost under it
/// … it should show the suggestion"):
/// - a marked word the pointer rests on: its best candidate as a ghost (a press puts it in), and ⋯ for its whole card;
/// - a changed word the pointer rests on, or with the caret in it after a click: his own word as a ghost (a press puts
///   it back; the owner, N-858.p3: "clicking on the changed word … shows the original text");
/// - the whole card of a marked word, after that ⋯ or Ctrl+. (N-487.p88): the word struck through and the best
///   candidate (a click puts it in), up to four more dim, and `Add to dictionary`.
///
/// Each stands under the word at once, without motion, and never takes the keyboard (unless opened with Ctrl+.).
pub fn card_layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(c) = s.spell.card.clone().or_else(|| crate::spell::caret_card(s)) else { return };
    let Some(e) = s.composers.get(&c.key) else { return };
    if e.text.get(c.range.clone()) != Some(c.word.as_str()) {
        return;
    }
    // a changed word: his word under it, quietly (N-487.p90, 3: the way back to what he typed)
    if let Some((_, his)) = crate::spell::changed_at(s, &c.key, e).into_iter().find(|(r, _)| *r == c.range) {
        let at = format!("{}-at", crate::spell::changed_key(&c.key, &c.range));
        // the caret's: only where the word is drawn now
        if rune::memo::layout(&at).is_none() {
            return;
        }
        let (to, r, h) = (c.key.clone(), c.range.clone(), his.clone());
        let back = ghost_word(&his, move |s: &mut App| crate::spell::put_back(s, &to, r.clone(), &h), k).help("Put back what you typed");
        layer.push(under(keyed(ghost_row(&c, k).child(back), &c), at));
        return;
    }
    // the caret's ghost on this word (suggest.rs `Fix`, 0.2 line 27): the word under it is the one Tab puts in, also
    // when it is the cleaning's
    let fix = crate::suggest::ghost(s, &c.key).filter(|g| g.kind == crate::suggest::Kind::Fix && g.range == c.range);
    let mut cands = crate::spell::candidates_at(s, c.key.0, e, &c.range);
    if let Some(g) = &fix {
        cands.retain(|w| *w != g.with);
        cands.insert(0, g.with.clone());
    }
    let at = format!("{}-at", crate::spell::mark_key(&c.key, &c.range));
    if !c.keys && !s.spell.whole {
        let mut body = ghost_row(&c, k);
        body = match cands.first() {
            Some(best) => {
                let (to, r, w) = (c.key.clone(), c.range.clone(), best.clone());
                let word = ghost_word(best, move |s: &mut App| crate::spell::choose(s, &to, r.clone(), &w), k);
                // shown by the caret: a click on it leaves the box the keyboard, which the ghost stands for
                body.child(if fix.is_some() { word.focusable(false).help("Tab") } else { word })
            }
            None => body.child(text("no word near it").font(k.ty.small).color(k.c.text_faint).px(8.0)),
        };
        if fix.is_some() {
            body = body.preserve_focus_on_press(true);
        }
        // the rest of its card one press away: the other candidates and `Add to dictionary`; opened from the caret's
        // ghost, the card stays as a card the pointer opened (the box may lose the keyboard to it)
        let opened = c.clone();
        let whole = Button::new("⋯")
            .variant(ButtonVariant::Ghost)
            .on_press(move |s: &mut App| {
                s.spell.whole = true;
                s.spell.card.get_or_insert_with(|| opened.clone());
            })
            .build(k)
            .help("Other words, Add to dictionary");
        layer.push(under(body.child(whole), at));
        return;
    }
    let (best, more) = crate::spell::card_rows(&cands);
    let pick = |w: &String, dim: bool| {
        let (to, r, w, w_label) = (c.key.clone(), c.range.clone(), w.clone(), w.clone());
        let b = Button::new(w.clone()).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::spell::choose(s, &to, r.clone(), &w));
        if dim { dimmed(b, w_label, k) } else { b }.build(k)
    };
    // no candidate: the word unstruck and a faint "no word near it", as the card over other programs (consult-4,
    // N-584.p16: a struck word with nothing beside it reads as "delete this")
    let word = text(c.word.clone()).font(k.ty.ui).color(k.c.text_muted);
    let mut first = row().gap(4.0).center_y();
    first = match best {
        Some(b) => first.child(word.strike(0, c.word.len(), Some(k.c.text_muted)).pl(8.0).pr(4.0)).child(pick(b, false)),
        None => first.child(word.pl(8.0).pr(4.0)).child(text("no word near it").font(k.ty.small).color(k.c.text_faint)),
    };
    let w = c.word.clone();
    let mut rest = row().gap(2.0).center_y().children(more.iter().map(|m| pick(m, true)));
    // `Add to dictionary` set apart from the candidates, so it does not read as one more (consult-4, N-584.p11)
    if !more.is_empty() {
        rest = rest.child(text("·").font(k.ty.small).color(k.c.text_faint).px(2.0));
    }
    rest = rest.child(dimmed(Button::new("Add to dictionary").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| crate::spell::keep(s, &w)), "Add to dictionary".into(), k).build(k));
    let mut body = col().key(format!("spell-card-{}-{}", c.key.0, c.key.1)).padding(4.0).gap(2.0).child(first).child(rest).on_hover_change(|s: &mut App, inside, _| crate::spell::card_hover(s, inside));
    body = keyed(body, &c);
    layer.push(rune_kit::popover::raised(k, body, k.radius.md).anchored(at, rune::Side::Bottom, rune::Align::Start, 2.0));
}

/// The row a ghost stands in for card C: on the box's own fill, no shadow or border, so it reads as the box's text and
/// not as a card; the pointer on it keeps it.
fn ghost_row(c: &crate::spell::Card, k: &Kit) -> El {
    row()
        .key(format!("spell-card-{}-{}", c.key.0, c.key.1))
        .gap(2.0)
        .center_y()
        .fill(k.c.bg_base)
        .radius(k.radius.sm)
        .on_hover_change(|s: &mut App, inside, _| crate::spell::card_hover(s, inside))
}

/// BODY right under the word whose region is AT.
fn under(body: El, at: String) -> El {
    body.anchored(at, rune::Side::Bottom, rune::Align::Start, 0.0)
}

/// WORD offered quietly under the word it is for: the box's text size in the faint colour; a press runs ON.
fn ghost_word(word: &str, on: impl Fn(&mut App) + 'static, k: &Kit) -> El {
    let (label, faint) = (word.to_string(), k.c.text_faint);
    Button::new(word.to_string()).variant(ButtonVariant::Ghost).on_press(on).label_with(move |style, _| text(label).font(style).color(faint)).build(k)
}

/// BODY of card C: opened from the keyboard (Ctrl+.), it has the focus, and Escape or a click outside closes it and
/// gives the box the keyboard back (consult-4, N-584.p11; for a changed word's card too, consult-2 N-584.p33).
fn keyed(body: El, c: &crate::spell::Card) -> El {
    if !c.keys {
        return body;
    }
    let to = c.key.clone();
    body.modal_tx(true, move |s: &mut App, _| {
        crate::spell::close_card(s);
        if let Some(e) = s.composers.get_mut(&to) {
            e.focused = true;
        }
    })
}

/// B, whose label is LABEL, with the label in the muted text colour (the card's other candidates, N-487.p88).
fn dimmed(b: Button, label: String, k: &Kit) -> Button {
    let muted = k.c.text_muted;
    b.label_with(move |style, _| text(label).font(style).color(muted))
}

/// Whether PLACE is an agent's (the others are `thread:#N` and `project`): an agent is told he is writing to it.
pub fn is_agent_place(place: &str) -> bool {
    !place.starts_with("thread:") && !place.starts_with("end:") && !place.starts_with("answer:") && place != "project" && place != COMMENT_PLACE
}

/// The thread whose box PLACE is: its own (`thread:ID`), or the box under one of its questions (`answer:ID.pN.K`),
/// whose words go with its own (threads.rs `send_answers`).
pub fn thread_of(place: &str) -> Option<&str> {
    place.strip_prefix("thread:").or_else(|| place.strip_prefix("answer:").and_then(|r| r.split_once(".p")).map(|(t, _)| t))
}

/// Where his writing in the box of PLACE is said, as `App::writing` keeps it: an agent's chat to the agent (its
/// name), a thread's boxes on the thread (`thread:ID`, proto-lead N-1446 p325); none for the other boxes. The comment
/// box (`COMMENT_PLACE`) is said where its comment goes, which its place alone does not tell (`App::writing_at`).
pub fn writing_key(place: &str) -> Option<String> {
    match thread_of(place) {
        Some(t) => Some(thread_place(t)),
        None => is_agent_place(place).then(|| place.to_string()),
    }
}

/// Send what the box of TARGET holds there, as the one request its target makes.
pub fn send(s: &mut App, target: &Target) {
    if let Target::Agent { project, name } = target {
        let (pi, name) = (*project, name.clone());
        s.send(pi, &name, false);
        return;
    }
    if let Target::Comment { .. } = target {
        crate::ask::send(s);
        return;
    }
    if let Target::Reply { id, to, .. } = target {
        crate::ask::reply(s, id, to);
        return;
    }
    if let Target::End { project, n, t } = target {
        crate::ending::send(s, *project, *n, t);
        return;
    }
    // his answers written in place under a post's questions: one message with the thread's box's words (threads.rs)
    match target {
        Target::Answer { project, t, post, .. } => {
            crate::threads::send_answers(s, *project, t, *post);
            return;
        }
        Target::Thread { project, t, post: Some(n), way: Way::Here } if crate::threads::has_answers(s, *project, t, *n) => {
            crate::threads::send_answers(s, *project, t, *n);
            return;
        }
        _ => {}
    }
    // as his chips say, when a proposal fits the box's text (split.rs); else one message, as ever. With comments held
    // for it (ask.rs `Notes::held`) one message, they and the box's words together
    if held_for(s, target).is_empty() && crate::split::send(s, target) {
        return;
    }
    match request(s, target) {
        Ok(req) => {
            sent(s, target, req);
        }
        Err(e) if e.is_empty() => {}
        Err(e) => {
            s.box_said.insert((target.project(), target.place()), Err(e));
        }
    }
}

/// The request the box of TARGET makes with what it holds; Err("") when it holds nothing, else Err(why it cannot go).
pub fn request(s: &App, target: &Target) -> Result<Request, String> {
    let pi = target.project();
    match target {
        Target::Agent { .. } => Err("an agent's box sends through its chat".into()),
        Target::Comment { .. } => Err("a comment goes from its box's Add comment, quoting its passage (ask.rs `send`)".into()),
        Target::Reply { .. } => Err("a reply goes as mail to its To row".into()),
        Target::End { .. } => Err("ending a thread is not a message".into()),
        Target::Answer { .. } => Err("an answer goes with its post's answers (threads.rs `send_answers`)".into()),
        Target::Intake { .. } => {
            let text = taken(s, target).ok_or_else(String::new)?;
            // to each agent tagged and each running agent of a group tagged, the first as its `to` (0.2 B10): not the
            // clerk, nor whoever leads unless tagged (consult-3, N-1402.p2, 2); none tagged: the clerk files it
            let (to, names) = addressed(None, tags(s, pi, &text, None)?);
            Ok(Request::Say { to, names, text, follows: None, wait: false, under: None, label: None, thread: false, typed: None, split_from: None, comments: Vec::new() })
        }
        Target::Thread { t, post: None, way: Way::Here, .. } => {
            // his comments held for the thread, each with its quote, then the box's words: one post, as a comment
            // goes (threads.rs `reply_with`)
            let held = held_for(s, target);
            if !held.is_empty() {
                return crate::threads::reply_with(s, pi, t, held, &taken(s, target).unwrap_or_default());
            }
            let text = taken(s, target).ok_or_else(String::new)?;
            no_group(s, pi, &text)?;
            // a note is answered as a message on it (views.rs, as its box did); a thread or ticket by a post
            Ok(if ticket(s, pi, t).is_some_and(|v| v.kind == "note") {
                Request::Msg { kind: None, to: Vec::new(), body: text, esc: false, soon: false, wait: false, as_host: None, re: Some(t.clone()), quotes: Vec::new(), origin: None, under: None, label: None, comments: Vec::new() }
            } else {
                Request::Post { kind: None, t: t.clone(), text, to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() }
            })
        }
        Target::Thread { t, post: Some(n), way: Way::Here, .. } => {
            let v = ticket(s, pi, t).ok_or_else(|| format!("not sent: {t} is not in this project"))?;
            let q = v.posts.iter().find(|q| q.n == *n).ok_or_else(|| format!("not sent: {t} has no post {n}"))?;
            let typed = written(s, target).map(|e| e.text.clone()).unwrap_or_default();
            no_group(s, pi, &typed)?;
            if crate::threads::open_to_owner(v, q) {
                // the answers to what it asks: one post to the asker that records the post it answers (design §16, 3)
                let asks = crate::threads::ask_texts(&s.tickets, q);
                // his answers written in place under its questions go with the box's words, as Send sends them
                // (threads.rs `send_answers`)
                let typed = crate::threads::with_answers(s, pi, &v.id, *n, &asks, &typed);
                let (text, _) = crate::threads::answers_from_box(&asks, &typed);
                let held = held_for(s, target);
                for c in held {
                    no_group(s, pi, &c.note)?;
                }
                // the comments held for the thread go with his answers, quoted first; with no answer, as a reply at
                // the post (below)
                if !text.is_empty() {
                    let text = if held.is_empty() { text } else { crate::ask::compose(held, &text).0 };
                    return Ok(Request::Post { kind: None, t: v.id.clone(), text, to: Some(q.by.clone()), answers: Some(*n), wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: held_data(s, pi, target) });
                }
                if held.is_empty() {
                    return Err(String::new());
                }
            }
            // a reply at post N: to its author, under it (docket-core4's form may carry "under N" itself, N-447 p13),
            // after the comments held for the thread
            for c in held_for(s, target) {
                no_group(s, pi, &c.note)?;
            }
            let text = with_held(s, target).ok_or_else(String::new)?;
            let to = Some(q.by.clone()).filter(|b| b != "owner");
            Ok(Request::Post { kind: None, t: v.id.clone(), text, to, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: held_data(s, pi, target) })
        }
        // a top thread of its own, nothing routes it (N-447 p35)
        Target::NewThread { .. } => {
            threads_taken(s, pi, "a new thread")?;
            let text = taken(s, target).ok_or_else(String::new)?;
            let (to, names) = addressed(None, tags(s, pi, &text, None)?);
            Ok(Request::Say { to, names, text, follows: None, wait: false, under: None, label: None, thread: true, typed: None, split_from: None, comments: Vec::new() })
        }
        // a thread tied at post N (none: the thread's last post), its first post to that post's author
        Target::Thread { t, post, way: Way::Subthread, .. } => {
            threads_taken(s, pi, "a subthread")?;
            let v = ticket(s, pi, t).ok_or_else(|| format!("not sent: {t} is not in this project"))?;
            let text = with_held(s, target).ok_or_else(String::new)?;
            // whom it tags: his words and his comments' notes, not the passages they quote
            let typed = std::iter::once(taken(s, target).unwrap_or_default()).chain(held_for(s, target).iter().map(|c| c.note.clone())).collect::<Vec<_>>().join("\n");
            let (under, to) = match post {
                Some(n) => {
                    let q = v.posts.iter().find(|q| q.n == *n).ok_or_else(|| format!("not sent: {t} has no post {n}"))?;
                    (format!("{}.p{n}", v.id), Some(q.by.clone()).filter(|b| b != "owner"))
                }
                None => (v.id.clone(), None),
            };
            let names = tags(s, pi, &typed, to.as_deref())?;
            let (to, names) = addressed(to, names);
            Ok(Request::Say { to, names, text, follows: None, wait: false, under: Some(under), label: None, thread: false, typed: None, split_from: None, comments: held_data(s, pi, target) })
        }
        // a step is answered by one of its results, beside the box: Send is not one
        Target::Thread { t, way: Way::Step { s: step }, .. } => Err(format!("not sent: pick an answer ({})", step_said(s, pi, t, step))),
    }
}

/// The agents TEXT tags in project PI (ask.rs `tagged`: each `@NAME`, each running agent of a `@team` or `@PART`), TO
/// left out: each one more recipient of the box's Say. Err when a tag names nobody.
fn tags(s: &App, pi: usize, text: &str, to: Option<&str>) -> Result<Vec<String>, String> {
    let p = s.projects.get(pi).ok_or_else(|| "not sent: no project".to_string())?;
    Ok(crate::ask::tagged(p, &[text])?.into_iter().filter(|n| Some(n.as_str()) != to).collect())
}

/// TO, else the first of NAMES, with the rest: a Say to whom it tags reaches them, not whoever leads.
fn addressed(to: Option<String>, mut names: Vec<String>) -> (Option<String>, Vec<String>) {
    match to {
        Some(_) => (to, names),
        None if names.is_empty() => (None, names),
        None => {
            let first = names.remove(0);
            (Some(first), names)
        }
    }
}

/// Err when TEXT tags a group and goes as a post on a thread whose coord reads no group there (`GROUPS_PROTOCOL`): the
/// group would be words and reach nobody. From that protocol coordd reads it by the same rule (`coord_types::groups`).
fn no_group(s: &App, pi: usize, text: &str) -> Result<(), String> {
    match s.projects.get(pi) {
        Some(p) if crate::ask::group_refused(p, &[text]) => Err(crate::ask::NO_GROUP_ON_POST.into()),
        _ => Ok(()),
    }
}

/// The words the box of TARGET holds, trimmed; none when it holds none.
fn taken(s: &App, target: &Target) -> Option<String> {
    let t = written(s, target)?.text.trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// The comments held for TARGET's place that its Send takes with its words (ask.rs `Notes::held`): a thread's box's
/// (with his answers to a post that asks him too), but for a step's, which Send does not answer; an agent's (its
/// chat's Send, lib.rs `send`); a mail's reply box's (ask.rs `reply`).
pub fn held_for<'a>(s: &'a App, target: &Target) -> &'a [crate::ask::Comment] {
    let takes = match target {
        Target::Thread { way: Way::Step { .. }, .. } => false,
        Target::Thread { .. } | Target::Agent { .. } | Target::Reply { .. } => true,
        _ => false,
    };
    if takes { crate::ask::held(s, target.project(), &target.place()) } else { &[] }
}

/// The comments held for the box of TARGET as data, for project PI's coordd (ask.rs `data`).
fn held_data(s: &App, pi: usize, target: &Target) -> Vec<coord_types::Comment> {
    s.projects.get(pi).map(|p| crate::ask::data(p, held_for(s, target))).unwrap_or_default()
}

/// The comments held for the box of TARGET, each with its quote, then its words (ask.rs `compose`); none when it holds
/// neither.
fn with_held(s: &App, target: &Target) -> Option<String> {
    let typed = taken(s, target).unwrap_or_default();
    let held = held_for(s, target);
    if held.is_empty() {
        return (!typed.is_empty()).then_some(typed);
    }
    Some(crate::ask::compose(held, &typed).0)
}

/// REQ sent for the box of TARGET: the box empties at once and his message stands above it, `sending…`, until the
/// engine's copy is in the snapshot (outgoing.rs, `Cleared::Out`); its answer goes to that message. One that could not
/// go stands as `not sent · why` with Send again where his messages stand above the box, and goes by itself once the
/// pipe is back; either way his words are written to ui.json at the press (N-1396 p51). False when it could not go
/// from a box with no messages above it (said under the box, his words still in it).
pub(crate) fn sent(s: &mut App, target: &Target, req: Request) -> bool {
    let key = (target.project(), target.place());
    s.box_said.remove(&key);
    // with comments held for it, his message as it goes, their quotes in it: what Take back gives back
    let comments = held_for(s, target).to_vec();
    let held = !comments.is_empty();
    let typed = written(s, target).map(|e| e.text.clone()).filter(|t| !held && !t.trim().is_empty()).unwrap_or_else(|| crate::words::text_of(&req).unwrap_or_default().to_string());
    // it goes as the box holds it, at once: its words were cleaned in place while he typed (changed.rs), nothing is
    // held to clean them after Send (0.2 B9)
    s.words.unclean.remove(&key);
    let (out, refused) = crate::outgoing::press(s, key.clone(), &req, &typed, crate::outgoing::Then::Box, None);
    if let Some(o) = s.outgoing.list.iter_mut().find(|o| o.id == out) {
        o.held = comments;
    }
    if let Some(why) = refused {
        // where his messages stand above the box (a thread's, the project's): kept as `not sent · why`, said once
        // there; elsewhere (a mail reply, an end) his words stay in the box, said under it (paper-research N-460.p4)
        if !matches!(target, Target::Thread { .. } | Target::Intake { .. } | Target::NewThread { .. }) {
            crate::outgoing::forget(s, out);
            s.box_said.insert(key, Err(why));
            return false;
        }
        crate::outgoing::refused(s, out, why.trim_start_matches("not sent: "));
    }
    s.composers.remove(&key);
    if held {
        s.notes.held.remove(&key);
    }
    // his writing there ends once coordd has answered the post: the `off` waits for it (lib.rs `writing_changed`)
    s.writing_changed(key.0, &key.1);
    crate::state::write_now(s);
    true
}

/// What coordd answered to the last send from the box of PLACE, under it.
pub fn said(s: &App, pi: usize, place: &str, k: &Kit) -> Option<El> {
    let key = (pi, place.to_string());
    let answer = s.box_said.get(&key);
    // his words went as typed: they could not be cleaned; when no model answered (the way to models, which Accounts
    // holds) Accounts is beside it, not when a model answered and its answer was refused (paper-research N-584.p62, 1)
    let unclean = s.words.unclean.get(&key).map(|why| {
        let mut r = row().gap(4.0).center_y().child(text("Sent as typed; not cleaned.").font(k.ty.small).color(k.c.text_faint));
        if *why == crate::words::Unclean::NoModel {
            r = r.child(Button::new("Accounts").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.sel = crate::ext::tab_page("accounts")).build(k));
        }
        r
    });
    if answer.is_none() && unclean.is_none() {
        return None;
    }
    let mut line = row().key(format!("box-said-{pi}-{place}")).instant_changes().w_full().justify_end().gap(12.0).center_y();
    line = line.children(unclean);
    line = line.children(answer.map(|r| answer_line(r, k)));
    Some(line)
}

/// What coordd answered to a press, in his window's one way: muted when it was taken, red only when it did not happen.
pub fn answer_line(r: &Result<String, String>, k: &Kit) -> El {
    let (t, c) = match r {
        Ok(t) => (t, k.c.text_muted),
        Err(e) => (e, k.c.danger.fg),
    };
    text(t.trim().to_string()).font(k.ty.small).color(c).wrap()
}
