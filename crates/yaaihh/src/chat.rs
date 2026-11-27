//! An agent's page: what it is doing, its session as a conversation (read in pages from coordd, older ones as the
//! top comes into view), and the composer that mails it, or stops its turn and mails it (esc); or, switched to
//! Terminal, its raw terminal at full size (term.rs). Text selected in the conversation can be commented on, and a
//! comment goes with his next message from its box, quoting the passage (ask.rs `send`); a question the agent asked shows as its card (ask.rs).

use crate::live::Conv;
use crate::App;
use crate::lens::{Filter, Lens};
use coord_types::{AgentView, Entry, Request, Transcript};
use rune::{El, GrowthSpec, ScrollSpec, col, row, text};
use rune_kit::markdown::Markdown;
use rune_kit::{Button, ButtonVariant, Kit};
use crate::work::{self, Item};
use std::borrow::Cow;
use std::rc::Rc;

/// Agent A's ⋯ in project PI: the other stream, Raw log, its prompts and home (from a coordd that serves its folder),
/// Session; each pick shuts it.
fn more_menu(s: &App, pi: usize, a: &AgentView, k: &Kit) -> rune_kit::Menu {
    use rune_kit::MenuItem;
    let key = (pi, a.name.clone());
    let toggle = key.clone();
    let trigger = Button::new("⋯").variant(ButtonVariant::Ghost).help("Raw log, prompts and session").on_press(move |s: &mut App| {
        if !s.streams.more.remove(&toggle) {
            s.streams.more.insert(toggle.clone());
        }
    });
    let shut = key.clone();
    let mut m = rune_kit::Menu::new(format!("agent-menu-{pi}-{}", a.name))
        .trigger(trigger.build(k).key(format!("agent-more-{}", a.name)))
        .open(s.streams.more.contains(&key))
        .placement(rune_kit::Placement::BottomEnd)
        .on_close(move |s: &mut App| {
            s.streams.more.remove(&shut);
        });
    let pick = |f: Box<dyn Fn(&mut App)>| {
        let key = key.clone();
        move |s: &mut App| {
            s.streams.more.remove(&key);
            f(s);
        }
    };
    let agents = s.streams.which.get(&key) == Some(&crate::streams::Which::Agents);
    let (word, to) = if agents { (crate::look::say(s, "with_you", &[]), crate::streams::Which::You) } else { ("With agents".to_string(), crate::streams::Which::Agents) };
    let at = key.clone();
    m = m.item(MenuItem::new(word).on_select(pick(Box::new(move |s: &mut App| {
        s.term.pages.remove(&at);
        s.streams.which.insert(at.clone(), to);
    }))));
    let who = a.name.clone();
    m = m.item(MenuItem::new("Raw log").on_select(pick(Box::new(move |s: &mut App| s.open_raw(pi, &who)))));
    if s.projects.get(pi).is_some_and(crate::sidebar::folder_served) {
        let n = a.name.clone();
        for (word, f) in [("System prompt", Filter::SystemPrompt { agent: n.clone() }), ("Initial prompt", Filter::InitialPrompt { agent: n.clone() }), ("Home", Filter::AgentHome { agent: n })] {
            let lens = crate::Sel::Lens(Lens::of(f));
            m = m.item(MenuItem::new(word).on_select(pick(Box::new(move |s: &mut App| s.open_pane_beside(pi, lens.clone())))));
        }
    }
    let who = a.name.clone();
    let session = MenuItem::new(crate::settings::button_words(a)).checked(crate::settings::shown(s, pi, &a.name));
    m.separator().item(session.on_select(pick(Box::new(move |s: &mut App| crate::settings::toggle(s, pi, &who)))))
}

/// Each agent's ⋯ open: its items in LAYER, over the page, anchored to its trigger.
pub(crate) fn more_layer(s: &App, k: &Kit, layer: &mut Vec<El>) {
    for (pi, name) in &s.streams.more {
        if let Some(a) = s.projects.get(*pi).and_then(|p| p.live.agents.iter().find(|a| a.name == *name)) {
            more_menu(s, *pi, a, k).build(k, layer);
        }
    }
}

/// Lines of a tool result or call, or of an edit's diff, shown in the chat; the whole opens apart (popup.rs).
const SHORT: usize = 4;

/// Agent NAME of project PI at full size: its chat or its terminal ("Chat | Terminal", kept per agent), with the
/// composer. Any pane of the split shows one (main.rs `view`), so nothing here reads the current project; FOCUSED:
/// it is the focused pane, whose list of threads has the search field (find.rs).
pub fn pane(s: &App, pi: usize, name: &str, focused: bool, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let Some(a) = p.live.agents.iter().find(|a| a.name == name) else {
        return col().padding(24.0).child(text(format!("{name} is not an agent of this box")).font(k.ty.body).color(k.c.text_muted));
    };
    let key = (pi, name.to_string());
    let shows_term = s.term.pages.contains(&key);
    let which = s.streams.which.get(&key).copied().unwrap_or_default();
    // the stream, its context ring, its footprint, its drawers' toggles, and ⋯: the rest is its menu (`more_menu`)
    let dots = more_menu(s, pi, a, k).build(k, &mut Vec::new());
    let head = col()
        .w_full()
        .child(row().w_full().px(24.0).pt(10.0).gap(12.0).center_y().child(crate::streams::switch(s, pi, name, k)).children(crate::circle::control(s, pi, a, k)).child(rune::el().grow(1.0)).children(crate::footprint::strip(a, pi, k)).child(crate::drawer::toggles(s, pi, name, k)).child(dots))
        .child(facts(s, pi, a, !shows_term && which == crate::streams::Which::You, k))
        .children(crate::settings::panel(s, pi, a, k))
        .child(rune_kit::Divider::horizontal().build(k))
        // its reads waiting on him, in every view of its page, above what it says
        .children(crate::read_ask::cards(pi, a, k));
    // the drawer open beside what the page shows (drawer.rs): its commits went into Repos
    let drawer = crate::drawer::view(s, pi, a, k);
    if shows_term {
        return col().w_full().h_full().child(head).child(crate::drawer::beside(col().w_full().grow(1.0).min_h(0.0).padding(12.0).child(crate::term::view(s, k, pi, name, true)), drawer));
    }
    // With you: his threads with the agent; with none yet, its session as the chat below
    if which == crate::streams::Which::You && !crate::withyou::session_instead(p, name) {
        return col().w_full().h_full().child(head).child(crate::drawer::beside(crate::withyou::view(s, pi, a, focused, k), drawer));
    }
    // With agents: what the agent says with the others; theirs, so no box
    if which == crate::streams::Which::Agents {
        return col().w_full().h_full().child(head).child(crate::drawer::beside(crate::streams::view(s, pi, name, which, k), drawer));
    }
    let conv = p.convs.get(name);
    // the chat's rows: what is said in view, the work between folded (work.rs)
    let items = conv.map(|c| items(s, pi, name, c)).unwrap_or_default();
    let (n, last) = (items.len(), last_row(&items));
    let row_keys: Vec<String> = conv.map(|c| items.iter().map(|it| item_key(c, it)).collect()).unwrap_or_default();
    let (who, kit) = (name.to_string(), k.clone());
    // its rows, built as they come into view, drawn in the split's pane it is in (ask.rs `ours`)
    let pane = crate::ask::pane_drawn(s);
    let rows = col()
        .key(format!("rows-{pi}-{name}"))
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            n,
            72.0,
            {
                let (who, items) = (who.clone(), items.clone());
                move |s: &App, i| Cow::Owned(s.projects.get(pi).and_then(|p| p.convs.get(&who)).map(|c| item_key(c, &items[i])).unwrap_or_default())
            },
            move |s: &App, i| crate::ask::in_pane(s, pane, || item_row(s, pi, &who, &items[i], Some(i) == last, &kit)),
        )
        .on_visible_rows({
            let who = name.to_string();
            // where he reads, kept across a swap (read_place.rs): the rows' keys, the end being where it sticks
            let view = crate::read_place::chat_view(&p.id, name);
            let (scope, keys) = (format!("rows-{pi}-{name}"), row_keys);
            let rows_in = conv.is_some_and(|c| c.loaded || c.refused.is_some());
            // what is held, for reading back to a row further up (read_place.rs)
            let held = conv.map(|c| (c.session.clone(), c.start, c.entries.len(), c.next, c.loaded, c.older_pending));
            move |s: &mut App, r| {
                let at_end = r.end >= keys.len();
                let held = held.as_ref().map(|(session, start, entries, next, loaded, pending)| crate::read_place::Held { session, start: *start, entries: *entries, next: *next, loaded: *loaded, pending: *pending });
                // a row before what is held may yet come: the rows are not all in
                let rows_in = rows_in && !held.is_some_and(|h| s.read_places.behind(&view, h));
                let back = s.read_places.report(&view, &scope, &keys, r.clone(), at_end, rows_in);
                if let Some(h) = held {
                    s.read_places.file(&view, h.session);
                }
                // an earlier page only at its top once laid out at its end
                if let Some(p) = s.projects.get_mut(pi)
                    && p.convs.get_mut(&who).is_some_and(|c| wants_older(c, r, n))
                {
                    p.read_older(&who);
                }
                back
            }
        })
        // his own scroll wins over a scroll back at once
        .on_wheel({
            let view = crate::read_place::chat_view(&p.id, name);
            move |s: &mut App, _, _| {
                s.read_places.his_scroll(&view);
                false
            }
        });
    // a place further back than the pages read says so, where no other line stands
    let top = match top_line(conv) {
        t if t.is_empty() && s.read_places.further(&crate::read_place::chat_view(&p.id, name)) => crate::read_place::FURTHER.to_string(),
        t => t,
    };
    let target = name.to_string();
    let transcript = col()
        .key(format!("conv-{pi}-{name}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        // a row cut by the scroll fades out under the line's strip
        .clip_fade_linear(0.0, 12.0, 0.0, 0.0)
        .on_scroll_offset(s.read_places.offset_watch(crate::read_place::chat_view(&p.id, name)))
        .selection_group(true)
        // a passage inside one entry offers Comment; one across entries copies alone (`ask::one_source`)
        .selection_menu_item_when("comment", "Comment", crate::ask::one_source, move |s: &mut App, sel, tx| crate::ask::comment_on(s, pi, &target, sel, tx))
        .on_highlight_press(|s: &mut App, key, _| crate::ask::highlight_pressed(s, key))
        .child(rows);
    let body = col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        // the line over the chat in a strip of its own, outside the scrolled rows; none when there is no line
        .child(row().px(24.0).pt(if top.is_empty() { 0.0 } else { 6.0 }).pb(if top.is_empty() { 0.0 } else { 8.0 }).child(text(top).font(k.ty.small).color(k.c.text_faint)))
        .child(transcript)
        .child(queued(s, pi, a, k))
        .child(rune_kit::Divider::horizontal().build(k))
        .child(composer(s, pi, a, k));
    col().w_full().h_full().child(head).child(crate::drawer::beside(body, drawer))
}

/// Whether chat C, rows R of N in sight, asks for an earlier page: at its top after its end was in sight, or if all fit.
pub fn wants_older(c: &mut Conv, r: std::ops::Range<usize>, n: usize) -> bool {
    let seen = c.end_seen;
    if n > 0 && r.end >= n {
        c.end_seen = true;
    }
    n > 0 && r.start <= 2 && (seen || (r.start == 0 && r.end >= n))
}

/// The faint line over the chat: what is being read, where the session starts, or why it was not read.
fn top_line(conv: Option<&Conv>) -> String {
    match conv {
        // its last page on its way after a new stream: what is held is drawn, never as the end
        Some(c) if c.catching_up() => "reading the session…".to_string(),
        Some(c) if c.older_pending => "reading earlier entries…".to_string(),
        Some(c) if c.loaded && c.start == 0 => "the session's start".to_string(),
        Some(Conv { refused: Some(why), .. }) => format!("not read: {why}"),
        Some(c) if !c.loaded => "reading the session…".to_string(),
        _ => String::new(),
    }
}

/// The owner's posts naming the agent that it has not taken yet, greyed at the chat's foot, worded as under his post.
fn queued(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let why = match a.state.as_str() {
        "stopped" => format!("waiting · {} stopped", a.name),
        "working" => format!("waiting · {} busy since {}", a.name, crate::when::at(&a.since)),
        _ => format!("waiting · not yet sent to {}", a.name),
    };
    let mut cards = Vec::new();
    for v in p.live.tickets.values() {
        let upto = crate::seen::read_upto(v, &a.name);
        let closed = matches!(v.status, docket_machine::ViewStatus::Done { .. } | docket_machine::ViewStatus::Failed { .. });
        for post in v.posts.iter().filter(|q| !closed && q.by == "owner" && q.names.iter().chain(q.told.iter()).any(|n| *n == a.name)) {
            let body = crate::threads::state(&s.tickets, post).shown();
            // design §13: a post a handler holds, or dropped, shows only in the chat it was typed in (it names the agent)
            if post.dropped || post.held.is_some() {
                if post.names.iter().any(|n| *n == a.name) {
                    cards.push(held_card(pi, p, v, post, body.trim(), k));
                }
                continue;
            }
            if post.n <= upto {
                continue;
            }
            // told to someone other than whom it was typed to (the handler's tell): said on the message
            let to = told_elsewhere(post).map(|t| format!(" · to {t}")).unwrap_or_default();
            cards.push(
                col()
                    .key(format!("queued-{}.p{}", v.id, post.n))
                    .w_full()
                    .gap(4.0)
                    .padding(10.0)
                    .radius(k.radius.md)
                    .border(1.0, k.c.border_weak)
                    .fill(k.c.layer_01)
                    .opacity(0.6)
                    .child(text(format!("{} · {}.p{} · {}{to} · {}", crate::refs::ticket(Some(p), v).label, v.id, post.n, crate::when::at(&post.at.to_string()), why)).font(k.ty.small).color(k.c.text_faint).wrap())
                    .child(text(body.trim().to_string()).font(k.ty.body).color(k.c.text_muted).wrap().max_lines(4)),
            );
        }
        // his message to it with no post yet: held while its burst gathers, being classified, or not yet given
        if v.kind == "thread" && v.form["to"].as_str() == Some(a.name.as_str()) && !v.given.iter().any(|g| g.to == a.name) && v.posts.is_empty() {
            if let Some(goes) = crate::whereabouts::gathers(v) {
                let head = format!("you · {} · {} · {}", crate::when::at(&v.opened_at.to_string()), v.id, crate::whereabouts::held_goes(goes.as_deref()));
                let body = v.form["text"].as_str().unwrap_or("").trim();
                cards.push(
                    col()
                        .key(format!("held-{}", v.id))
                        .w_full()
                        .gap(6.0)
                        .padding(10.0)
                        .radius(k.radius.md)
                        .border(1.0, k.c.border_weak)
                        .fill(k.c.layer_01)
                        .child(text(head).font(k.ty.small).color(k.c.text_muted).wrap())
                        .child(text(body.to_string()).font(k.ty.body).color(k.c.text_base).wrap().max_lines(4))
                        .child(send_now(pi, &v.id, k)),
                );
            } else {
                let state = match &v.status {
                    docket_machine::ViewStatus::Awaits { agent, .. } if agent == "watcher" => Some("deciding where it goes".to_string()),
                    docket_machine::ViewStatus::Done { result } if result["head"] == "OneOff" => Some(format!("a one-off · {why}")),
                    _ => None,
                };
                if let Some(state) = state {
                    cards.push(
                        col()
                            .key(format!("queued-{}", v.id))
                            .w_full()
                            .gap(4.0)
                            .padding(10.0)
                            .radius(k.radius.md)
                            .border(1.0, k.c.border_weak)
                            .fill(k.c.layer_01)
                            .opacity(0.6)
                            .child(text(format!("you · {} · {state}", crate::when::at(&v.opened_at.to_string()))).font(k.ty.small).color(k.c.text_faint).wrap())
                            .child(text(v.form["text"].as_str().unwrap_or("").trim().to_string()).font(k.ty.body).color(k.c.text_muted).wrap().max_lines(4)),
                    );
                }
            }
        }
    }
    if cards.is_empty() {
        return col();
    }
    col().key(format!("queued-{pi}-{}", a.name)).w_full().px(24.0).pb(8.0).gap(6.0).children(cards)
}

/// Whom POST was told to, when a handler routed it to others than it was typed to (design §13): "fixer".
pub(crate) fn told_elsewhere(post: &docket_machine::Post) -> Option<String> {
    let mut told = post.told.clone();
    let mut named = post.names.clone();
    told.sort();
    named.sort();
    (post.routed && !told.is_empty() && told != named).then(|| post.told.join(", "))
}

/// Text struck through, drawn with the combining long stroke (U+0336) after each character: rune's spans have no
/// strikethrough of their own.
pub(crate) fn struck(s: &str) -> String {
    s.chars().flat_map(|c| [c, '\u{336}']).collect()
}

/// His POST on V that a handler holds or dropped, at the foot of the chat it was typed in, with what he can press.
fn held_card(pi: usize, p: &crate::Proj, v: &coord_types::TicketView, post: &docket_machine::Post, body: &str, k: &Kit) -> El {
    let card = |head: String, words: El| {
        col()
            .key(format!("held-{}.p{}", v.id, post.n))
            .w_full()
            .gap(6.0)
            .padding(10.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_weak)
            .fill(k.c.layer_01)
            .child(text(head).font(k.ty.small).color(k.c.text_muted).wrap())
            .child(words)
    };
    let when = crate::when::at(&post.at.to_string());
    if post.dropped {
        return card(format!("you · {} · {}.p{} · dropped", when, v.id, post.n), text(struck(body)).font(k.ty.body).color(k.c.text_faint).wrap().max_lines(4)).opacity(0.6);
    }
    let words = text(body.to_string()).font(k.ty.body).color(k.c.text_base).wrap().max_lines(4);
    let handler = post.held.as_deref().and_then(|h| crate::threads::handler(p, v, h));
    if let Some((h, goes)) = handler.and_then(|h| crate::whereabouts::gathers(h).map(|goes| (h.id.clone(), goes))) {
        let head = format!("you · {} · {}.p{} · {}", when, v.id, post.n, crate::whereabouts::held_goes(goes.as_deref()));
        return card(head, words).child(send_now(pi, &h, k));
    }
    let asking = handler.and_then(|h| match &h.status {
        docket_machine::ViewStatus::Awaits { agent, call, args, .. } if agent == "owner" && call == "clarify" => Some((h.id.clone(), args["why"].as_str().unwrap_or("").to_string())),
        _ => None,
    });
    let Some((h, why)) = asking else {
        return card(format!("you · {} · {}.p{} · routing", when, v.id, post.n), words).opacity(0.6);
    };
    card(format!("you · {} · {}.p{} · not given: {why}", when, v.id, post.n), words).border(1.0, k.c.border_strong).child(held_choices(pi, v, &h, k))
}

/// The answers to handler H's question about his held post on V: a button per agent on the ticket, everyone, drop it.
/// Drawn in the chat it was typed in and on the post's own line (whereabouts.rs).
pub(crate) fn held_choices(pi: usize, v: &coord_types::TicketView, h: &str, k: &Kit) -> El {
    let act = |label: String, head: serde_json::Value| held_answer(pi, h, label, head, k);
    let on = crate::outline::agents_on(v);
    let mut choices = row().w_full().gap(6.0).flex_wrap();
    for a in &on {
        choices = choices.child(act(a.clone(), serde_json::json!({"head": "Send", "to": [a]})));
    }
    if on.len() > 1 {
        choices = choices.child(act("everyone".into(), serde_json::json!({"head": "Send", "to": on})));
    }
    choices.child(act("drop it".into(), serde_json::json!({"head": "Drop"})))
}

/// His Send now on burst handler H waiting on his `gather`: it goes to the holder at once.
pub(crate) fn send_now(pi: usize, h: &str, k: &Kit) -> El {
    held_answer(pi, h, "Send now".into(), serde_json::json!({"head": "Send"}), k)
}

/// A button answering handler H's step owed to him with HEAD: one of `held_choices`, or `send_now`.
fn held_answer(pi: usize, h: &str, label: String, head: serde_json::Value, k: &Kit) -> El {
    let req = Request::Answer { t: h.to_string(), head, text: None };
    Button::new(label)
        .variant(ButtonVariant::Outline)
        .on_press(move |s: &mut App| {
            s.request_in(pi, req.clone(), crate::Pending::TicketDone { clear: crate::threads::Cleared::Act });
        })
        .build(k)
}

/// The agent's latest commits; a click shows the session where it made it. In its Repos drawer (drawer.rs).
pub(crate) fn commits(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let mine: Vec<crate::Commit> = s.projects.get(pi).map_or(&[][..], |p| &p.commits).iter().filter(|c| c.agent == name).take(6).cloned().collect();
    if mine.is_empty() {
        return col();
    }
    // one a line, its hash and its subject, in a drawer's width
    let lines = mine.into_iter().map(|c| {
        let hash = text(c.hash[..7.min(c.hash.len())].to_string()).font(k.ty.mono).color(k.c.text_faint);
        k.interactive(row().w_full().px(4.0).py(2.0).gap(8.0).center_y().radius(k.radius.sm).child(hash).child(text(c.subject.clone()).font(k.ty.small).color(k.c.text_muted).truncate().shrink()))
            .on_press(move |s: &mut App| {
                s.focus_on(pi, &c.agent);
                s.jump(c.clone())
            })
    });
    col().key("commits").w_full().px(8.0).gap(2.0).child(row().px(4.0).child(text("Commits").font(k.ty.small_medium).color(k.c.text_faint))).children(lines)
}

/// What the agent is, for the circle's panel only: state, id · harness · model · since, account, items, memory, place.
pub fn facts_lines(s: &App, p: &crate::Proj, a: &AgentView) -> Vec<String> {
    let mut out = vec![crate::streams::state_words(p, a), format!("{} · {} · {} · since {}", a.id, a.kind, a.model, crate::when::at(&a.since))];
    out.extend(crate::accounts::agent_account(s, a).map(|l| format!("on {l}")));
    if !a.items.is_empty() {
        out.push(format!("works on {}", a.items.join(", ")));
    }
    out.extend(a.mem_mb.map(|m| format!("{m} MB")));
    out.extend(crate::agents::where_lines(a).into_iter().filter(|(_, warn)| !warn).map(|(t, _)| t));
    out
}

/// The line under the head, only when there is something for him (docs/specs/window-rules.md §1, §5): what waits on him in
/// this agent's topics, in the accent, its click opening With you, which lists them (outline.rs `waits_in`); what went wrong with its place, in support text; and what it
/// does now, quiet. The rest of what it is goes behind the circle (`facts_lines`).
fn facts(s: &App, pi: usize, a: &AgentView, below: bool, k: &Kit) -> El {
    let mut c = col().w_full();
    // a dialog on its terminal holds it: said first, with the press that shows the terminal to answer it in
    if let Some(title) = &a.dialog {
        let name = a.name.clone();
        let open = Button::new("Open its terminal").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| {
            s.term.pages.insert((pi, name.clone()));
        });
        let card = row().w_full().gap(12.0).center_y().padding(10.0).radius(k.radius.md).border(1.0, k.c.border_weak).fill(k.c.layer_01).key(format!("agent-waiting-card-{}", a.name))
            .child(col().shrink().min_w(0.0).gap(2.0).child(text(crate::agents::WAITING).font(k.ty.ui_medium).color(k.c.text_accent)).child(text(title.clone()).font(k.ty.small).color(k.c.text_muted).wrap()))
            .child(open.build(k).key(format!("agent-waiting-open-{}", a.name)));
        c = c.child(row().w_full().px(24.0).pt(8.0).child(card));
    }
    let waits = s.projects.get(pi).map_or(0, |p| crate::outline::waits_in_drawn(p, Some(&a.name), &crate::outline::pieces_drawn(p, Some(&a.name))).len());
    if waits > 0 {
        let name = a.name.clone();
        let words = crate::refs::count(&crate::look::say(s, "waits_on_you", &[]), waits);
        c = c.child(
            row().w_full().px(24.0).pt(8.0).child(
                k.interactive(text(words).font(k.ty.small).color(k.c.danger.fg)).on_press(move |s: &mut App| {
                    s.term.pages.remove(&(pi, name.clone()));
                    s.streams.which.insert((pi, name.clone()), crate::streams::Which::You);
                }),
            ),
        );
    }
    for (t, _) in crate::agents::where_lines(a).into_iter().filter(|(t, warn)| *warn && !t.starts_with(crate::agents::WAITING)) {
        let line = text(t.clone()).font(k.ty.small).color(k.c.text_muted).wrap();
        // its session takes no messages: what he can do as presses in coordd's words
        let line = if a.deaf.as_ref() == Some(&t) { crate::agents::deaf_links(line, &t, pi, a, k) } else { line };
        c = c.child(row().px(24.0).pt(6.0).child(line));
    }
    if !below && let Some(l) = crate::streams::working_now(a, k) {
        c = c.child(row().w_full().px(24.0).pt(6.0).child(l));
    }
    c.pb(6.0)
}


/// Each agent chat's rows by (project id, agent), with what they were built from, so a frame doesn't rebuild them.
#[derive(Default)]
pub struct Rows(std::cell::RefCell<std::collections::HashMap<(String, String), (RowsFrom, Rc<Vec<Item>>)>>);

type RowsFrom = ((bool, usize, u64, u64, String, usize), (usize, u64), u64, Vec<(u64, u64)>);

/// Which runs are open, as one number for any order of `App::expanded` (a run's key begins `w-`, work.rs).
fn runs_open(expanded: &std::collections::HashSet<String>) -> (usize, u64) {
    use std::hash::{BuildHasher, BuildHasherDefault};
    let h = BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default();
    expanded.iter().filter(|k| k.starts_with("w-")).fold((0, 0), |(n, x), k| (n + 1, x ^ h.hash_one(k)))
}

/// AGENT's chat C in project PI as its rows, with its brain's passes that spoke at the times they spoke
/// ([`with_passes`]), kept until C, its open runs, the mail or those passes move.
pub fn items(s: &App, pi: usize, agent: &str, c: &Conv) -> Rc<Vec<Item>> {
    let p = s.projects.get(pi);
    let passes = p.map(|p| crate::agents::passes(s, p, agent)).unwrap_or_default();
    let from: RowsFrom = (c.stamp(), runs_open(&s.expanded), p.map_or(0, |p| p.live.mail.rev), passes);
    let key = (p.map_or(String::new(), |p| p.id.clone()), agent.to_string());
    if let Some((at, rows)) = s.chat_rows.0.borrow().get(&key)
        && *at == from
    {
        return rows.clone();
    }
    let rows = work::items(c, &s.expanded, |t| p.is_some_and(|p| crate::ask::asked(p, t).is_some()));
    let rows = Rc::new(with_passes(c, rows, &from.3));
    s.chat_rows.0.borrow_mut().insert(key, (from, rows.clone()));
    rows
}

/// ROWS of chat C with PASSES (its brain's passes that spoke), each a row at the time it spoke, never inside a run.
fn with_passes(c: &Conv, rows: Vec<Item>, passes: &[(u64, u64)]) -> Vec<Item> {
    let Some(first) = c.entries.iter().find_map(|e| crate::turn::nanos(&e.at)) else { return rows };
    let at = |it: &Item| match *it {
        Item::Entry(i) | Item::Work { from: i, .. } => crate::turn::nanos(&c.entries[i].at),
        // his edit stands before entry AT: a pass before that entry goes before it
        Item::Restarted { at: i, .. } => c.entries.get(i).and_then(|e| crate::turn::nanos(&e.at)),
        Item::Call { .. } | Item::Pass(_) => None,
    };
    let mut next = passes.iter().filter(|(_, t)| *t >= first).peekable();
    let mut out = Vec::with_capacity(rows.len() + passes.len());
    for it in rows {
        if let Some(t) = at(&it) {
            while let Some((n, _)) = next.next_if(|(_, p)| *p < t) {
                out.push(Item::Pass(*n));
            }
        }
        out.push(it);
    }
    out.extend(next.map(|(n, _)| Item::Pass(*n)));
    out
}

/// The place in ITEMS of the chat's last row of its own, which goes on while the agent works ([`item_row`]'s LAST): a
/// pass that spoke after it, at a compaction in the turn, does not take that from it.
pub fn last_row(items: &[Item]) -> Option<usize> {
    items.iter().rposition(|it| !matches!(it, Item::Pass(_)))
}

/// The element key of ITEM: its entry's, its run's, its call's or its pass's.
pub fn item_key(c: &Conv, item: &Item) -> String {
    match *item {
        Item::Entry(i) => c.keys[i].clone(),
        Item::Work { from, .. } => work::run_key(c, from),
        Item::Call { call, .. } => work::call_key(c, call),
        Item::Pass(n) => format!("pass-{n}"),
        Item::Restarted { mark, .. } => format!("restarted-{mark}-{}", c.restarted.get(mark).map_or(0, |r| r.offset)),
    }
}

/// ITEM of AGENT's chat in project PI as its row (the full view's and a tile's); LAST: it is the chat's last row (a
/// run there is the work going on while the agent works).
pub fn item_row(s: &App, pi: usize, agent: &str, item: &Item, last: bool, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let Some(c) = p.convs.get(agent) else { return col() };
    let a = p.live.agents.iter().find(|a| a.name == agent);
    let cwd = a.and_then(|a| a.cwd.as_deref());
    let key = item_key(c, item);
    // a pass's row is a few words, built each frame: the labels of the ids it names come in after it is first drawn
    if let Item::Pass(n) = *item {
        return pass_row(s, pi, n, key, k);
    }
    let live = last && a.is_some_and(|a| a.state == "working");
    // a row drawn once and reused until it changes (Rune's subtree cache): a frame for the spinner of the run going
    // on, or for anything else that moves, no longer repaints every row's text (9 Nov, m151 fix 2); the live run
    // itself, whose spinner changes 30 times a second, draws live
    let shown: Vec<usize> = match *item {
        Item::Entry(i) => vec![i],
        Item::Work { from, to } => (from..to.min(c.entries.len())).collect(),
        Item::Call { call, result } => std::iter::once(call).chain(result).collect(),
        Item::Pass(_) | Item::Restarted { .. } => Vec::new(),
    };
    // a row is built again only when what it reads changes (Rune's memo): a keystroke in the composer, or a new entry
    // below, reuses it as it is
    let mut v = crate::version::Version::default();
    // a mail's or coord ask's row also shows its thread and its reply box (ask.rs)
    if let Item::Entry(i) = *item && matches!(c.entries[i].kind.as_str(), "mail" | "result") {
        // its reply box open: the one message box reads more than a version says (spelling, speech, the cleaning),
        // so the row is built each frame, as every box is
        if crate::ask::holds_box(s, p, &c.entries[i].kind, &c.entries[i].text) {
            return item_built(s, pi, agent, c, item, key, cwd, live, k);
        }
        crate::ask::thread_version(s, p, &mut v);
    }
    v.add((&c.session, live, cwd, s.expanded.contains(&key)));
    // his edit's row: the fold under it as it stands, and its images
    if let Item::Restarted { mark, .. } = *item {
        let fold = c.restarted.get(mark).and_then(|r| r.line.as_ref()).and_then(|l| c.folds.get(&l.from));
        v.add((c.restarted.get(mark).map(|r| (r.offset, r.at.as_str())), p.protocol, p.down.is_some()));
        if let Some(f) = fold {
            v.add((f.open, f.conv.stamp(), f.conv.newer_pending, f.conv.refused.as_deref()));
            for e in f.conv.entries.iter().filter(|_| f.open) {
                for n in &e.images {
                    v.add(matches!(image(s, &image_key(agent, &f.conv.session, e.offset, *n)), Some(Img::Ready(_) | Img::Failed(_))));
                }
            }
        }
    }
    for &i in &shown {
        let (e, ek) = (&c.entries[i], &c.keys[i]);
        v.add((ek, s.expanded.contains(ek), s.found.as_deref() == Some(ek.as_str())));
        for n in &e.images {
            match image(s, &image_key(agent, &c.session, e.offset, *n)) {
                Some(Img::Ready(img)) => v.add((1u8, img.size(), img.error().map(|e| e.to_string()))),
                Some(Img::Failed(why)) => v.add((2u8, why)),
                Some(Img::Asked | Img::Bytes(_)) | None => v.add(0u8),
            };
        }
        crate::ask::marks_version(s, p, &format!("{agent}@{}", e.offset), v.hasher());
    }
    v.add(k.dark);
    let version = v.finish();
    rune::memo::memo_with_hash(key.clone(), version, || item_built(s, pi, agent, c, item, key, cwd, live, k))
}

/// ITEM's row as [`item_row`] builds it, keyed KEY.
#[allow(clippy::too_many_arguments)]
fn item_built(s: &App, pi: usize, agent: &str, c: &Conv, item: &Item, key: String, cwd: Option<&str>, live: bool, k: &Kit) -> El {
    let row = match *item {
        Item::Entry(i) => entry(s, pi, agent, &c.entries[i], &c.keys[i], cwd, k).key(key),
        Item::Work { from, to } => {
            let line = work::run_line(s, c, from, to, live, k);
            // a folded run's images (screenshots it read) stay in view under its line
            // (open, its calls show them)
            let open = s.expanded.contains(&key);
            let shown: Vec<El> = if open { Vec::new() } else { (from..to.min(c.entries.len())).filter_map(|i| images(s, pi, agent, &c.entries[i], k)).collect() };
            if shown.is_empty() { line.key(key) } else { col().key(key).w_full().gap(6.0).child(line).child(col().pl(20.0).gap(8.0).children(shown)) }
        }
        Item::Restarted { mark, .. } => restarted_row(s, pi, agent, c, mark, cwd, k).key(key),
        Item::Call { call, result } => {
            let line = work::call_line(s, c, call, result, k);
            if s.expanded.contains(&key) {
                let shown = std::iter::once(call).chain(result).map(|i| entry(s, pi, agent, &c.entries[i], &c.keys[i], cwd, k).key(c.keys[i].clone()).pl(20.0));
                col().key(key).w_full().child(line).children(shown.collect::<Vec<_>>())
            } else {
                line.key(key)
            }
        }
        Item::Pass(n) => return pass_row(s, pi, n, key, k),
    };
    if live { row } else { row.cache() }
}

/// Pass N of the agent's brain as its chat row, keyed KEY: what it was a pass on, when, its first line; a press opens it.
fn pass_row(s: &App, pi: usize, n: u64, key: String, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col().key(key) };
    let Some((v, post)) = crate::find::views(s, p).find(|v| v.n == n).and_then(|v| Some((v, crate::agents::spoke(v)?))) else { return col().key(key) };
    let (words, rests_on) = pass_words(s, v, post);
    let open = s.expanded.contains(&key);
    let faint = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    let head = row()
        .w_full()
        .gap(6.0)
        .center_y()
        .child(text(if open { "▾" } else { "▸" }).font(k.ty.small).color(k.c.text_faint))
        .child(text("◆").font(k.ty.small).color(k.c.text_accent))
        .child(text("its brain").font(k.ty.small_medium).color(k.c.text_muted))
        .children(pass_on(v).map(faint))
        .child(faint(crate::when::at(&post.at)));
    let gist = (!open).then(|| text(crate::texts::first_line_cut(&words)).font(k.ty.small).color(k.c.text_base).truncate());
    let fold = key.clone();
    let line = k.interactive(col().w_full().gap(4.0).child(head).children(gist)).help(if open { "Fold its words" } else { "Its words and what it rested on" }).on_press(move |s: &mut App| work::toggle(s, &fold));
    let mut c = col().key(key).w_full().px(24.0).py(8.0).gap(6.0).child(line);
    if open {
        let (t, id) = (v.n, v.id.clone());
        let pass = k.interactive(crate::refs::el(&crate::refs::Ref { label: "the pass".into(), id: v.id.clone() }, k.ty.small, k.c.text_muted, k)).help("Open the pass").on_press(move |s: &mut App| crate::find::open_at(s, t, &id, None));
        let rested = (!rests_on.is_empty()).then(|| row().w_full().gap(8.0).child(faint("rested on".into())).child(col().grow(1.0).shrink().min_w(0.0).child(crate::post::message_body(Some(p), &rests_on, None, None, k))));
        c = c.child(
            col()
                .key(format!("pass-words-{n}"))
                .w_full()
                .padding(12.0)
                .gap(8.0)
                .radius(k.radius.md)
                .fill(k.c.layer_01)
                .border(1.0, k.c.border_weak)
                .child(crate::post::message_body(Some(p), &words, None, None, k))
                .children(rested)
                .child(pass),
        );
    }
    c
}

/// What pass V was a pass on: `after 20 turns` (Ahead), `at its compaction` (Compacting), `on your posts` (Attend).
fn pass_on(v: &coord_types::TicketView) -> Option<String> {
    let pass = v.form.get("pass")?;
    let head = pass.get("head").and_then(serde_json::Value::as_str).or_else(|| pass.as_str())?;
    match head {
        "Ahead" => Some(match pass.get("turns").and_then(serde_json::Value::as_u64) {
            Some(1) => "after its turn".into(),
            Some(n) => format!("after {n} turns"),
            None => "at a turn end".into(),
        }),
        "Compacting" => Some("at its compaction".into()),
        "Attend" => Some("on your posts".into()),
        _ => None,
    }
}

/// Pass V's words and what they rest on, its POST being what it said: its result's thought (Next, Said) or text (Kept),
/// else the post's words; its `rests_on` (Next, Said), else the ids its post names (Kept's lines each name theirs:
/// `TicketView::mentions`), comma-separated, none when it names none.
fn pass_words(s: &App, v: &coord_types::TicketView, post: &docket_machine::Post) -> (String, String) {
    let result = match &v.status {
        docket_machine::ViewStatus::Done { result } => Some(result),
        _ => None,
    };
    let field = |f: &str| result.and_then(|r| r.get(f)).and_then(serde_json::Value::as_str).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
    let words = field("thought").or_else(|| field("text")).unwrap_or_else(|| crate::threads::state(&s.tickets, post).shown());
    let rests_on = field("rests_on").unwrap_or_else(|| v.mentions.get(&post.n).map(|ids| ids.join(", ")).unwrap_or_default());
    (words, rests_on)
}

/// One entry of the session (also a tile's chat tail, agents.rs).
/// An entry of AGENT's session in project PI. CWD is the agent's folder: what a relative path in its text means
/// (paths.rs).
/// A chat's image: asked of coordd, held as it came (decoded only while it shows: [`image`]), decoded (or decoding),
/// or why not.
pub enum Img {
    Asked,
    /// The encoded picture, decoded only to draw: decoded images held for the app's life reached 5.5 GB.
    Bytes(std::sync::Arc<[u8]>),
    Ready(rune::Image),
    Failed(String),
}

/// The most the images decoded lately hold together; past it the oldest drawn is let go of (it is decoded again from
/// its bytes when it shows again). The newest [`DECODED_FEW`] stay whatever their size.
const DECODED_MAX: u64 = 512 << 20;
const DECODED_FEW: usize = 4;

/// The images [`image`] decoded lately, the last drawn first: they stay decoded while they show and a little after.
#[derive(Default)]
pub struct Decoded(std::sync::Mutex<std::collections::VecDeque<(String, rune::Image)>>);

/// What a decoded image holds: its pixels and mips (a third more); one still decoding counts as a large still.
fn cost(i: &rune::Image) -> u64 {
    i.size().map_or(16 << 20, |[w, h]| u64::from(w) * u64::from(h) * 4 * 4 / 3)
}

impl Decoded {
    fn keep(&self, key: &str, img: &rune::Image) {
        self.keep_within(key, img, DECODED_MAX);
    }

    fn keep_within(&self, key: &str, img: &rune::Image, max: u64) {
        let mut q = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match q.iter().position(|(k, _)| k == key) {
            Some(0) => {}
            Some(at) => {
                let e = q.remove(at).expect("found");
                q.push_front(e);
            }
            None => q.push_front((key.to_string(), img.clone())),
        }
        let (mut total, mut kept) = (0, 0);
        for (i, (_, x)) in q.iter().enumerate() {
            total += cost(x);
            if i >= DECODED_FEW && total > max {
                break;
            }
            kept = i + 1;
        }
        q.truncate(kept);
    }

    #[cfg(test)]
    fn keys(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().map(|(k, _)| k.clone()).collect()
    }
}

/// Image KEY as it draws now. One held as its bytes is decoded through the cache, which hands back the same image while
/// something still holds it (an element drawing it, or [`Decoded`]), so only what shows, and what showed lately, is
/// held decoded.
pub fn image(s: &App, key: &str) -> Option<Img> {
    Some(match s.images.get(key)? {
        Img::Bytes(b) => match s.image_cache.as_ref() {
            Some(c) => {
                let i = c.load(b.clone());
                s.decoded.keep(key, &i);
                Img::Ready(i)
            }
            None => Img::Failed("no image decoder".into()),
        },
        Img::Ready(i) => Img::Ready(i.clone()),
        Img::Failed(w) => Img::Failed(w.clone()),
        Img::Asked => Img::Asked,
    })
}

/// An image's key: the agent, the transcript, its line's offset and its place among that line's images.
pub fn image_key(agent: &str, session: &str, offset: u64, n: u32) -> String {
    format!("{agent}|{session}|{offset}|{n}")
}

/// The requests for the images of PAGE's entries not held yet.
pub fn image_requests(agent: &str, page: &Transcript, held: &std::collections::HashMap<String, Img>) -> Vec<(String, Request)> {
    page.entries
        .iter()
        .flat_map(|e| e.images.iter().map(move |n| (e.offset, *n)))
        .map(|(offset, n)| (image_key(agent, &page.session, offset, n), Request::Image { name: agent.into(), session: page.session.clone(), offset, n }))
        .filter(|(key, _)| !held.contains_key(key))
        .collect()
}

/// The requests for all images of what is held of AGENT's session (a headless scene's).
pub fn conv_image_requests(agent: &str, c: &Conv) -> Vec<(String, Request)> {
    let page = Transcript { entries: c.entries.clone(), start: c.start, next: c.next, session: c.session.clone(), ..Default::default() };
    image_requests(agent, &page, &Default::default())
}

/// coordd's answer to an image request (`{"media", "data"}`), held as it came: [`image`] decodes it when it shows.
pub fn take_image(ok: bool, out: &str) -> Img {
    use base64::Engine;
    if !ok {
        return Img::Failed(out.to_string());
    }
    let data = serde_json::from_str::<serde_json::Value>(out).ok().and_then(|v| Some(v.get("data")?.as_str()?.to_string()));
    match data.and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok()) {
        Some(bytes) => Img::Bytes(bytes.into()),
        None => Img::Failed("not an image".into()),
    }
}

/// The widest an image in a chat draws; a narrower one keeps its own width.
const IMAGE_MAX_W: f32 = 720.0;

/// E's images, each at most [`IMAGE_MAX_W`] wide (half its pixels: screenshots are taken at 2×) at its aspect ratio.
fn images(s: &App, pi: usize, agent: &str, e: &Entry, k: &Kit) -> Option<El> {
    if e.images.is_empty() {
        return None;
    }
    let session = s.projects.get(pi)?.convs.get(agent)?.session.clone();
    images_in(s, agent, &session, e, k)
}

/// E's images as [`images`] draws them, E an entry of AGENT's transcript SESSION: the old line's beside his edit too.
fn images_in(s: &App, agent: &str, session: &str, e: &Entry, k: &Kit) -> Option<El> {
    if e.images.is_empty() {
        return None;
    }
    let mut out = col().gap(8.0);
    for n in &e.images {
        let key = image_key(agent, session, e.offset, *n);
        out = out.child(match image(s, &key) {
            Some(Img::Ready(img)) if img.error().is_none() => {
                let w = img.size().map_or(IMAGE_MAX_W, |[w, _]| (w as f32 / 2.0).min(IMAGE_MAX_W));
                let open = key.clone();
                rune::el().w(w).radius(k.radius.sm).border(1.0, k.c.border_weak).pixels(img.pixels()).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| crate::viewer::show(s, open.clone()))
            }
            Some(Img::Ready(img)) => text(format!("image: {}", img.error().map(|e| e.to_string()).unwrap_or_default())).font(k.ty.small).color(k.c.text_faint),
            Some(Img::Failed(why)) => text(format!("image: {why}")).font(k.ty.small).color(k.c.text_faint),
            Some(Img::Asked | Img::Bytes(_)) | None => text("image: reading…").font(k.ty.small).color(k.c.text_faint),
        });
    }
    Some(out)
}

/// His system-prompt edit in words: `System prompt changed · 10:12`, then `from turn 7` or `from an earlier point`.
pub fn restarted_words(r: &docket_machine::graph::Restarted) -> String {
    let when = crate::when::at(&r.at);
    match &r.line {
        Some(l) if l.turns > 0 => format!("System prompt changed · {when} · from turn {}", l.turns),
        Some(_) => format!("System prompt changed · {when} · from an earlier point"),
        None => format!("System prompt changed · {when}"),
    }
}

/// His system-prompt edit as its row across AGENT's chat at C's MARK; made from a turn, the old line folds under it.
fn restarted_row(s: &App, pi: usize, agent: &str, c: &Conv, mark: usize, cwd: Option<&str>, k: &Kit) -> El {
    let Some(r) = c.restarted.get(mark) else { return col() };
    let rule = || rune::el().h(1.0).grow(1.0).fill(k.c.border_weak);
    let head = row().w_full().gap(10.0).center_y().child(rule()).child(text(restarted_words(r)).font(k.ty.small_medium).color(k.c.text_muted)).child(rule());
    let out = col().w_full().px(24.0).py(10.0).gap(8.0).child(head);
    let served = s.projects.get(pi).is_some_and(|p| p.down.is_none() && p.protocol >= coord_types::TRANSCRIPT_SESSION_PROTOCOL);
    let Some(l) = r.line.as_ref().filter(|_| served) else { return out };
    let fold = c.folds.get(&l.from);
    let open = fold.is_some_and(|f| f.open);
    let after = if l.turns > 0 { format!("turn {}", l.turns) } else { "this point".to_string() };
    let said = format!("{} The old turns after {after}", if open { "▾" } else { "▸" });
    let (who, from, cut) = (agent.to_string(), l.from.clone(), r.offset);
    let press = k
        .interactive(row().gap(8.0).center_y().child(text(said).font(k.ty.small_medium).color(k.c.text_accent)))
        .key(format!("fold-{pi}-{agent}-{cut}"))
        .on_press(move |s: &mut App| {
            if let Some(p) = s.projects.get_mut(pi) {
                p.toggle_fold(&who, &from, cut);
            }
        });
    let out = out.child(press);
    if open { out.child(old_rows(s, pi, agent, fold, cwd, k)) } else { out }
}

/// The old line's part after his edit, as it was read ([`crate::live::Fold`]): what was said, as the chat draws it,
/// each run of work summed up on its line (work.rs `run_sum`), an edit of his in it by its words, its images; read
/// only (no comment, reply box or popup: its bytes are not the new line's). Then where it ends and the new line goes on.
fn old_rows(s: &App, pi: usize, agent: &str, fold: Option<&crate::live::Fold>, cwd: Option<&str>, k: &Kit) -> El {
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    let Some(f) = fold.filter(|f| f.conv.loaded) else {
        return match fold.and_then(|f| f.conv.refused.clone()) {
            Some(why) => note(format!("the old turns were not read: {why}")).pl(24.0),
            None => note("reading the old turns…".to_string()).pl(24.0),
        };
    };
    let o = &f.conv;
    let rows: Vec<El> = work::items(o, &Default::default(), |_| false)
        .into_iter()
        .map(|it| match it {
            Item::Entry(i) => old_entry(s, pi, agent, &o.session, &o.entries[i], cwd, k),
            Item::Work { from, to } => work::run_sum(&o.entries[from..to], k),
            Item::Restarted { mark, .. } => o.restarted.get(mark).map_or(col(), |r| note(restarted_words(r)).pl(24.0)),
            Item::Call { .. } | Item::Pass(_) => col(),
        })
        .collect();
    let end = if rows.is_empty() { "nothing came after this before the change" } else { "new system prompt from here" };
    col().w_full().py(8.0).gap(4.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).children(rows).child(note(end.to_string()).pl(24.0))
}

/// Entry E of the old line (AGENT's transcript SESSION) as [`old_rows`] draws it: who said it and when, its words
/// (mail's without its tag), its images.
fn old_entry(s: &App, pi: usize, agent: &str, session: &str, e: &Entry, cwd: Option<&str>, k: &Kit) -> El {
    let proj = s.projects.get(pi);
    let (who, words) = match e.kind.as_str() {
        "assistant" => (agent.to_string(), e.text.clone()),
        "mail" | "user" if channel_words(&e.text).is_some() => {
            let (words, _) = channel_words(&e.text).unwrap_or_default();
            (channel_heading(&e.text).unwrap_or_else(|| "mail".into()), words)
        }
        _ if his(e) => ("you".to_string(), e.text.clone()),
        "result" => ("result".to_string(), e.text.clone()),
        _ => ("prompt".to_string(), e.text.clone()),
    };
    let label = row().gap(6.0).child(text(who).font(k.ty.small_medium).color(k.c.text_muted)).child(text(crate::when::at(&e.at)).font(k.ty.small).color(k.c.text_faint));
    let body = crate::post::message_body(proj, &words, None, cwd.map(String::from), k);
    col().w_full().px(24.0).py(6.0).gap(4.0).child(label).child(body).children(images_in(s, agent, session, e, k))
}

/// What a delivery is, from its channel tag (`step C-12.s1`, `message from you · N-44`, `3 threads`); None without a ticket.
pub(crate) fn channel_heading(t: &str) -> Option<String> {
    let tag = t.strip_prefix("<channel ")?.split('>').next()?;
    let attr = |name: &str| {
        let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
        tag[at..].split('"').next().map(str::to_string)
    };
    // the owner's words to an agent holding only them: the tag says only who they are from
    if attr("source").as_deref() == Some("coord") && attr("ticket").is_none() && attr("id").is_none() {
        return attr("from").map(|f| if f == "owner" { crate::refs::person(&f) } else { format!("from {f}") });
    }
    // one message joining several deliveries: how many threads, and whose when they are his posts into a running turn
    if attr("source").as_deref() == Some("coord") {
        let n = attr("id").map_or(0, |ids| ids.split_whitespace().count());
        match attr("kind").as_deref() {
            Some("handover") => return Some(format!("{n} threads")),
            Some("owner") => return Some(format!("{} on {n} threads", crate::refs::person("owner"))),
            _ => {}
        }
    }
    let ticket = attr("ticket")?;
    if let Some(step) = attr("step") {
        return Some(format!("step {ticket}.s{step}"));
    }
    // one word for a post delivered, whatever kind of ticket it came on (0.2's C16: no "note" beside "thread")
    Some(match attr("from").as_deref() {
        None | Some("coord") => format!("from coord · {ticket}"),
        Some(from) => format!("message from {} · {ticket}", crate::refs::person(from)),
    })
}

pub fn entry(s: &App, pi: usize, agent: &str, e: &Entry, key: &str, cwd: Option<&str>, k: &Kit) -> El {
    let proj = s.projects.get(pi);
    let his = his(e);
    // `coord ask`'s answer: the question it made, as its card
    if e.kind == "result" {
        if let Some((p, q)) = proj.and_then(|p| Some((p, crate::ask::asked(p, &e.text)?))) {
            return col().w_full().px(24.0).py(8.0).child(crate::ask::card(s, p, q, k));
        }
    }
    // mail: each mail as its card, with its thread
    // under the owner's message, what became of it and its override (`verdict_chip`)
    let chip = (e.kind == "mail").then(|| proj.and_then(|p| verdict_chip(pi, p, agent, e, k))).flatten();
    if e.kind == "mail" {
        if let Some(m) = proj.and_then(|p| crate::ask::mail_entry(s, p, agent, &e.text, &e.at, k)) {
            let c = col().w_full().px(24.0).py(8.0).gap(6.0).child(m).children(chip);
            return if his { c.items_end() } else { c };
        }
    }
    // the comments on this entry, highlighted where they stand
    let source = format!("{agent}@{}", e.offset);
    let marks = |src: &str| proj.map(|p| crate::ask::marks(s, p, &source, src, k)).unwrap_or_default();
    // its words drawn as every message's are, ids as their labels, paths as links
    let md = |t: &str| crate::post::message_body(proj, t, Some((s, source.as_str())), cwd.map(String::from), k);
    let label = |t: String, ink| row().gap(6.0).child(text(t).font(k.ty.small_medium).color(ink)).child(text(crate::when::at(&e.at)).font(k.ty.small).color(k.c.text_faint));
    let body = match (e.kind.as_str(), posted(e)) {
        // the reply tool's call: its words drawn as the post it made, with what it answers as a reference
        (_, Some((to, words))) => {
            let thread = to.split_once(".p").map_or(to.as_str(), |(t, _)| t);
            let r = crate::refs::Ref { label: proj.map_or_else(|| "a ticket".to_string(), |p| crate::refs::ticket_id(p, thread).label), id: to.clone() };
            let head = row()
                .gap(6.0)
                .center_y()
                .child(text(format!("{} →", e.tool.clone().unwrap_or_default())).font(k.ty.small_medium).color(k.c.text_muted))
                .child(crate::refs::el(&r, k.ty.small, k.c.text_muted, k))
                .child(text(crate::when::at(&e.at)).font(k.ty.small).color(k.c.text_faint));
            col().w_full().gap(4.0).padding(8.0).child(head).child(md(&words))
        }
        ("assistant", None) => col().gap(4.0).child(md(&e.text)),
        ("user" | "mail", None) => {
            // the accent is for what needs him: mail is told from a prompt by its heading's brightness
            let (who, ink) = if e.kind == "mail" { (channel_heading(&e.text).unwrap_or_else(|| "mail".into()), k.c.text_base) } else { ("prompt".into(), k.c.text_muted) };
            if his {
                // his words on the right, in his bubble, as on a thread (window rules §6)
                col().w_full().items_end().gap(6.0).child(label(who, ink)).child(crate::post::bubble_for(&e.text, md(&e.text), false, k))
            } else {
                col()
                    .gap(6.0)
                    .child(label(who, ink))
                    .child(col().w_full().padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(md(&e.text)))
            }
        }
        _ if e.patch.is_some() => {
            // an edit: the change as a diff, its first lines here and the whole apart
            let patch = e.patch.as_deref().unwrap_or_default();
            let head = if e.kind == "tool" { e.tool.clone().unwrap_or_default() } else { "diff".into() };
            let (shown, more) = cut(patch);
            let src = format!("```diff\n{shown}\n```");
            let marks = marks(&src);
            let diff = |src: String, marks: Vec<crate::ask::Mark>| Markdown::new(src).highlights(marks).selectable(more == 0).build(k);
            // the comment box's room after the passage, while it stands in the flow
            let diff = match crate::ask::cut(s, &source, &src, crate::ask::Here::default()) {
                Some(cut) => crate::ask::split(s, &source, &src, &marks, cut, "diff-words", |part, marks, id| diff(part, marks).key(id)),
                None => diff(src, marks),
            };
            let b = col().w_full().gap(4.0).padding(8.0).radius(k.radius.sm).child(label(head.clone(), k.c.text_muted)).child(diff);
            apart(b, more, pi, agent, e, key, &head, k)
        }
        _ => {
            // a tool call or its result: a few lines, the whole text apart
            let (shown, more) = cut(&e.text);
            let head = e.tool.clone().unwrap_or_else(|| "result".into());
            let b = col()
                .w_full()
                .gap(4.0)
                .padding(8.0)
                .radius(k.radius.sm)
                .child(label(head.clone(), k.c.text_muted))
                .child(text(shown.into_owned()).font(k.ty.mono).color(ink(e, k)).wrap_anywhere());
            apart(b, more, pi, agent, e, key, &head, k)
        }
    };
    let body = match images(s, pi, agent, e, k) {
        // pasted images with no words: the label and the images, no empty text box
        Some(imgs) if e.text.trim().is_empty() && (e.kind == "user" || e.kind == "mail") => {
            let c = col().gap(6.0).child(label("prompt".into(), k.c.text_muted)).child(imgs);
            if his { c.items_end() } else { c }
        }
        Some(imgs) => col().w_full().gap(6.0).child(body).child(imgs),
        None => body,
    };
    // the comment box's room at its end, when no passage of it is lit
    let row = col().w_full().px(24.0).py(8.0).gap(6.0).child(body).children(chip).children(crate::ask::room_at(s, key));
    let row = if his { row.items_end() } else { row };
    // the entry a commit was found at
    if s.found.as_deref() == Some(key) { row.fill(k.c.surface_raised).border(1.0, k.c.border_focus) } else { row }
}

/// A call of coord's reply tool in E: the first id it answers and its words; None if its arguments are cut.
pub(crate) fn posted(e: &Entry) -> Option<(String, String)> {
    let tool = e.tool.as_deref()?;
    if e.kind != "tool" || !(tool.contains("coord") && tool.ends_with("reply")) {
        return None;
    }
    let args: serde_json::Value = serde_json::from_str(&e.text).ok()?;
    let to = args.get("to")?.as_str()?.split(',').next()?.trim().to_string();
    Some((to, args.get("text")?.as_str()?.to_string()))
}

/// TEXT's first [`SHORT`] lines and how many more it has; the whole of it when that is all.
pub(crate) fn cut(text: &str) -> (Cow<'_, str>, usize) {
    let n = text.lines().count();
    if n <= SHORT {
        return (Cow::Borrowed(text), 0);
    }
    (Cow::Owned(text.lines().take(SHORT).collect::<Vec<_>>().join("\n")), n - SHORT)
}

/// A tool call's text drawn in the base ink, its result's muted.
fn ink(e: &Entry, k: &Kit) -> rune::Color {
    if e.kind == "tool" { k.c.text_base } else { k.c.text_muted }
}

/// B, entry E's box (keyed KEY in AGENT's session in project PI, headed HEAD), MORE of its lines left out: how many,
/// and a press opens it whole apart (popup.rs); B itself when nothing is left out.
#[allow(clippy::too_many_arguments)]
fn apart(b: El, more: usize, pi: usize, agent: &str, e: &Entry, key: &str, head: &str, k: &Kit) -> El {
    let whole = if e.patch.is_some() { "the whole diff" } else if e.kind == "tool" { "the whole call" } else { "the whole result" };
    let title = crate::popup::title(&format!("{agent}'s {head}"), &crate::when::at(&e.at), whole);
    crate::popup::more(b, more, title, crate::popup::What::Entry { pi, agent: agent.to_string(), key: key.to_string() }, k)
}

/// Entry KEY of AGENT's session in project PI whole, as its popup shows it: an edit's whole diff, or a tool call's or
/// result's whole text; None when the session held no longer has it. APART is the popup's (its text's key, its comment
/// source): the passage he is commenting on in it is lit there (popup.rs `source`).
pub fn whole(s: &App, pi: usize, agent: &str, key: &str, apart: (&str, &str), k: &Kit) -> Option<El> {
    let p = s.projects.get(pi)?;
    let c = p.convs.get(agent)?;
    let e = c.entry(key)?;
    Some(match &e.patch {
        Some(patch) => {
            let src = format!("```diff\n{patch}\n```");
            // the entry's comments as its row shows them, except one being written there
            let entry = format!("{agent}@{}", e.offset);
            let mut marks = if s.notes.picked.as_ref().is_some_and(|x| x.source == entry) { Vec::new() } else { crate::ask::marks(s, p, &entry, &src, k) };
            marks.extend(crate::ask::marks(s, p, apart.1, &src, k));
            let diff = |src: String, marks: Vec<crate::ask::Mark>| Markdown::new(src).highlights(marks).selectable(true).build(k);
            // the comment box's room under the lit line, the diff's fence closed before it and opened after it
            match crate::ask::cut(s, apart.1, &src, crate::ask::Here::default()) {
                Some(cut) => crate::ask::split(s, apart.1, &src, &marks, cut, &format!("{}-diff", apart.0), |part, marks, id| diff(part, marks).key(id)),
                None => diff(src, marks),
            }
        }
        None => crate::ask::lit_words(s, apart.0, apart.1, &e.text, |w| text(w).font(k.ty.mono).color(ink(e, k)).wrap_anywhere().selectable(true), k),
    })
}

/// Whether entry E is his, drawn on the right (window rules §6): a prompt typed into the agent's terminal, or a
/// delivery whose every message is the owner's. Mail from agents and the agent's own words stand on the left.
fn his(e: &Entry) -> bool {
    match e.kind.as_str() {
        "user" => true,
        "mail" => {
            let m = crate::ask::delivered(&e.text);
            !m.is_empty() && m.iter().all(|(_, from, _, _)| from == "owner")
        }
        _ => false,
    }
}

/// The words of a coord channel entry (`<channel ...>\nWORDS\n</channel>`) and its `ticket`, if its tag names one.
fn channel_words(t: &str) -> Option<(String, Option<String>)> {
    let rest = t.strip_prefix("<channel source=\"coord\"")?;
    let (tag, body) = rest.split_once('>')?;
    let ticket = tag.split("ticket=\"").nth(1).and_then(|r| r.split('"').next()).map(String::from);
    let body = body.trim_end();
    Some((body.strip_suffix("</channel>").unwrap_or(body).trim().to_string(), ticket))
}

/// The thread his message E to AGENT opened: one to AGENT with these words, reached it latest at or before E.
fn thread_of<'a>(p: &'a crate::Proj, agent: &str, e: &Entry) -> Option<&'a coord_types::TicketView> {
    let (words, ticket) = channel_words(&e.text)?;
    let views = &p.live.tickets;
    if let Some(id) = ticket {
        return views.values().find(|v| v.id == id && v.kind == "thread");
    }
    let reached = |v: &coord_types::TicketView| -> Option<u64> {
        let on = match moved_to(v) {
            Some(to) => views.values().find(|x| x.id == to)?,
            None => v,
        };
        on.given.iter().filter(|g| g.to == agent).filter_map(|g| g.place.as_ref()).map(|l| l.offset).filter(|o| *o <= e.offset).max()
    };
    views
        .values()
        .filter(|v| v.kind == "thread" && v.form["to"].as_str() == Some(agent) && v.form["text"].as_str().map(str::trim) == Some(words.as_str()))
        .filter_map(|v| reached(v).map(|o| (o, v)))
        .max_by_key(|(o, _)| *o)
        .map(|(_, v)| v)
}

/// The ticket thread V's words were placed on (its result `Moved`), if they were.
fn moved_to(v: &coord_types::TicketView) -> Option<String> {
    match &v.status {
        docket_machine::ViewStatus::Done { result } if result["head"] == "Moved" => result["to"].as_str().map(String::from),
        _ => None,
    }
}

fn one_off(v: &coord_types::TicketView) -> bool {
    matches!(&v.status, docket_machine::ViewStatus::Done { result } if result["head"] == "OneOff")
}

/// The one-offs to AGENT just before thread V and V itself, oldest first: back to the last thread to AGENT that is not
/// a one-off (the run `Keep` takes, consecutive by the machine's rule).
fn one_off_run(p: &crate::Proj, agent: &str, v: &coord_types::TicketView) -> Vec<String> {
    let mut run = vec![v.id.clone()];
    for x in p.live.tickets.values().rev().filter(|x| x.n < v.n && x.kind == "thread" && x.form["to"].as_str() == Some(agent)) {
        if !one_off(x) {
            break;
        }
        run.insert(0, x.id.clone());
    }
    run
}

/// Under his message to AGENT: what became of it, and the one click that overrides it (Keep, DropThread, Move).
fn verdict_chip(pi: usize, p: &crate::Proj, agent: &str, e: &Entry, k: &Kit) -> Option<El> {
    let v = thread_of(p, agent, e)?;
    let act = move |label: String, req: Request| {
        Button::new(label)
            .variant(ButtonVariant::Ghost)
            .on_press(move |s: &mut App| {
                s.request_in(pi, req.clone(), crate::Pending::TicketDone { clear: crate::threads::Cleared::Act });
            })
            .build(k)
    };
    let say = |t: String| text(t).font(k.ty.small).color(k.c.text_muted);
    let mut chip = row().gap(8.0).items_center();
    let unnamed = v.form["addressed"]["head"] == "Unnamed";
    match &v.status {
        docket_machine::ViewStatus::Done { .. } if one_off(v) => {
            chip = chip.child(say("one-off: nothing kept but this chat".into())).child(act("Make this a ticket".into(), Request::Keep { ts: vec![v.id.clone()] }));
            let run = one_off_run(p, agent, v);
            if run.len() > 1 {
                chip = chip.child(act(format!("…with the {} before it", run.len() - 1), Request::Keep { ts: run }));
            }
        }
        docket_machine::ViewStatus::Done { .. } if moved_to(v).is_some() => {
            let to = moved_to(v).unwrap_or_default();
            // the owner's words there: the last post of the owner's with them; moved on since (Move), its stub says where
            let words = v.form["text"].as_str().unwrap_or("").trim().to_string();
            let same = |q: &&docket_machine::Post| q.by == "owner" && matches!(&q.text, docket_machine::Text::New(w) if w.trim() == words);
            let there = p.live.tickets.values().find(|x| x.id == to);
            let post = there.and_then(|x| x.posts.iter().rev().filter(same).find(|q| q.moved.is_none()).map(|q| q.n));
            let on = there.and_then(|x| x.posts.iter().rev().filter(same).find_map(|q| q.moved.clone()));
            match (post, on) {
                (Some(n), _) => {
                    chip = chip.child(say(format!("put on {to}")));
                    chip = chip.child(act("Not there: a thread of its own".into(), Request::Move { t: to, n: n as u32, to: None }));
                }
                (None, Some(on)) => chip = chip.child(say(format!("put on {to}, then moved to {on}"))),
                (None, None) => chip = chip.child(say(format!("put on {to}"))),
            }
        }
        docket_machine::ViewStatus::Resting => {
            chip = chip.child(say(format!("thread {}", v.id)));
            if unnamed {
                chip = chip.child(act("Drop: just a one-off".into(), Request::DropThread { t: v.id.clone() }));
            }
        }
        _ => chip = chip.child(say(format!("thread {}", v.id))),
    }
    Some(chip)
}

/// A note to the agent (and those @tagged): Send (Cmd/Ctrl+Enter); while the agent is in a turn, at the row's left end
/// Stop (Esc), or with words in the box Interrupt & Send (Esc) (outgoing.rs `interrupt`).
pub(crate) fn composer(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    // its session restarting (a Safe or Yolo change, line 25; a model, effort or account one): the box gives way to a
    // line saying so until the new session is up, what he had written in it kept for then
    if let Some(words) = crate::settings::restarting(a) {
        return col().w_full().px(16.0).py(12.0).child(text(words).font(k.ty.small).color(k.c.text_muted));
    }
    let running = a.state != "stopped";
    let target = crate::message_box::Target::Agent { project: pi, name: a.name.clone() };
    // in a turn: Stop (Esc) alone, or Interrupt & Send (Esc) with words in the box, apart at the row's left end
    let left = if a.state == "working" {
        let who = a.name.clone();
        let (label, help) = if crate::outgoing::written(s, pi, &a.name) {
            ("Interrupt & Send (Esc)", "Send and stop its turn")
        } else {
            ("Stop (Esc)", "Stop its turn; nothing is sent")
        };
        row().child(Button::new(label).variant(ButtonVariant::Ghost).help(help).on_press(move |s: &mut App| crate::outgoing::interrupt(s, pi, &who)).build(k).key(format!("composer-esc-{}", a.name)))
    } else {
        row().child(text(if running { "" } else { "stopped: your message waits for its next start" }).font(k.ty.small).color(k.c.text_faint))
    };
    // the box says whom; how it sends is behind "?" at its end
    let right = vec![crate::message_box::help(s, &target, "Cmd/Ctrl+Enter sends; @ tags another agent", k)];
    let actions = crate::message_box::Actions { left: Some(left), right, send: Some("Send".into()), placeholder: crate::look::say(s, "to_agent", &[("agent", &a.name)]) };
    // his messages on their way above the box (outgoing.rs), coordd's answer under it
    col().w_full().px(16.0).py(12.0).gap(6.0).children(crate::outgoing::dock(s, pi, &target.place(), k)).child(crate::message_box::build(s, &target, &target, actions, k)).children(crate::message_box::said(s, pi, &target.place(), k))
}

#[cfg(test)]
mod tests {
    // a chat opened and left alone asks no earlier page; scrolled to its top, one
    #[test]
    fn a_chat_asks_an_earlier_page_only_when_he_is_at_its_top() {
        use crate::live::Conv;
        let mut c = Conv { loaded: true, start: 180_000, ..Default::default() };
        let n = 20;
        let mut asked = 0;
        // opened: the view built before the page came reports no rows, then it is laid out at its end and stays there
        asked += super::wants_older(&mut c, 0..0, 0) as u32;
        for r in [14..20, 14..20, 15..20] {
            asked += super::wants_older(&mut c, r, n) as u32;
        }
        assert_eq!(asked, 0, "left alone: no earlier page");
        // he scrolls up to its top
        for r in [9..15, 3..9, 1..7] {
            asked += super::wants_older(&mut c, r, n) as u32;
        }
        assert_eq!(asked, 1, "at its top: one");
        // a chat whose rows all fit asks at once, having no top to scroll to
        let mut short = Conv { loaded: true, start: 5_000, ..Default::default() };
        assert!(super::wants_older(&mut short, 0..4, 4));
    }

    /// The first open timed in an offscreen window against a made-up bridge (`READ_BACK_BRIDGE`), left `FIRST_OPEN_AT` s.
    #[test]
    #[ignore]
    fn first_open_still() {
        let (Ok(out), Ok(_)) = (std::env::var("STILL_OUT"), std::env::var("READ_BACK_BRIDGE")) else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "standin".into(), id: "standin".into(), boxed: false }, Vec::new());
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Agent("dev".into());
            s.streams.which.insert((0, "dev".into()), crate::streams::Which::Work);
            s
        }
        fn setup(s: &mut crate::App, wake: rune::Waker) {
            let cmd = vec!["python3".to_string(), "-I".into(), std::env::var("READ_BACK_BRIDGE").unwrap()];
            s.projects[0].link = Some(crate::link::Link::start(cmd, wake.clone(), wake));
        }
        fn poll(s: &mut crate::App, _: f64, tx: &mut rune::Tx) -> bool {
            let ms: Vec<_> = s.projects[0].link.as_ref().map(|l| l.drain()).unwrap_or_default();
            let changed = !ms.is_empty();
            for m in ms {
                s.take(0, m);
            }
            s.projects[0].open_conv("dev");
            tx.wake_after(std::time::Duration::from_millis(50));
            changed
        }
        let at = std::env::var("FIRST_OPEN_AT").unwrap_or_else(|_| "5".into());
        let args = vec!["--realtime".to_string(), "--snap".into(), out.join("first-open.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), poll: Some(poll), setup: Some(setup), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    /// A chat that grew while hidden asks nothing until it shows again; then its last page, with the faint line while
    /// it is asked, and it draws at its newest entry.
    #[test]
    fn a_chat_that_grew_while_hidden_reads_its_last_page_when_it_shows() {
        use crate::view_cost::{linked_app, page_asked, page_reply};
        use crate::live::Conv;
        let (_held, mut s, _cat) = linked_app(&["dev"]);
        s.select_agent("dev");
        let id = page_asked(&s, "dev").expect("opened: read");
        s.take(0, page_reply(id, 50, 300, &[100, 200], &[]));
        s.sel = crate::Sel::Tickets;
        let mut a = s.projects[0].live.agents[0].clone();
        a.last_active = Some("2026-11-18T11:00:00Z".into());
        s.take(0, crate::link::Incoming::Event(coord_types::Event { seq: 2, body: coord_types::Body::Agent(a) }));
        assert!(page_asked(&s, "dev").is_none(), "hidden: nothing asked as it grows");
        s.select_agent("dev");
        let id = page_asked(&s, "dev").expect("shown again: its last page asked");
        assert_eq!(super::top_line(s.projects[0].convs.get("dev")), "reading the session…");
        s.take(0, page_reply(id, 300, 400, &[300], &[]));
        assert_eq!(super::top_line(s.projects[0].convs.get("dev")), "", "no faint line");
        s.streams.which.insert((0, "dev".into()), crate::streams::Which::Work);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        rt.frame(0.016);
        assert!(rt.find("e300-0").is_some(), "its newest entry drawn");
        assert!(!rt.elements().iter().any(|e| e.text.as_deref() == Some("reading the session…")), "no reading line");
        // the bridge down, nothing asked, nothing said
        let c = Conv { loaded: true, start: 10, catch_up: true, newer_pending: true, ..Default::default() };
        assert_eq!(super::top_line(Some(&Conv { newer_pending: false, ..c })), "");
    }

    /// A made-up chat of agent "orch" whose system prompt he edited from turn 7 at 10:12 and again with Now at 10:30
    /// (path fix 6 point 3): line 2's page as coordd sends it with his edits, and line 1's part after the cut, which a
    /// press reads (`Pending::Fold`). For the test below and its stills.
    fn restarted_chat() -> (coord_types::Transcript, coord_types::Transcript) {
        use coord_types::{Entry, Transcript};
        use docket_machine::graph::{Branched, Restarted};
        let e = |offset: u64, at: &str, kind: &str, tool: Option<&str>, text: &str| Entry {
            offset,
            at: format!("2026-11-24T{at}:00Z"),
            kind: kind.into(),
            tool: tool.map(String::from),
            text: text.into(),
            patch: None,
            images: vec![],
        };
        let line = Branched { k: 2, from: "/p/s1.jsonl".into(), from_line: 1, turns: 7 };
        let restarted = vec![
            Restarted { offset: 2100, at: "2026-11-24T10:12:00Z".into(), system: "2".into(), line: Some(line) },
            Restarted { offset: 2700, at: "2026-11-24T10:30:00Z".into(), system: "3".into(), line: None },
        ];
        let two = vec![
            e(1500, "10:01", "user", None, "Read the record's last day and say what moved."),
            e(1600, "10:02", "tool", Some("Bash"), "{\"command\": \"tail -n 400 log.jsonl\"}"),
            e(1700, "10:02", "result", None, "400 lines"),
            e(1800, "10:05", "assistant", None, "Three threads moved: the delivery text, the window's history and the brain's first post."),
            e(1900, "10:06", "tool", Some("Read"), "{\"file_path\": \"notes.md\"}"),
            e(2000, "10:06", "result", None, "notes"),
            e(2100, "10:12", "user", None, "Go on from turn 7 with the new system prompt."),
            e(2200, "10:13", "assistant", None, "Going on: the summary again, in the shorter voice."),
            e(2300, "10:14", "tool", Some("Edit"), "{\"file_path\": \"summary.md\"}"),
            e(2400, "10:14", "result", None, "ok"),
            e(2500, "10:15", "assistant", None, "The summary is in summary.md, four lines."),
            e(2700, "10:31", "user", None, "Check the wording once more."),
            e(2800, "10:32", "assistant", None, "Done: two words changed."),
        ];
        let one = vec![
            e(2100, "10:08", "user", None, "Now the figures for the appendix."),
            e(2200, "10:09", "assistant", None, "Making the two figures from the record."),
            e(2300, "10:09", "tool", Some("Bash"), "{\"command\": \"python3 fig.py\"}"),
            e(2400, "10:10", "result", None, "fig/a.png fig/b.png"),
            e(2500, "10:10", "assistant", None, "The figures are in fig/; the second needs your word on its axis."),
            e(2600, "10:11", "user", None, "Use a log scale."),
            e(2650, "10:11", "assistant", None, "Switched the second to a log scale."),
        ];
        let page = Transcript { entries: two, start: 1500, next: 2900, session: "/p/c0ff.jsonl".into(), restarted, ..Default::default() };
        (page, Transcript { entries: one, start: 2100, next: 2700, session: "/p/s1.jsonl".into(), ..Default::default() })
    }

    /// His system-prompt edits stand as marks in the history; from a coordd that serves it, a press opens the old line.
    #[test]
    fn his_edit_from_a_turn_stands_where_it_fell_and_opens_the_old_line_beside_it() {
        use crate::view_cost::{linked_app, page_asked};
        use crate::work::Item;
        let reply = |id: u64, page: &coord_types::Transcript| crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out: serde_json::to_string(page).unwrap(), post: None });
        let (page, old) = restarted_chat();
        let (_held, mut s, _cat) = linked_app(&["orch"]);
        s.projects[0].protocol = coord_types::TRANSCRIPT_SESSION_PROTOCOL - 1;
        s.select_agent("orch");
        let id = page_asked(&s, "orch").expect("its page");
        s.take(0, reply(id, &page));
        let items = crate::work::items(&s.projects[0].convs["orch"], &Default::default(), |_| false);
        let want = [Item::Work { from: 4, to: 6 }, Item::Restarted { mark: 0, at: 6 }, Item::Entry(6)];
        assert_eq!(items[3..6], want, "turn 7's run, the mark, line 2's prompt under it");
        assert_eq!(items[items.len() - 3..], [Item::Restarted { mark: 1, at: 11 }, Item::Entry(11), Item::Entry(12)], "Now before the turn after it");
        s.streams.which.insert((0, "orch".into()), crate::streams::Which::Work);
        // tall enough for both lines, line 1 open, with no scroll
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 2400.0), rune::Env::new(false, 1.0));
        let frames = |rt: &mut rune::runtime::Runtime<crate::App>| {
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
        };
        let said = |rt: &rune::runtime::Runtime<crate::App>| rt.elements().into_iter().filter_map(|e| e.text).collect::<Vec<_>>();
        frames(&mut rt);
        let mark = format!("System prompt changed · {} · from turn 7", crate::when::at("2026-11-24T10:12:00Z"));
        let now = format!("System prompt changed · {}", crate::when::at("2026-11-24T10:30:00Z"));
        let words = said(&rt);
        assert!(words.contains(&mark) && words.contains(&now), "{words:?}");
        assert!(!words.iter().any(|t| t.contains("The old turns")), "an older coordd: no press, {words:?}");
        rt.update(|s, _| s.projects[0].toggle_fold("orch", "/p/s1.jsonl", 2100));
        assert!(!rt.state.projects[0].pending.values().any(|w| matches!(w, crate::Pending::Fold { .. })), "nothing asked of it");
        // a coordd at the number: the press, and line 1 asked by its path
        rt.update(|s, _| s.projects[0].protocol = coord_types::TRANSCRIPT_SESSION_PROTOCOL);
        frames(&mut rt);
        assert!(said(&rt).contains(&"▸ The old turns after turn 7".to_string()), "{:?}", said(&rt));
        let press = |rt: &mut rune::runtime::Runtime<crate::App>| {
            let (x, y) = rt.find("fold-0-orch-2100").expect("the press is drawn");
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            rt.frame(0.016);
        };
        press(&mut rt);
        let asked: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| matches!(w, crate::Pending::Fold { agent, from } if agent == "orch" && from == "/p/s1.jsonl")).map(|(id, _)| *id).collect();
        assert_eq!(asked.len(), 1, "line 1 asked once");
        frames(&mut rt);
        assert!(said(&rt).contains(&"reading the old turns…".to_string()), "{:?}", said(&rt));
        rt.update(|s, _| {
            s.take(0, reply(asked[0], &old));
        });
        frames(&mut rt);
        let words = said(&rt);
        for w in ["▾ The old turns after turn 7", "Making the two figures from the record.", "1 tool call", "Switched the second to a log scale.", "new system prompt from here", "Going on: the summary again, in the shorter voice."] {
            assert!(words.iter().any(|t| t.contains(w)), "{w}: {words:?}");
        }
        // folded again: nothing more asked, line 1's words gone
        press(&mut rt);
        frames(&mut rt);
        assert!(!rt.state.projects[0].pending.values().any(|w| matches!(w, crate::Pending::Fold { .. })));
        let words = said(&rt);
        assert!(words.contains(&"▸ The old turns after turn 7".to_string()) && !words.iter().any(|t| t.contains("Making the two figures")), "{words:?}");
    }

    /// Stills of his system-prompt edit (made up): folded, and open beside the mark.
    #[test]
    #[ignore]
    fn restarted_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn base(open: bool) -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            s.notice = None;
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            p.live.agents = vec![serde_json::from_value(serde_json::json!({"id": "AGT-1", "name": "orch", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle",
                "since": "2026-11-24T09:00:00Z", "last_active": "2026-11-24T10:32:00Z", "tools": [], "items": []})).unwrap()];
            let (page, old) = restarted_chat();
            let c = p.convs.entry("orch".into()).or_default();
            c.merge(page, false);
            let f = c.folds.entry("/p/s1.jsonl".into()).or_default();
            f.conv.merge(old, false);
            f.open = open;
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Agent("orch".into());
            s.streams.which.insert((0, "orch".into()), crate::streams::Which::Work);
            s
        }
        // the open one taller, so the mark and its press stand above the old turns as well as their end under them
        let shots: [(&str, fn() -> crate::App, u32); 2] = [("restarted-folded.png", || base(false), 900), ("restarted-open.png", || base(true), 1500)];
        for (name, start, h) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), format!("1400x{h}"), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, h as f32), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    #[test]
    fn decoded_images_are_let_go_of_past_the_budget() {
        let cache = rune::ImageCache::new(std::sync::Arc::new(|| {}));
        let d = super::Decoded::default();
        // 64 by 64: 21845 bytes each with mips; 150000 holds six
        let imgs: Vec<rune::Image> = (0..10u8).map(|n| cache.rgba(64, 64, vec![n; 64 * 64 * 4])).collect();
        for (n, i) in imgs.iter().enumerate() {
            let _ = i.wait();
            d.keep_within(&format!("k{n}"), i, 150_000);
        }
        assert_eq!(d.keys(), ["k9", "k8", "k7", "k6", "k5", "k4"]);
        // one drawn again comes first, and the oldest goes
        d.keep_within("k6", &imgs[6], 150_000);
        d.keep_within("k0", &imgs[0], 150_000);
        assert_eq!(d.keys(), ["k0", "k6", "k9", "k8", "k7", "k5"]);
        // the newest few stay whatever they hold
        d.keep_within("big", &imgs[1], 0);
        assert_eq!(d.keys().len(), super::DECODED_FEW);
    }

    #[test]
    fn an_image_comes_as_its_bytes() {
        assert!(matches!(super::take_image(true, r#"{"media":"image/png","data":"iVBORw0KGgo="}"#), super::Img::Bytes(b) if b.len() == 8));
        assert!(matches!(super::take_image(false, "no"), super::Img::Failed(w) if w == "no"));
    }

    #[test]
    fn channel_headings_say_what_it_is() {
        let h = |t: &str| super::channel_heading(t);
        assert_eq!(h(r#"<channel source="coord" from="owner" id="N-44.p1" kind="note" ticket="N-44">hi</channel>"#).as_deref(), Some("message from you · N-44"));
        // one word, whatever the ticket's kind (0.2's C16)
        assert_eq!(h(r#"<channel source="coord" from="docket-core" id="N-45.p3" kind="thread" ticket="N-45">hi</channel>"#).as_deref(), Some("message from docket-core · N-45"));
        // the owner's words to an agent holding only them: who they are from, nothing else
        assert_eq!(h("<channel source=\"coord\" from=\"owner\">\nwhat is 5+5\n</channel>").as_deref(), Some("you"));
        assert_eq!(h("<channel source=\"coord\" from=\"minder\">\ndo this\n</channel>").as_deref(), Some("from minder"));
        assert_eq!(super::channel_words("<channel source=\"coord\" from=\"owner\">\nwhat is 5+5\n</channel>"), Some(("what is 5+5".to_string(), None)));
        assert_eq!(super::channel_words("<channel source=\"coord\" from=\"owner\">\nwhat is 5+5\n</channel>\n"), Some(("what is 5+5".to_string(), None)), "a newline after the tag");
        assert_eq!(h(r#"<channel source="coord" from="coord" id="C-12.s1" kind="claim" step="1" ticket="C-12">x"#).as_deref(), Some("step C-12.s1"));
        assert_eq!(h(r#"<channel source="coord" from="coord" id="C-12.c" kind="claim" ticket="C-12">x"#).as_deref(), Some("from coord · C-12"));
        assert_eq!(h(r#"<channel source="coord" from="lead" id="m10">x"#), None);
        // one message joining several: how many threads, not old mail with an id made of three
        assert_eq!(h(r#"<channel source="coord" from="coord" id="N-412.p851 N-591.p40 C-12.s3" kind="handover">x"#).as_deref(), Some("3 threads"));
        assert_eq!(h(r#"<channel source="coord" again="second hand-over" from="owner" id="N-944.p3 N-950.p7" kind="owner">x"#).as_deref(), Some("you on 2 threads"));
        let mails = crate::ask::delivered("<channel source=\"coord\" from=\"coord\" id=\"N-412.p851 N-591.p40\" kind=\"handover\">\n2 threads, one after another.\n</channel>");
        assert_eq!(mails.len(), 1);
        assert_eq!(mails[0].3.as_deref(), Some("2 threads"), "a ticket's card, not old mail's");
    }

    /// Research's made-up session for the brain's passes (each_brain_pass_that_spoke_stands_in_its_agents_chat_at_the_time_it_spoke):
    /// paper-research's entries at offsets 100 to 800, on 22 Nov 10:00–10:10 and 24 Nov 17:50–18:02 UTC, a run of
    /// work at 17:55–17:56.
    fn brain_session() -> coord_types::Transcript {
        let e = |offset: u64, at: &str, kind: &str, tool: Option<&str>, text: &str| coord_types::Entry { offset, at: at.into(), kind: kind.into(), tool: tool.map(String::from), text: text.into(), patch: None, images: vec![] };
        let entries = vec![
            e(100, "2026-11-22T10:00:00Z", "user", None, "Read docket-core's head before the train."),
            e(200, "2026-11-22T10:10:00Z", "assistant", None, "Read: it holds; my Right is on it."),
            e(300, "2026-11-24T17:50:00Z", "user", None, "go on with the engine page"),
            e(400, "2026-11-24T17:55:00Z", "tool", Some("Bash"), "coord show N-1446"),
            e(500, "2026-11-24T17:56:00Z", "result", None, "N-1446 …"),
            e(600, "2026-11-24T17:58:00Z", "assistant", None, "The engine page's move 6 is posted."),
            e(700, "2026-11-24T18:01:00Z", "user", None, "and the proof he asked for?"),
            e(800, "2026-11-24T18:02:00Z", "assistant", None, "Sent it on N-1104."),
        ];
        coord_types::Transcript { entries, start: 100, next: 900, session: "s".into(), ..Default::default() }
    }

    /// Pass B-N of AGENT's brain, a pass on PASS, closed at AT with RESULT; SAID: the post it made then, none when it
    /// said nothing; MENTIONS: the ids that post names, as coordd fills them.
    fn brain_pass(n: u64, agent: &str, pass: serde_json::Value, at: &str, result: serde_json::Value, said: Option<&str>, mentions: &[&str]) -> coord_types::TicketView {
        let id = format!("B-{n}");
        let posts = said.map_or(serde_json::json!([]), |t| serde_json::json!([{"n": 1, "by": id, "at": at, "text": t, "names": [agent]}]));
        let mentions = if mentions.is_empty() { serde_json::json!({}) } else { serde_json::json!({"1": mentions}) };
        serde_json::from_value(serde_json::json!({"id": id, "n": n, "kind": "brain", "opener": agent, "ctx": {}, "form": {"agent": agent, "pass": pass},
            "title": "one pass of an agent's brain", "status": {"is": "done", "result": result}, "children": [], "program": "", "bound": 2, "spent": 1,
            "opened_at": at, "closed_at": at, "given": [], "posts": posts, "mentions": mentions}))
        .unwrap()
    }

    /// B-7's words are research's B-1604's (its record, 24 Nov 18:00:09 UTC); the rest made up.
    const NEXT: &str = "N-1104.p1014 passed me his p1013 ask. I’ll check whether he got the proof, then send it or say what’s still missing.";
    const KEPT: &str = "On: the 0.4 engine page, move 6 posted (N-1446.p2906).\nOwed: accounts-lead's holder-1 notes, unanswered (N-1587.p2).\nSaid I would: the by-part baseline before 77 goes in (H-1545.p4).";

    /// The passes a day old or less, the snapshot's: B-7 (Next after 20 turns, 18:00:09), B-8 (Same, nothing said), B-9
    /// (Kept at a compaction, 18:10), and docket-core's B-11 (18:00:30).
    fn todays_passes() -> Vec<coord_types::TicketView> {
        let ahead = serde_json::json!({"head": "Ahead", "turns": 20});
        let next = |thought: &str, rests_on: &str| serde_json::json!({"head": "Next", "goal": "the engine page", "critic": "", "memory": "", "attention": "", "act": thought, "rests_on": rests_on, "thought": thought, "notes": ""});
        vec![
            brain_pass(7, "paper-research", ahead.clone(), "2026-11-24T18:00:09Z", next(NEXT, "N-1104.p1014"), Some(NEXT), &["N-1104.p1014"]),
            brain_pass(8, "paper-research", ahead.clone(), "2026-11-24T18:00:40Z", serde_json::json!({"head": "Same", "goal": "", "critic": "", "memory": "", "attention": "", "notes": ""}), None, &[]),
            brain_pass(9, "paper-research", serde_json::json!({"head": "Compacting"}), "2026-11-24T18:10:00Z", serde_json::json!({"head": "Kept", "text": KEPT}), Some(KEPT), &["N-1446.p2906", "N-1587.p2", "H-1545.p4"]),
            brain_pass(11, "docket-core", ahead, "2026-11-24T18:00:30Z", next("Answer the read on H-1601.", "H-1601.p3"), Some("Answer the read on H-1601."), &["H-1601.p3"]),
        ]
    }

    /// Passes closed more than a day ago, which no snapshot holds: B-3 (21 Nov, before the session held) and B-5 (22 Nov
    /// 10:05, inside it); and B-7 again, as coordd lists every pass.
    fn older_passes() -> Vec<coord_types::TicketView> {
        let ahead = serde_json::json!({"head": "Ahead", "turns": 20});
        let next = |thought: &str, rests_on: &str| serde_json::json!({"head": "Next", "goal": "", "critic": "", "memory": "", "attention": "", "act": thought, "rests_on": rests_on, "thought": thought, "notes": ""});
        let (b3, b5) = ("I said I'd read the syntax page first; I'll do that now.", "docket-core's head waits on my read before the train; I'll read it now.");
        vec![
            brain_pass(3, "paper-research", ahead.clone(), "2026-11-21T09:00:00Z", next(b3, "N-1446.p1900"), Some(b3), &["N-1446.p1900"]),
            brain_pass(5, "paper-research", ahead, "2026-11-22T10:05:00Z", next(b5, "N-1446.p2100"), Some(b5), &["N-1446.p2100"]),
            todays_passes().remove(0),
        ]
    }

    /// Project 0 as research: paper-research with a brain and its session ([`brain_session`]), docket-core with none of
    /// its own read, today's passes in the snapshot, from a coordd at BRAIN_PROTOCOL.
    fn brain_research(s: &mut crate::App) {
        let p = &mut s.projects[0];
        p.protocol = coord_types::BRAIN_PROTOCOL;
        for a in p.live.agents.iter_mut().filter(|a| a.name == "paper-research") {
            a.brain = Some(coord_types::BrainView { on: true, since: None, last_pass: Some("2026-11-24T18:10:00Z".into()), last_id: Some("B-9".into()), ready: Vec::new() });
        }
        p.convs.entry("paper-research".into()).or_default().merge(brain_session(), false);
        for v in todays_passes() {
            p.live.tickets.insert(v.n, v);
        }
    }

    /// Each brain pass that spoke stands in its agent's chat at its time; passes past the snapshot are asked once.
    #[test]
    fn each_brain_pass_that_spoke_stands_in_its_agents_chat_at_the_time_it_spoke() {
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["docket-core", "paper-research"]);
        brain_research(&mut s);
        // a link whose requests are kept, to read what went
        let file = std::env::temp_dir().join(format!("yaaihh-brain-passes-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut keep = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", file.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(keep.stdin.take().unwrap()));
        let keys = |s: &crate::App| {
            let c = &s.projects[0].convs["paper-research"];
            super::items(s, 0, "paper-research", c).iter().map(|it| super::item_key(c, it)).collect::<Vec<_>>()
        };
        assert_eq!(keys(&s), ["e100-0", "e200-0", "e300-0", "w-e400-0", "e600-0", "pass-7", "e700-0", "e800-0", "pass-9"], "the snapshot's passes at their times");
        // what the window asked of coordd for the passes, each with its id, read once the request is written
        let asked = |n: usize| -> Vec<u64> {
            let read = || std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).filter(|l| l["req"] == serde_json::json!({"op": "tickets", "filter": {"kind": "brain"}})).filter_map(|l| l["id"].as_u64()).collect::<Vec<_>>();
            for _ in 0..100 {
                if read().len() >= n {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            read()
        };
        let mut rt = crate::view_cost::window(s);
        // an older coordd sends no brain and is asked nothing for it
        rt.update(|s, tx| {
            s.projects[0].protocol = coord_types::BRAIN_PROTOCOL - 1;
            crate::poll(s, 0.0, tx);
        });
        assert!(asked(1).is_empty(), "nothing asked of an older coordd");
        rt.update(|s, tx| {
            s.projects[0].protocol = coord_types::BRAIN_PROTOCOL;
            crate::poll(s, 0.0, tx);
            crate::poll(s, 0.0, tx);
        });
        let ids = asked(1);
        assert_eq!(ids.len(), 1, "every pass asked once its pipe is up, once: {ids:?}");
        let out = older_passes().iter().map(|v| serde_json::to_string(v).unwrap()).collect::<Vec<_>>().join("\n");
        rt.update(|s, _| _ = s.take(0, crate::link::Incoming::Reply(ids[0], coord_types::Reply { ok: true, out, post: None })));
        assert_eq!(keys(&rt.state), ["e100-0", "pass-5", "e200-0", "e300-0", "w-e400-0", "e600-0", "pass-7", "e700-0", "e800-0", "pass-9"], "B-5 at its time, B-3 nowhere, B-7 once");
        // on its card: the last rows of its chat, B-7 and B-9 among them, each its ◆, what it was a pass on and its words' first line
        rt.update(|s, _| {
            s.sel = crate::Sel::Agents;
            s.agents_tab = 0;
        });
        for _ in 0..10 {
            rt.frame(0.016);
        }
        let texts = |rt: &rune::runtime::Runtime<crate::App>, key: &str| rt.elements().into_iter().filter(|e| e.path.contains(key)).filter_map(|e| e.text).collect::<Vec<_>>();
        let on_card = texts(&rt, "paper-research-pass-7");
        for w in ["◆", "its brain", "after 20 turns", NEXT] {
            assert!(on_card.iter().any(|t| t == w), "{w:?} on its card: {on_card:?}");
        }
        let kept = texts(&rt, "paper-research-pass-9");
        assert!(kept.iter().any(|t| t == "at its compaction") && kept.iter().any(|t| t == "On: the 0.4 engine page, move 6 posted (N-1446.p2906). …"), "B-9 on its card: {kept:?}");
        assert!(texts(&rt, "tile-docket-core").iter().all(|t| t != "its brain"), "docket-core's card has none of paper-research's passes");
        // on its page, its chat: a press opens B-7's words whole, what it rested on and the pass, which opens
        rt.update(|s, _| {
            s.select_agent("paper-research");
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        });
        for _ in 0..10 {
            rt.frame(0.016);
        }
        let (y5, y7, y8) = (rt.find("e600-0").expect("its answer at 17:58").1, rt.find("pass-7").expect("B-7 on its page").1, rt.find("e700-0").expect("the prompt at 18:01").1);
        assert!(y5 < y7 && y7 < y8, "B-7 drawn between 17:58 and 18:01: {y5} {y7} {y8}");
        let press = |rt: &mut rune::runtime::Runtime<crate::App>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            for _ in 0..5 {
                rt.frame(0.016);
            }
        };
        assert!(texts(&rt, "pass-words-7").is_empty(), "its words folded");
        press(&mut rt, "pass-7");
        let open = texts(&rt, "pass-words-7");
        for w in ["rested on", "the pass", "B-7"] {
            assert!(open.iter().any(|t| t == w), "{w:?} under B-7: {open:?}");
        }
        assert!(open.iter().filter(|t| t.contains("N-1104.p1014")).count() >= 2, "its words whole and what it rested on, each naming N-1104.p1014: {open:?}");
        let (x, y) = rt.elements().into_iter().find(|e| e.path.contains("pass-words-7") && e.text.as_deref() == Some("the pass")).map(|e| (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0)).expect("the pass");
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&crate::Sel::Tickets, Some(7)), "the pass opened");
        // the link let go, so the `cat` keeping its requests ends (view_cost.rs's writing test)
        rt.state.projects[0].link = None;
        let _ = keep.wait();
        let _ = std::fs::remove_file(&file);
    }

    /// Stills of research's made-up brain passes ([`brain_research`], the older ones fetched): paper-research's card on
    /// the Agents page, B-7 and B-9 in its chat's last rows (brain-pass-card.png); its page's chat with B-7 open, its
    /// words, what it rested on and the pass (brain-pass-open.png).
    /// `STILL_OUT=DIR yaaihh-tests chat::tests::brain_pass_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn brain_pass_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn app() -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.live.agents = ["docket-core", "paper-research"]
                .iter()
                .enumerate()
                .map(|(i, n)| {
                    serde_json::from_value(serde_json::json!({"id": format!("AGT-{i}"), "name": n, "parent": "owner", "kind": "claude", "model": "claude-opus-5-5", "state": "idle",
                        "since": "2026-11-24T18:02:00Z", "last_active": format!("2026-11-24T18:0{i}:00Z"), "tools": [], "items": []}))
                    .unwrap()
                })
                .collect();
            s.projects = vec![p];
            s.cur = 0;
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            brain_research(&mut s);
            s.tickets.find.fetched.entry("research".into()).or_default().extend(older_passes().into_iter().map(|v| (v.n, v)));
            s
        }
        fn card() -> crate::App {
            let mut s = app();
            s.sel = crate::Sel::Agents;
            s.agents_tab = 0;
            s
        }
        fn page() -> crate::App {
            let mut s = app();
            s.select_agent("paper-research");
            s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
            s.expanded.insert("pass-7".into());
            s
        }
        for (name, start) in [("brain-pass-card.png", card as fn() -> crate::App), ("brain-pass-open.png", page)] {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
