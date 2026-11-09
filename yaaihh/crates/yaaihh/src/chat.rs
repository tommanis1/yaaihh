//! An agent's page: what it is doing, its session as a conversation (read in pages from coordd, older ones as the
//! top comes into view), and the composer that mails it, or stops its turn and mails it (esc); or, switched to
//! Terminal, its raw terminal at full size (term.rs). Text selected in the conversation can be commented on, and the
//! comments go with the next mail; a question the agent asked shows as its card (ask.rs).

use crate::live::Conv;
use crate::App;
use coord_types::{AgentView, Entry, Request, Transcript};
use rune::{El, GrowthSpec, ScrollSpec, TextEdit, col, row, text};
use rune_kit::markdown::Markdown;
use rune_kit::{Badge, Button, ButtonVariant, Kit, SegmentedControl, TextArea};
use crate::work::{self, Item};
use std::borrow::Cow;
use std::rc::Rc;

/// Lines of a tool result or call shown before it is opened.
const SHORT: usize = 4;

/// Agent NAME of project PI at full size: its chat or its terminal ("Chat | Terminal", kept per agent), with the
/// composer. Any pane of the split shows one (main.rs `view`), so nothing here reads the current project.
pub fn pane(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let Some(a) = p.live.agents.iter().find(|a| a.name == name) else {
        return col().padding(24.0).child(text(format!("{name} is not an agent of this box")).font(k.ty.body).color(k.c.text_muted));
    };
    let key = (pi, name.to_string());
    let shows_term = s.term.pages.contains(&key);
    let switch = SegmentedControl::new(&["Chat", "Terminal"], Some(shows_term as usize))
        .label("Show")
        .fit()
        .on_pick(move |s: &mut App, i, _| {
            if i == 1 {
                s.term.pages.insert(key.clone());
            } else {
                s.term.pages.remove(&key);
            }
        })
        .build(k);
    let head = col().w_full().child(row().w_full().px(24.0).pt(10.0).child(switch)).child(facts(a, k)).child(commits(s, pi, name, k)).child(rune_kit::Divider::horizontal().build(k));
    if shows_term {
        return col().w_full().h_full().child(head).child(col().w_full().grow(1.0).min_h(0.0).padding(12.0).child(crate::term::view(s, k, pi, name, true)));
    }
    let conv = p.convs.get(name);
    // the chat's rows: what is said in view, the work between folded (work.rs)
    let items = Rc::new(conv.map(|c| items(s, pi, c)).unwrap_or_default());
    let n = items.len();
    let (who, kit) = (name.to_string(), k.clone());
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
            move |s: &App, i| item_row(s, pi, &who, &items[i], i + 1 == n, &kit),
        )
        .on_visible_rows({
            let who = name.to_string();
            move |s: &mut App, r| {
                if r.start > 2 {
                    return false;
                }
                s.projects.get_mut(pi).map(|p| p.read_older(&who));
                false
            }
        });
    let top = match conv {
        Some(c) if c.older_pending => "reading earlier entries…".to_string(),
        Some(c) if c.loaded && c.start == 0 => "the session's start".to_string(),
        Some(Conv { refused: Some(why), .. }) => format!("not read: {why}"),
        Some(c) if !c.loaded => "reading the session…".to_string(),
        _ => String::new(),
    };
    let target = name.to_string();
    let transcript = col()
        .key(format!("conv-{pi}-{name}"))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: true })
        .selection_group(true)
        .selection_menu_item("comment", "Comment", move |s: &mut App, sel, _| crate::ask::comment_on(s, pi, &target, sel))
        .on_highlight_press(|s: &mut App, key, _| crate::ask::highlight_pressed(s, key))
        .child(rows);
    col()
        .w_full()
        .h_full()
        .child(head)
        .child(row().px(24.0).child(text(top).font(k.ty.small).color(k.c.text_faint)))
        .child(transcript)
        .child(queued(s, pi, a, k))
        .child(rune_kit::Divider::horizontal().build(k))
        .child(composer(s, pi, a, k))
}

/// The owner's mails to the agent that have not reached it yet, at the chat's foot, greyed until they are delivered:
/// a click (or Edit) takes one back into the composer, Cancel takes it back, Send now stops the agent's turn and
/// delivers it.
fn queued(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let waiting: Vec<&coord_types::MailView> = p
        .live
        .mail
        .iter()
        .filter(|m| m.from == "owner" && m.to.contains(&a.name) && !m.delivered.contains(&a.name) && !m.withdrawn.contains(&a.name))
        // one its message net delivered others in place of is not waiting: those are
        .filter(|m| !m.clerk.as_deref().is_some_and(|c| c.starts_with("delivered as")))
        .collect();
    if waiting.is_empty() {
        return col();
    }
    let why = match a.state.as_str() {
        "stopped" => format!("{} is stopped: delivered when it starts", a.name),
        "working" => format!("queued: delivered when {}'s turn ends", a.name),
        _ => "being delivered…".to_string(),
    };
    let cards = waiting.into_iter().map(|m| {
        let (edit, cancel, now, open) = ((m.id.clone(), a.name.clone()), (m.id.clone(), a.name.clone()), m.id.clone(), (m.id.clone(), a.name.clone()));
        let mut acts = row()
            .gap(4.0)
            .child(Button::new("Edit").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.unsend(pi, &edit.1, &edit.0, true)).build(k))
            .child(Button::new("Cancel").variant(ButtonVariant::GhostMuted).on_press(move |s: &mut App| s.unsend(pi, &cancel.1, &cancel.0, false)).build(k));
        if a.state == "working" && m.clerk.is_none() {
            acts = acts.child(Button::new("Send now").variant(ButtonVariant::Warning).on_press(move |s: &mut App| s.send_now(pi, &now)).build(k));
        }
        // sent back by its message net (the clerk): the owner may have it delivered as it was
        if m.clerk.as_deref().is_some_and(|c| c.starts_with("sent back")) {
            let id = m.id.clone();
            acts = acts.child(Button::new("Deliver anyway").variant(ButtonVariant::Warning).on_press(move |s: &mut App| {
                s.mail_in(pi, coord_types::Request::Release { id: id.clone() });
            }).build(k));
        }
        let why = match m.clerk.as_deref() {
            None => why.clone(),
            Some("with the clerk") => format!("the clerk is reading it before it goes to {}", a.name),
            Some(c) if c.starts_with("sent back") => format!("the clerk {c} — edit it, or deliver it as it is"),
            Some(c) => format!("the clerk: {c}"),
        };
        let body = k.interactive(
            col()
                .grow(1.0)
                .shrink()
                .gap(4.0)
                .child(text(format!("{} · {} · {}", m.id, crate::domain::local(&m.at), why)).font(k.ty.small).color(k.c.text_faint).wrap())
                .child(text(m.body.trim().to_string()).font(k.ty.body).color(k.c.text_muted).wrap().max_lines(4)),
        )
        .help("Click to take it back into the box and edit it")
        .on_press(move |s: &mut App| s.unsend(pi, &open.1, &open.0, true));
        row()
            .key(format!("queued-{}", m.id))
            .w_full()
            .gap(8.0)
            .padding(10.0)
            .radius(k.radius.md)
            .border(1.0, k.c.border_weak)
            .fill(k.c.layer_01)
            .opacity(0.6)
            .child(body)
            .child(acts)
    });
    col().key(format!("queued-{pi}-{}", a.name)).w_full().px(24.0).pb(8.0).gap(6.0).children(cards.collect::<Vec<_>>())
}

/// The agent's latest commits; a click shows the session where it made it.
fn commits(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let mine: Vec<crate::Commit> = s.projects.get(pi).map_or(&[][..], |p| &p.commits).iter().filter(|c| c.agent == name).take(6).cloned().collect();
    if mine.is_empty() {
        return col();
    }
    let chips = mine.into_iter().map(|c| {
        let label = format!("{} {}", &c.hash[..7.min(c.hash.len())], c.subject);
        k.interactive(
            row().max_w(360.0).px(8.0).py(4.0).radius(k.radius.sm).border(1.0, k.c.border_weak).child(text(label).font(k.ty.small).color(k.c.text_muted).truncate()),
        )
        .on_press(move |s: &mut App| {
            s.focus_on(pi, &c.agent);
            s.jump(c.clone())
        })
    });
    row().key("commits").w_full().scroll_x().child(row().px(24.0).pb(8.0).gap(6.0).child(text("commits").font(k.ty.small_medium).color(k.c.text_faint)).children(chips))
}

/// What the agent is: state, model, items, running tools, memory.
fn facts(a: &AgentView, k: &Kit) -> El {
    let mut parts = vec![format!("{} · {} · {} · since {}", a.id, a.kind, a.model, crate::domain::local(&a.since))];
    if !a.items.is_empty() {
        parts.push(format!("works on {}", a.items.join(", ")));
    }
    parts.extend(a.mem_mb.map(|m| format!("{m} MB")));
    let r = parts.into_iter().fold(
        row().w_full().px(24.0).py(10.0).gap(8.0).center_y().child(Badge::new(a.state.clone()).variant(rune_kit::BadgeVariant::Accent).build(k)),
        |r, t| r.child(text(t).font(k.ty.small).color(k.c.text_muted)),
    );
    let mut c = col().w_full().child(r);
    for t in &a.tools {
        c = c.child(row().px(24.0).pb(6.0).child(text(format!("running {} · {}", t.tool, t.what)).font(k.ty.small).color(k.c.text_faint).truncate()));
    }
    c
}


/// The rows of AGENT's conversation C in project PI (work.rs), the runs the owner opened open.
pub fn items(s: &App, pi: usize, c: &Conv) -> Vec<Item> {
    let p = s.projects.get(pi);
    work::items(c, &s.expanded, |t| p.is_some_and(|p| crate::ask::asked(p, t).is_some()))
}

/// The element key of ITEM: its entry's, its run's or its call's.
pub fn item_key(c: &Conv, item: &Item) -> String {
    match *item {
        Item::Entry(i) => c.keys[i].clone(),
        Item::Work { from, .. } => work::run_key(c, from),
        Item::Call { call, .. } => work::call_key(c, call),
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
    let live = last && a.is_some_and(|a| a.state == "working");
    // a row drawn once and reused until it changes (Rune's subtree cache): a frame for the spinner of the run going
    // on, or for anything else that moves, no longer repaints every row's text; the live run
    // itself, whose spinner changes 30 times a second, draws live
    let shown: Vec<usize> = match *item {
        Item::Entry(i) => vec![i],
        Item::Work { from, to } => (from..to.min(c.entries.len())).collect(),
        Item::Call { call, result } => std::iter::once(call).chain(result).collect(),
    };
    // a row is built again only when what it reads changes (Rune's memo): a keystroke in the composer, or a new entry
    // below, reuses it as it is
    let mut v = crate::version::Version::default();
    // a mail's or coord ask's row also shows its thread and its reply box (ask.rs)
    if let Item::Entry(i) = *item && matches!(c.entries[i].kind.as_str(), "mail" | "result") {
        crate::ask::thread_version(s, p, None, &mut v);
    }
    v.add((&c.session, live, cwd, s.expanded.contains(&key)));
    for &i in &shown {
        let (e, ek) = (&c.entries[i], &c.keys[i]);
        v.add((ek, s.expanded.contains(ek), s.found.as_deref() == Some(ek.as_str())));
        for n in &e.images {
            match s.images.get(&image_key(agent, &c.session, e.offset, *n)) {
                Some(Img::Ready(img)) => v.add((1u8, img.size(), img.error().map(|e| e.to_string()))),
                Some(Img::Failed(why)) => v.add((2u8, why)),
                Some(Img::Asked) | None => v.add(0u8),
            };
        }
        crate::ask::marks_version(s, p, &format!("{agent}@{}", e.offset), v.hasher());
    }
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
        Item::Call { call, result } => {
            let line = work::call_line(s, c, call, result, k);
            if s.expanded.contains(&key) {
                let shown = std::iter::once(call).chain(result).map(|i| entry(s, pi, agent, &c.entries[i], &c.keys[i], cwd, k).key(c.keys[i].clone()).pl(20.0));
                col().key(key).w_full().child(line).children(shown.collect::<Vec<_>>())
            } else {
                line.key(key)
            }
        }
    };
    if live { row } else { row.cache() }
}

/// One entry of the session (also a tile's chat tail, agents.rs).
/// An entry of AGENT's session in project PI. CWD is the agent's folder: what a relative path in its text means
/// (paths.rs).
/// A chat's image: asked of coordd, decoded (or decoding), or why not.
pub enum Img {
    Asked,
    Ready(rune::Image),
    Failed(String),
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
    let page = Transcript { entries: c.entries.clone(), start: c.start, next: c.next, session: c.session.clone() };
    image_requests(agent, &page, &Default::default())
}

/// coordd's answer to an image request (`{"media", "data"}`), decoded by CACHE.
pub fn take_image(cache: Option<&rune::ImageCache>, ok: bool, out: &str) -> Img {
    use base64::Engine;
    if !ok {
        return Img::Failed(out.to_string());
    }
    let data = serde_json::from_str::<serde_json::Value>(out).ok().and_then(|v| Some(v.get("data")?.as_str()?.to_string()));
    match (cache, data.and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok())) {
        (Some(c), Some(bytes)) => Img::Ready(c.load(bytes)),
        (None, _) => Img::Failed("no image decoder".into()),
        (_, None) => Img::Failed("not an image".into()),
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
    let mut out = col().gap(8.0);
    for n in &e.images {
        let key = image_key(agent, &session, e.offset, *n);
        out = out.child(match s.images.get(&key) {
            Some(Img::Ready(img)) if img.error().is_none() => {
                let w = img.size().map_or(IMAGE_MAX_W, |[w, _]| (w as f32 / 2.0).min(IMAGE_MAX_W));
                rune::el().w(w).radius(k.radius.sm).border(1.0, k.c.border_weak).pixels(img.pixels())
            }
            Some(Img::Ready(img)) => text(format!("image: {}", img.error().map(|e| e.to_string()).unwrap_or_default())).font(k.ty.small).color(k.c.text_faint),
            Some(Img::Failed(why)) => text(format!("image: {why}")).font(k.ty.small).color(k.c.text_faint),
            Some(Img::Asked) | None => text("image: reading…").font(k.ty.small).color(k.c.text_faint),
        });
    }
    Some(out)
}

pub fn entry(s: &App, pi: usize, agent: &str, e: &Entry, key: &str, cwd: Option<&str>, k: &Kit) -> El {
    let proj = s.projects.get(pi);
    // `coord ask`'s answer: the question it made, as its card
    if e.kind == "result" {
        if let Some((p, q)) = proj.and_then(|p| Some((p, crate::ask::asked(p, &e.text)?))) {
            return col().w_full().px(24.0).py(8.0).child(crate::ask::card(s, p, q, k));
        }
    }
    // mail: each mail as its card, with its thread
    if e.kind == "mail" {
        if let Some(m) = proj.and_then(|p| crate::ask::mail_entry(s, p, agent, &e.text, &e.at, k)) {
            return col().w_full().px(24.0).py(8.0).child(m);
        }
    }
    // the comments on this entry, highlighted where they stand
    let source = format!("{agent}@{}", e.offset);
    let marks = |src: &str| proj.map(|p| crate::ask::marks(s, p, &source, src, k)).unwrap_or_default();
    let md = |t: &str| {
        let cwd = cwd.map(String::from);
        let src = crate::paths::linkify(t);
        let marks = marks(&src);
        Markdown::new(src).highlights(marks).selectable(true).on_link(move |_: &mut App, url| crate::paths::open(url, cwd.clone())).build(k)
    };
    let label = |t: String, ink| row().gap(6.0).child(text(t).font(k.ty.small_medium).color(ink)).child(text(crate::domain::local(&e.at)).font(k.ty.small).color(k.c.text_faint));
    let body = match e.kind.as_str() {
        "assistant" => col().gap(4.0).child(md(&e.text)),
        "user" | "mail" => {
            let (who, ink) = if e.kind == "mail" { ("mail", k.c.text_accent) } else { ("prompt", k.c.text_muted) };
            col()
                .gap(6.0)
                .child(label(who.into(), ink))
                .child(col().w_full().padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(md(&e.text)))
        }
        _ if e.patch.is_some() => {
            // an edit: the change it made, as a diff (Claude Code's on the result, Codex's on the apply_patch call)
            let patch = e.patch.as_deref().unwrap_or_default();
            let head = if e.kind == "tool" { e.tool.clone().unwrap_or_default() } else { "diff".into() };
            col().w_full().gap(4.0).padding(8.0).child(label(head, k.c.text_muted)).child({
                let src = format!("```diff\n{patch}\n```");
                let marks = marks(&src);
                Markdown::new(src).highlights(marks).selectable(true).build(k)
            })
        }
        _ => {
            // a tool call or its result: a few lines, the rest on a click
            let open = s.expanded.contains(key);
            let lines: Vec<&str> = e.text.lines().collect();
            let shown = if open || lines.len() <= SHORT { e.text.clone() } else { format!("{}\n… {} more lines", lines[..SHORT].join("\n"), lines.len() - SHORT) };
            let head = e.tool.clone().unwrap_or_else(|| "result".into());
            let key = key.to_string();
            k.interactive(
                col()
                    .w_full()
                    .gap(4.0)
                    .padding(8.0)
                    .radius(k.radius.sm)
                    .child(label(head, k.c.text_muted))
                    .child(text(shown).font(k.ty.mono).color(if e.kind == "tool" { k.c.text_base } else { k.c.text_muted }).wrap_anywhere()),
            )
            .on_press(move |s: &mut App| {
                if !s.expanded.remove(&key) {
                    s.expanded.insert(key.clone());
                }
            })
        }
    };
    let body = match images(s, pi, agent, e, k) {
        // pasted images with no words: the label and the images, no empty text box
        Some(imgs) if e.text.trim().is_empty() && (e.kind == "user" || e.kind == "mail") => col().gap(6.0).child(label("prompt".into(), k.c.text_muted)).child(imgs),
        Some(imgs) => col().w_full().gap(6.0).child(body).child(imgs),
        None => body,
    };
    let row = col().w_full().px(24.0).py(8.0).child(body);
    // the entry a commit was found at
    if s.found.as_deref() == Some(key) { row.fill(k.c.surface_raised).border(1.0, k.c.border_focus) } else { row }
}

/// Mail to the agent: Send (Cmd/Ctrl+Enter) puts it in its inbox; Esc stops its turn first.
fn composer(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let running = a.state != "stopped";
    let key = (pi, a.name.clone());
    let empty = TextEdit::new("");
    let (edit, submit, send, esc) = (key.clone(), key.clone(), key.clone(), key.clone());
    let (f1, f2) = (crate::ask::Field::Composer(pi, a.name.clone()), crate::ask::Field::Composer(pi, a.name.clone()));
    let field = TextArea::new(s.composers.get(&key).unwrap_or(&empty))
        .width(None)
        .placeholder(format!("Mail to {} (Cmd/Ctrl+Enter sends; @ tags another agent)", a.name))
        .auto_grow(2, 10)
        .on_edit(move |s: &mut App| s.composers.entry(edit.clone()).or_insert_with(|| TextEdit::new("")))
        .on_change(move |s: &mut App, _| crate::ask::follow_mention(s, &f1))
        .on_key(move |s: &mut App, key, _| crate::ask::mention_key(s, &f2, key))
        .on_submit(move |s: &mut App, _| s.send(submit.0, &submit.1, false))
        .build(k);
    let field = col().key(crate::ask::Field::Composer(pi, a.name.clone()).key()).w_full().child(field);
    let send = Button::new("Send").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| s.send(send.0, &send.1, false)).build(k);
    let esc = Button::new("Esc and send").variant(ButtonVariant::Warning).on_press(move |s: &mut App| s.send(esc.0, &esc.1, true)).build(k);
    let mut buttons = row().gap(8.0).child(send);
    if running {
        buttons = buttons.child(esc);
    }
    col()
        .key(format!("composer-{pi}-{}", a.name))
        .w_full()
        .px(16.0)
        .py(12.0)
        .gap(8.0)
        .child(crate::ask::bar(s, pi, &a.name, k))
        .child(field)
        .child(row().w_full().justify_between().center_y().child(text(if running { "" } else { "stopped: mail waits for its next start" }).font(k.ty.small).color(k.c.text_faint)).child(buttons))
}
