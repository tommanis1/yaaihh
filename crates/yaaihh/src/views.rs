//! The project's views, drawn from the project file (design §3: kinds, columns and cards are data) with each source's
//! last JSON. The kinds: `overview` (no source of its own: drawn from the views it names), `board` (the items, as a
//! grouped list or as cards in columns), `stages` (the runner), `commits`, `decisions` (domain.rs draws these), `lens`
//! (a template: templates.rs); any other kind shows its data as it came, or with no source says it needs a newer
//! window. And the box's mail.

use crate::domain::{self, Detail};
use crate::{App, Proj};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::{Badge, Kit};
use serde_json::Value;

/// View ID (empty: the project's home) with its title.
pub fn view(s: &App, p: &Proj, id: &str, k: &Kit) -> (String, El) {
    let id = if id.is_empty() { p.live.home() } else { id.to_string() };
    // a project whose file has no home view (a project made in New project has no file): its Home (home.rs), which
    // every project has as its first tab
    if id == crate::global::ID {
        return ("Home".into(), crate::global::page(s, Some(p), k));
    }
    if id == crate::global::USAGE {
        return ("Usage".into(), crate::global::usage(s, k));
    }
    if id == crate::policy::ID {
        return ("Settings".into(), crate::policy::page(s, p, k));
    }
    if id.is_empty() || id == crate::home::ID {
        return ("Overview".into(), crate::home::page(s, p, k));
    }
    let Some(spec) = p.live.view_specs().into_iter().find(|v| str_at(v, "id") == id) else {
        return (id.clone(), note(&format!("the project has no view {id}"), k));
    };
    let title = spec.get("title").and_then(Value::as_str).unwrap_or(&id).to_string();
    // the page is built again only when what it shows changes: its data, the project's agents, mail and commits, and
    // what the owner set on it; the times it shows (a running step's seconds, "today") move at most every 10 s
    let mut v = crate::version::Version::default();
    let pi = s.place_of(p);
    v.add((&id, p.live.views_rev, p.live.mail.rev, s.show_closed, pi, s.notes.show_all, &s.open_mail, k.dark));
    v.debug(spec).debug(&s.sel).debug(&s.domain).add((p.commits.len(), p.commits.first().map(|c| &c.hash), p.older.len(), &p.older_from, p.older_pending));
    for a in &p.live.agents {
        v.add((&a.name, &a.items));
    }
    // the item tickets: a board's items, their titles and whether they are closed (E5)
    for t in p.live.tickets.values().filter(|t| t.kind == "item") {
        v.add((t.n, &t.title, t.closed_at.is_some()));
    }
    v.add(chrono::Utc::now().timestamp() / 10);
    // a template's answers (templates.rs)
    v.add(s.templates.rev);
    let page = rune::memo::memo_with_hash(format!("page-{pi}-{id}"), v.finish(), || page(s, p, spec, &id, k));
    (title, col().w_full().h_full().child(page))
}

/// View ID's page, as [`view`] memoises it.
fn page(s: &App, p: &Proj, spec: &Value, id: &str, k: &Kit) -> El {
    if str_at(spec, "kind") == "overview" {
        return domain::overview(s, p, spec, k);
    }
    if str_at(spec, "kind") == "lens" {
        return crate::templates::page(s, p, spec, id, k);
    }
    let data = p.live.views.get(id);
    // a board's items are the item tickets with what its source adds; a board with no source is the tickets alone
    let board = (str_at(spec, "kind") == "board").then(|| data.and_then(|d| d.data.as_ref())).and_then(|d| match d {
        Some(d) => Some(domain::with_item_tickets(p, spec, Some(d))),
        None if spec.get("source").is_none() => Some(domain::with_item_tickets(p, spec, None)),
        None => None,
    });
    let el = match (board.as_ref().or(data.and_then(|d| d.data.as_ref())), str_at(spec, "kind").as_str()) {
        // a kind this window does not draw, with no source to wait for: a newer window's (paper-research p1789, 3)
        (None, kind) if spec.get("source").is_none() && !["board", "stages", "commits", "decisions"].contains(&kind) => note(&format!("Needs a newer window."), k),
        (None, _) => note(data.and_then(|d| d.error.as_deref()).unwrap_or("waiting for the source's first output"), k),
        (Some(d), "board") => domain::items(s, p, spec, d, k),
        (Some(d), "stages") => domain::runner(s, p, spec, d, k),
        (Some(_), "commits") => domain::commits(s, p, k),
        (Some(d), "decisions") => domain::decisions(s, p, d, k),
        (Some(d), _) => col().padding(24.0).scroll(ScrollSpec { stick_bottom: false }).child(text(serde_json::to_string_pretty(d).unwrap_or_default()).font(k.ty.mono).wrap_anywhere()),
    };
    let el = match data.and_then(|d| d.error.as_ref()).filter(|_| data.is_some_and(|d| d.data.is_some())) {
        Some(e) => col().w_full().h_full().child(row().px(24.0).py(6.0).child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).truncate())).child(el),
        None => el,
    };
    el
}

pub fn note(t: &str, k: &Kit) -> El {
    col().padding(24.0).child(text(t.to_string()).font(k.ty.body).color(k.c.text_muted).wrap())
}

pub fn str_at(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(x) => x.to_string(),
    }
}

pub fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect()).unwrap_or_default()
}

/// A template's `{field}`s filled from CARD (its top-level fields, then its `fields`).
pub fn fill(template: &str, card: &Value) -> Option<String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        let j = rest[i..].find('}')? + i;
        let key = &rest[i + 1..j];
        let v = card.get(key).or_else(|| card.pointer(&format!("/fields/{key}"))).filter(|v| !v.is_null())?;
        out.push_str(&rest[..i]);
        out.push_str(&v.as_str().map(String::from).unwrap_or_else(|| v.to_string()));
        rest = &rest[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// Open a web link (http, https, mailto) in his browser: the only thing the window opens outside itself. Code, files,
/// repositories and commits open in the window (code.rs); anything else is logged and not opened.
pub fn open(target: String) {
    if !web_link(&target) {
        return crate::log::line(&format!("not opened (not a web link): {target}"));
    }
    // a test's window opens nothing: what it would open is kept for the test to read
    #[cfg(test)]
    return OPENED.lock().unwrap().push(target);
    #[allow(unreachable_code)]
    if cfg!(windows) {
        let _ = crate::places::command("explorer.exe").arg(target).spawn();
        return;
    }
    let _ = crate::places::command("xdg-open").arg(target).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

/// The web links a test's window would have opened in his browser.
#[cfg(test)]
pub(crate) static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// An http, https or mailto link with nothing in it a command line could take otherwise.
pub(crate) fn web_link(target: &str) -> bool {
    ["http://", "https://", "mailto:"].iter().any(|p| target.starts_with(p)) && !target.chars().any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

#[cfg(test)]
mod open_tests {
    #[test]
    fn only_web_links_open_on_windows() {
        assert!(super::web_link("https://example.org/a?b=c"));
        assert!(super::web_link("mailto:x@y.z"));
        assert!(!super::web_link("/home/t/notes.md"));
        assert!(!super::web_link("https://a b"));
        assert!(!super::web_link("file:///etc/passwd"));
    }
}

/// One column of cards.
fn column(title: &str, cards: Vec<El>, k: &Kit) -> El {
    let n = cards.len();
    col()
        .key(format!("col-{title}"))
        .w(300.0)
        .h_full()
        .gap(8.0)
        .child(row().gap(8.0).center_y().child(text(title.to_string()).font(k.ty.ui_medium).color(k.c.text_base)).child(Badge::new(n.to_string()).build(k)))
        .child(col().key(format!("cards-{title}")).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().gap(8.0).pb(16.0).children(cards)))
}

/// A board card; a click opens D in the detail panel.
fn card(k: &Kit, head: String, title: String, lines: Vec<String>, d: Detail, faint: bool) -> El {
    let c = col()
        .w_full()
        .gap(4.0)
        .padding(10.0)
        .radius(k.radius.md)
        .fill(if faint { k.c.bg_base } else { k.c.layer_01 })
        .border(1.0, k.c.border_weak)
        .child(text(head).font(k.ty.small_medium).color(k.c.text_muted))
        .child(text(title).font(k.ty.ui).color(if faint { k.c.text_muted } else { k.c.text_base }).wrap());
    let c = lines.into_iter().fold(c, |c, l| c.child(text(l).font(k.ty.small).color(k.c.text_faint).wrap()));
    k.interactive(c).on_press(move |s: &mut App| s.domain.detail = Some(d.clone()))
}

/// Items in the view's columns; the ones closed by hand (coordd) in the hidden columns, behind the items view's
/// toggle (domain.rs).
pub fn board(s: &App, p: &Proj, spec: &Value, data: &Value, k: &Kit) -> El {
    let mut columns = strings(spec.get("columns"));
    let hidden = strings(spec.get("hidden"));
    if s.show_closed {
        columns.extend(hidden.iter().cloned());
    }
    let fields = strings(spec.pointer("/card/fields"));
    let closed = p.live.closed();
    let items: Vec<&Value> = data.get("items").and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default();
    let cols = columns.iter().map(|c| {
        let cards = items
            .iter()
            .filter(|it| {
                let is_closed = closed.contains(str_at(it, "id").as_str());
                if hidden.contains(c) { is_closed } else { !is_closed && str_at(it, "column") == *c }
            })
            .map(|it| {
                let id = str_at(it, "id");
                let mut lines: Vec<String> = fields.iter().filter_map(|f| it.pointer(&format!("/fields/{f}")).and_then(Value::as_str).filter(|v| !v.is_empty()).map(|v| format!("{f}: {v}"))).collect();
                let workers: Vec<&str> = p.live.agents.iter().filter(|a| a.items.iter().any(|i| crate::live::same_item(i, &id))).map(|a| a.name.as_str()).collect();
                if !workers.is_empty() {
                    lines.insert(0, format!("worked on by {}", workers.join(", ")));
                }
                card(k, id.clone(), str_at(it, "title"), lines, Detail::Item(id), hidden.contains(c))
            })
            .collect();
        column(c, cards, k)
    });
    row().key("board").w_full().grow(1.0).min_h(0.0).scroll_x().child(row().h_full().px(24.0).gap(16.0).children(cols))
}

/// "Before tickets" at the foot of the Threads list (design part 9b; the Notes page is gone): the old questions still open
/// first, each as its card, answered or dismissed there (m1617), then the old mail, read only, a row opening in place.
pub fn before_tickets(s: &App, p: &Proj, k: &Kit) -> Vec<El> {
    let mut c: Vec<El> = Vec::new();
    let section = |t: &str| row().w_full().pt(10.0).pb(2.0).child(text(t.to_string()).font(k.ty.small_medium).color(k.c.text_muted).wrap());
    let waiting: Vec<&coord_types::MailView> = crate::ask::questions(p, false);
    if p.live.mail.is_empty() {
        return c;
    }
    c.push(section("BEFORE TICKETS"));
    if !waiting.is_empty() {
        c.push(section(&format!("Old questions still open ({}): answer or dismiss each here", waiting.len())));
        c.push(col().w_full().gap(12.0).children(waiting.into_iter().map(|q| crate::ask::card(s, p, q, k))));
        c.push(section("Old mail, read only"));
    }
    let rows = p.live.mail.iter().rev().map(|m| {
        let open = s.open_mail.as_deref() == Some(m.id.as_str());
        let id = m.id.clone();
        let mut head = row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(text(format!("{} → {}", m.from, m.to.join(", "))).font(k.ty.small_medium).color(k.c.text_muted))
            .child(text(crate::when::at(&m.at)).font(k.ty.small).color(k.c.text_faint));
        if m.esc {
            head = head.child(Badge::new("esc").variant(rune_kit::BadgeVariant::Accent).build(k));
        }
        if m.kind == "question" {
            head = head.child(Badge::new(m.state.clone().map_or("question".to_string(), |st| format!("question · {st}"))).build(k));
        }
        let mut c = col().w_full().gap(4.0).px(4.0).py(8.0).child(head).child(text(format!("{} · {}", m.id, m.subject)).font(k.ty.ui).color(k.c.text_base).truncate());
        // opened, the body below its line; a body that is only its subject is not said twice
        if open && m.body.trim() != m.subject.trim() {
            c = c.child(col().w_full().pt(6.0).child(crate::post::message_body(Some(p), &m.body, None, None, k)));
        }
        k.interactive(c.key(m.id.clone())).on_press(move |s: &mut App| s.open_mail = if s.open_mail.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) })
    });
    c.extend(rows);
    c
}

