//! A project's domain views: the Home overview (open items and the runner now), the items as a list grouped by owner with the board behind a
//! toggle, the runner drawn as lanes with step strips, its queue and its outcomes, the commits with filters, and the
//! decisions (rulings and the owner's answered questions). Every object (item, runner card, commit, ruling) opens in
//! one detail panel on the right, which links to the others, to the agents and to the questions.
//!
//! Read-only: nothing here changes the box; the actions open a file, an agent's session or the Questions inbox. The
//! data is each view's source JSON (a project's view source documents the shapes it gives); items, runner cards and
//! rulings are found through the project's view of their kind (`board`, `stages`, `decisions`).

use crate::views::{board, fill, note, open, str_at, strings};
use crate::{App, Commit, Proj, Sel};
use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::markdown::Markdown;
use rune_kit::{Button, ButtonVariant, Divider, Kit, SegmentedControl, Switch, TextInput};
use serde_json::Value;
use std::collections::BTreeMap;

/// What the domain views remember.
#[derive(Default, Debug)]
pub struct State {
    /// The items view shows the other layout than its project file's `layout` (the board, for a list view).
    pub board: bool,
    /// The object open in the detail panel.
    pub detail: Option<Detail>,
    /// The commits view's filters.
    pub agent: Option<String>,
    pub repo: Option<String>,
    pub item: Option<String>,
    /// The decisions view's search.
    pub query: TextEdit,
    /// The decisions view shows the answered questions instead of the rulings.
    pub answered: bool,
}

/// An object the detail panel shows.
#[derive(Clone, PartialEq, Debug)]
pub enum Detail {
    /// An item, by id.
    Item(String),
    /// A runner card, by its id (`queue AGENT`, `check NAME`).
    Card(String),
    /// A commit, by its hash or a prefix of it.
    Commit(String),
    /// A ruling, by number.
    Ruling(String),
}

/// A filter of the commits view.
#[derive(Clone, Copy)]
enum Filter {
    Agent,
    Repo,
    Item,
}

impl State {
    fn filter(&mut self, f: Filter) -> &mut Option<String> {
        match f {
            Filter::Agent => &mut self.agent,
            Filter::Repo => &mut self.repo,
            Filter::Item => &mut self.item,
        }
    }
}

// ---- data ---------------------------------------------------------------------------------------------------------

/// View ID's spec in the project file.
fn view_spec<'a>(p: &'a Proj, id: &str) -> Option<&'a Value> {
    p.live.view_specs().into_iter().find(|v| str_at(v, "id") == id)
}

/// The project's first view of KIND: its spec.
fn spec_of_kind<'a>(p: &'a Proj, kind: &str) -> Option<&'a Value> {
    p.live.view_specs().into_iter().find(|v| str_at(v, "kind") == kind)
}

/// View ID's source data.
fn data<'a>(p: &'a Proj, id: &str) -> Option<&'a Value> {
    p.live.views.get(id)?.data.as_ref()
}

/// The source data of the project's first view of KIND.
fn of_kind<'a>(p: &'a Proj, kind: &str) -> Option<&'a Value> {
    data(p, &str_at(spec_of_kind(p, kind)?, "id"))
}

/// The array at KEY of V (empty when there is none).
fn arr<'a>(v: Option<&'a Value>, key: &str) -> Vec<&'a Value> {
    v.and_then(|v| v.get(key)).and_then(Value::as_array).map(|a| a.iter().collect()).unwrap_or_default()
}

/// The runner card with ID.
fn find_card<'a>(p: &'a Proj, id: &str) -> Option<&'a Value> {
    arr(of_kind(p, "stages"), "cards").into_iter().find(|c| str_at(c, "id") == id)
}

/// The item with ID.
fn find_item<'a>(p: &'a Proj, id: &str) -> Option<&'a Value> {
    arr(of_kind(p, "board"), "items").into_iter().find(|i| str_at(i, "id") == id)
}

/// The agents whose current items include ID.
fn workers<'a>(p: &'a Proj, id: &str) -> Vec<&'a str> {
    p.live.agents.iter().filter(|a| a.items.iter().any(|i| i == id)).map(|a| a.name.as_str()).collect()
}

/// An item's state: closed by hand (coordd), else its derived column.
fn item_state(p: &Proj, it: &Value) -> String {
    if p.live.closed().contains(str_at(it, "id").as_str()) { "closed".into() } else { str_at(it, "column") }
}

// ---- time ---------------------------------------------------------------------------------------------------------

/// AT (RFC 3339, coordd writes UTC) in local time: the clock today, else the day and the clock.
pub(crate) fn local(at: &str) -> String {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(at) else { return at.to_string() };
    let t = t.with_timezone(&chrono::Local);
    if t.date_naive() == chrono::Local::now().date_naive() { t.format("%H:%M").to_string() } else { t.format("%-d %b %H:%M").to_string() }
}

/// A step's time: its `secs`, or for a running step (the source sends none, so its view does not change every few
/// seconds) the time since its start.
fn step_secs(st: &Value) -> Option<i64> {
    st.get("secs").and_then(Value::as_i64).or_else(|| {
        let start = chrono::DateTime::parse_from_rfc3339(st.get("start")?.as_str()?).ok()?;
        (str_at(st, "status") == "running").then(|| (chrono::Utc::now() - start.with_timezone(&chrono::Utc)).num_seconds())
    })
}

/// SECS shortly: `42 s`, `7 min`, `1 h 05`.
fn dur(secs: i64) -> String {
    match secs {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s => format!("{} h {:02}", s / 3600, s / 60 % 60),
    }
}

// ---- pieces -------------------------------------------------------------------------------------------------------

/// The colours of STATE (an item's column, a stage, a step's status, a result): (fill, ink, border).
fn tone(k: &Kit, state: &str) -> (rune::Color, rune::Color, rune::Color) {
    let t = match state {
        "landed" | "done" | "green" | "checked" | "answered" => &k.c.success,
        "rejected" | "failed" | "red" | "oom" => &k.c.danger,
        "in the runner" | "running" | "pre-check" | "batch" => &k.c.info,
        "moving" | "waiting" | "deferred" | "inconclusive" | "pending" | "stopped" | "not yet" => &k.c.warning,
        _ => return (k.c.layer_02, k.c.text_muted, k.c.border_weak),
    };
    (t.bg, t.fg, t.border)
}

/// A small label in STATE's colours.
fn pill(label: &str, state: &str, k: &Kit) -> El {
    let (bg, fg, border) = tone(k, state);
    row().px(7.0).py(1.0).radius(k.radius.sm).fill(bg).border(1.0, border).child(text(label.to_string()).font(k.ty.small_medium).color(fg))
}

/// A chip that opens D in the detail panel.
fn link(label: String, d: Detail, k: &Kit) -> El {
    k.interactive(row().px(7.0).py(2.0).radius(k.radius.sm).border(1.0, k.c.border_weak).child(text(label).font(k.ty.small).color(k.c.text_accent)))
        .on_press(move |s: &mut App| s.domain.detail = Some(d.clone()))
}

/// A chip that shows agent NAME's page.
fn agent_link(name: &str, k: &Kit) -> El {
    let n = name.to_string();
    k.interactive(row().px(7.0).py(2.0).radius(k.radius.sm).fill(k.c.layer_02).child(text(name.to_string()).font(k.ty.small_medium).color(k.c.text_base)))
        .on_press(move |s: &mut App| s.open_agent(&n, false))
}

/// A button that opens TARGET (a file or folder) with the desktop's handler.
fn open_button(label: &str, target: String, k: &Kit) -> El {
    Button::new(label).variant(ButtonVariant::Outline).on_press(move |_: &mut App| open(target.clone())).build(k)
}

/// A section's title with its count.
fn heading(t: &str, n: Option<usize>, k: &Kit) -> El {
    let r = row().w_full().gap(8.0).center_y().pt(8.0).child(text(t.to_string()).font(k.ty.ui_medium).color(k.c.text_base).truncate().shrink());
    match n {
        Some(n) => r.child(text(n.to_string()).font(k.ty.small).color(k.c.text_faint)),
        None => r,
    }
}

/// A titled list of ROWS, EMPTY in their place when there are none.
fn section(title: &str, rows: Vec<El>, empty: &str, k: &Kit) -> El {
    let n = rows.len();
    let c = col().w_full().gap(6.0).child(heading(title, Some(n), k));
    if rows.is_empty() { c.child(text(empty.to_string()).font(k.ty.small).color(k.c.text_faint)) } else { c.children(rows) }
}

/// LABEL value lines, the empty ones left out.
fn facts(pairs: Vec<(&str, String)>, k: &Kit) -> El {
    pairs.into_iter().filter(|(_, v)| !v.is_empty()).fold(col().w_full().gap(4.0), |c, (l, v)| {
        c.child(row().w_full().gap(8.0).child(text(l.to_string()).font(k.ty.small).color(k.c.text_faint).w(84.0)).child(text(v).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink()))
    })
}

/// A list row that opens D, marked while D is the one shown.
fn pressable(s: &App, body: El, d: Detail, k: &Kit) -> El {
    let shown = s.domain.detail.as_ref() == Some(&d);
    let body = body.w_full().px(10.0).py(6.0).radius(k.radius.sm);
    k.interactive(if shown { body.fill(k.c.surface_raised) } else { body }).on_press(move |s: &mut App| s.domain.detail = Some(d.clone()))
}

/// A COMMIT row (source.py's shape): hash, subject, state; it opens the commit.
fn commit_row(s: &App, c: &Value, k: &Kit) -> El {
    let hash = str_at(c, "hash");
    let state = str_at(c, "state");
    let body = row()
        .gap(8.0)
        .center_y()
        .child(text(hash.chars().take(9).collect::<String>()).font(k.ty.mono).color(k.c.text_muted))
        .child(text(str_at(c, "subject")).font(k.ty.small).color(k.c.text_base).truncate().grow(1.0).shrink())
        .child(pill(&state, &state, k));
    pressable(s, body, Detail::Commit(hash), k)
}

// ---- the detail panel ---------------------------------------------------------------------------------------------

/// MAIN, with the detail panel on its right while an object is open.
fn with_detail(s: &App, p: &Proj, main: El, k: &Kit) -> El {
    let r = row().w_full().h_full().child(col().grow(1.0).min_w(0.0).h_full().child(main));
    match &s.domain.detail {
        Some(d) => r.child(Divider::vertical().build(k)).child(panel(s, p, d, k)),
        None => r,
    }
}

fn panel(s: &App, p: &Proj, d: &Detail, k: &Kit) -> El {
    let (what, body) = match d {
        Detail::Item(id) => ("Item", item_detail(s, p, id, k)),
        Detail::Card(id) => ("In the runner", card_detail(s, p, id, k)),
        Detail::Commit(h) => ("Commit", commit_detail(s, p, h, k)),
        Detail::Ruling(n) => ("Ruling", ruling_detail(p, n, k)),
    };
    let close = Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.domain.detail = None).build(k);
    col()
        .key("detail")
        .w(if matches!(d, Detail::Ruling(_)) { 560.0 } else { 420.0 })
        .h_full()
        .child(row().w_full().px(16.0).pt(10.0).pb(6.0).justify_between().center_y().child(text(what).font(k.ty.small_medium).color(k.c.text_faint)).child(close))
        .child(col().key("detail-body").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(body.px(16.0).pb(24.0)))
}

/// An item: its note's fields, the agents on it, its commits, the checks that carried them, its questions.
fn item_detail(s: &App, p: &Proj, id: &str, k: &Kit) -> El {
    let Some(it) = find_item(p, id) else { return note(&format!("{id} is not in the items view"), k) };
    let state = item_state(p, it);
    let field = |f: &str| it.pointer(&format!("/fields/{f}")).and_then(Value::as_str).unwrap_or("").to_string();
    let agents: Vec<El> = workers(p, id).into_iter().map(|a| agent_link(a, k)).collect();
    let commits = arr(Some(it), "commits").into_iter().map(|c| commit_row(s, c, k)).collect();
    let checks = arr(Some(it), "checks")
        .into_iter()
        .map(|c| {
            let (name, st) = (str_at(c, "name"), str_at(c, "state"));
            let body = row()
                .gap(8.0)
                .center_y()
                .child(text(name.clone()).font(k.ty.small_medium).color(k.c.text_base).grow(1.0).shrink().truncate())
                .child(text(format!("{} · {}", str_at(c, "to"), local(&str_at(c, "at")))).font(k.ty.small).color(k.c.text_faint))
                .child(pill(&st, &st, k));
            let card = format!("check {name}");
            if find_card(p, &card).is_some() { pressable(s, body, Detail::Card(card), k) } else { body.w_full().px(10.0).py(6.0) }
        })
        .collect();
    let questions = crate::ask::questions(p, true)
        .into_iter()
        .filter(|q| q.subject.contains(id) || q.body.contains(id))
        .map(|q| {
            let st = q.state.clone().unwrap_or_default();
            let body = row().gap(8.0).center_y().child(text(format!("{} · {}", q.id, q.from)).font(k.ty.small).color(k.c.text_faint)).child(text(q.subject.clone()).font(k.ty.small).color(k.c.text_base).truncate().grow(1.0).shrink()).child(pill(&st, &st, k));
            k.interactive(body.w_full().px(10.0).py(6.0).radius(k.radius.sm)).on_press(|s: &mut App| {
                s.notes.show_all = true;
                s.sel = Sel::Questions;
            })
        })
        .collect();
    let mut agents_row = row().w_full().gap(6.0).flex_wrap();
    agents_row = if agents.is_empty() { agents_row.child(text("no agent works on it").font(k.ty.small).color(k.c.text_faint)) } else { agents_row.children(agents) };
    // the project file's card.open (an item's note, say)
    let target = spec_of_kind(p, "board").and_then(|v| v.pointer("/card/open")).and_then(Value::as_str).and_then(|t| fill(t, it));
    let mut c = col()
        .w_full()
        .gap(12.0)
        .child(row().gap(8.0).center_y().child(text(id.to_string()).font(k.ty.ui_medium).color(k.c.text_muted)).child(pill(&state, &state, k)))
        .child(text(str_at(it, "title")).font(k.ty.body_medium).color(k.c.text_base).wrap())
        .child(facts(vec![("owner", field("owner")), ("status", field("status")), ("next", field("next")), ("blocked by", field("blocked-by")), ("design", field("design"))], k))
        .child(col().w_full().gap(6.0).child(heading("Agents on it", None, k)).child(agents_row))
        .child(section("Commits", commits, "no commit names it", k))
        .child(section("Checks", checks, "no check of the last two weeks carried its commits", k))
        .child(section("Questions", questions, "no question names it", k));
    if let Some(t) = target {
        c = c.child(row().pt(4.0).child(open_button("Open the note", t, k)));
    }
    c
}

/// A runner card: what it is and where, its steps with times and logs, what it carries.
fn card_detail(s: &App, p: &Proj, id: &str, k: &Kit) -> El {
    let Some(c) = find_card(p, id) else { return note(&format!("{id} is not in the runner now"), k) };
    let stage = str_at(c, "stage");
    let at = str_at(c, "at");
    let steps = arr(Some(c), "steps")
        .into_iter()
        .map(|st| {
            let (name, status) = (str_at(st, "step"), str_at(st, "status"));
            let mut r = row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(rune::el().w(8.0).h(8.0).radius(4.0).fill(strip_ink(k, &status)))
                .child(text(name.clone()).font(k.ty.small_medium).color(if status == "skipped" { k.c.text_faint } else { k.c.text_base }).w(110.0))
                .child(text(status.clone()).font(k.ty.small).color(k.c.text_muted).grow(1.0));
            let mut times = Vec::new();
            times.extend(st.get("start").and_then(Value::as_str).map(local));
            times.extend(step_secs(st).map(dur));
            times.extend(st.get("peak_gb").and_then(Value::as_f64).map(|g| format!("{g:.1} GB")));
            r = r.child(text(times.join(" · ")).font(k.ty.small).color(k.c.text_faint));
            match st.get("log").and_then(Value::as_str) {
                Some(log) => {
                    let log = log.to_string();
                    r.child(Button::new("log").variant(ButtonVariant::GhostMuted).on_press(move |_: &mut App| open(log.clone())).build(k))
                }
                None => r,
            }
        })
        .collect();
    let carries = c.get("carries");
    let commits = arr(carries, "commits").into_iter().map(|x| commit_row(s, x, k)).collect();
    let items: Vec<El> = carries.and_then(|v| v.get("items")).map(|v| strings(Some(v))).unwrap_or_default().into_iter().map(|i| link(i.clone(), Detail::Item(i), k)).collect();
    let mut agent = row().gap(8.0).center_y();
    if let Some(a) = c.get("agent").and_then(Value::as_str) {
        agent = agent.child(agent_link(a, k));
    }
    let when = if at.is_empty() { String::new() } else { format!("{} ({})", local(&at), crate::ask::ago(&at)) };
    let mut out = col()
        .w_full()
        .gap(12.0)
        .child(row().gap(8.0).center_y().child(text(id.to_string()).font(k.ty.ui_medium).color(k.c.text_base)).child(pill(&stage, &stage, k)))
        .child(agent)
        .child(facts(
            vec![
                ("branch", str_at(c, "branch")),
                ("commit", str_at(c, "commit")),
                ("mode", str_at(c, "mode")),
                ("lane", str_at(c, "lane")),
                ("step", str_at(c, "step")),
                ("since", when),
                ("reason", str_at(c, "reason")),
            ],
            k,
        ))
        .child(section("Steps", steps, "no build yet", k))
        .child(section("Carries", commits, "no commits beyond dev", k));
    if !items.is_empty() {
        out = out.child(row().w_full().gap(6.0).flex_wrap().child(text("items").font(k.ty.small).color(k.c.text_faint)).children(items));
    }
    if let Some(b) = c.get("build").and_then(Value::as_str) {
        out = out.child(row().pt(4.0).child(open_button("Open the build folder", b.to_string(), k)));
    }
    out
}

/// A commit: what the commits view, the items and the runner know of it; its session when its agent is here.
fn commit_detail(s: &App, p: &Proj, hash: &str, k: &Kit) -> El {
    let full: Option<&Commit> = s.commits().iter().find(|c| c.hash.starts_with(hash));
    let cards = arr(of_kind(p, "stages"), "cards");
    let items = arr(of_kind(p, "board"), "items");
    let same = |r: &&Value| {
        let h = str_at(r, "hash");
        !h.is_empty() && (hash.starts_with(&h) || h.starts_with(hash))
    };
    // as the runner or an item knows it (repo, subject, state) when the commits view does not
    let known = cards.iter().flat_map(|c| arr(c.get("carries"), "commits")).chain(items.iter().flat_map(|i| arr(Some(*i), "commits"))).find(same);
    if full.is_none() && known.is_none() {
        return note(&format!("{hash} is not a commit the views know"), k);
    }
    let pick = |f: &str| full.map(|c| match f { "repo" => c.repo.clone(), "subject" => c.subject.clone(), _ => c.state.clone() }).filter(|v| !v.is_empty()).or_else(|| known.map(|r| str_at(r, f))).unwrap_or_default();
    let state = pick("state");
    let tagged: Vec<String> = full.map(|c| c.items.clone()).or_else(|| known.map(|r| strings(r.get("items")))).unwrap_or_default();
    let carriers: Vec<El> = cards.iter().filter(|c| arr(c.get("carries"), "commits").iter().any(same)).map(|c| link(str_at(c, "id"), Detail::Card(str_at(c, "id")), k)).collect();
    let mut out = col()
        .w_full()
        .gap(12.0)
        .child(row().gap(8.0).center_y().child(text(full.map_or(hash.to_string(), |c| c.hash.clone())).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere().shrink()).child(pill(&state, &state, k)))
        .child(text(pick("subject")).font(k.ty.body_medium).color(k.c.text_base).wrap())
        .child(facts(
            vec![
                ("repository", pick("repo")),
                ("agent", full.map(|c| c.agent.clone()).unwrap_or_default()),
                ("made", full.map(|c| format!("{} ({})", local(&c.at), crate::ask::ago(&c.at))).unwrap_or_default()),
                ("refs", full.map(|c| c.refs.clone()).unwrap_or_default()),
                ("session", full.map(|c| c.session.clone()).unwrap_or_default()),
            ],
            k,
        ));
    if !tagged.is_empty() {
        out = out.child(row().w_full().gap(6.0).flex_wrap().child(text("items").font(k.ty.small).color(k.c.text_faint)).children(tagged.into_iter().map(|i| link(i.clone(), Detail::Item(i), k))));
    }
    if !carriers.is_empty() {
        out = out.child(row().w_full().gap(6.0).flex_wrap().child(text("in the runner as").font(k.ty.small).color(k.c.text_faint)).children(carriers));
    }
    if let Some(c) = full.filter(|c| p.live.agents.iter().any(|a| a.name == c.agent)).cloned() {
        out = out.child(row().pt(4.0).child(Button::new("Show it in its session").variant(ButtonVariant::Outline).on_press(move |s: &mut App| s.jump(c.clone())).build(k)));
    }
    out
}

/// A ruling: its text.
fn ruling_detail(p: &Proj, number: &str, k: &Kit) -> El {
    let Some(r) = arr(of_kind(p, "decisions"), "rulings").into_iter().find(|r| str_at(r, "number") == number) else {
        return note(&format!("ruling {number} is not in the decisions view"), k);
    };
    let path = str_at(r, "path");
    col().w_full().gap(12.0).child(Markdown::new(str_at(r, "body")).selectable(true).build(k)).child(row().child(open_button("Open the file", path, k)))
}

// ---- items --------------------------------------------------------------------------------------------------------

/// One item in a list: id, title, the agents on it, its commits, its state.
fn item_row(s: &App, p: &Proj, it: &Value, k: &Kit) -> El {
    let id = str_at(it, "id");
    let state = item_state(p, it);
    let commits = arr(Some(it), "commits");
    let landed = commits.iter().filter(|c| str_at(c, "state") == "landed").count();
    let mut r = row()
        .gap(10.0)
        .center_y()
        .child(text(id.clone()).font(k.ty.mono).color(k.c.text_muted).w(64.0))
        .child(text(str_at(it, "title")).font(k.ty.small).color(k.c.text_base).truncate().min_w(160.0).grow(1.0).shrink());
    let on = workers(p, &id);
    if !on.is_empty() {
        r = r.child(text(on.join(", ")).font(k.ty.small_medium).color(k.c.text_accent));
    }
    if !commits.is_empty() {
        r = r.child(text(format!("{} commits · {landed} landed", commits.len())).font(k.ty.small).color(k.c.text_faint).truncate().shrink());
    }
    pressable(s, r.child(pill(&state, &state, k)), Detail::Item(id), k)
}

/// ITEMS grouped by their GROUP field, the groups in name order (those without it last); in a group, by the
/// order of COLUMNS (the view's columns), then id.
fn grouped(s: &App, p: &Proj, items: Vec<&Value>, group: &str, columns: &[String], k: &Kit) -> El {
    let mut groups: BTreeMap<(bool, String), Vec<&Value>> = BTreeMap::new();
    for it in items {
        let g = it.pointer(&format!("/fields/{group}")).and_then(Value::as_str).filter(|g| !g.is_empty() && *g != "none");
        groups.entry((g.is_none(), g.unwrap_or("").to_string())).or_default().push(it);
    }
    let place = |it: &Value| (columns.iter().position(|c| *c == item_state(p, it)).unwrap_or(columns.len()), str_at(it, "id"));
    groups.into_iter().fold(col().w_full().gap(4.0), |c, ((none, name), mut its)| {
        its.sort_by_key(|it| place(it));
        let title = if none { format!("no {group}") } else { name };
        c.child(heading(&title, Some(its.len()), k)).children(its.into_iter().map(|it| item_row(s, p, it, k)))
    })
}

/// The items view: the list grouped by the spec's `group` (or the board, by the toggle), closed ones behind theirs.
pub fn items(s: &App, p: &Proj, spec: &Value, d: &Value, k: &Kit) -> El {
    let board_first = str_at(spec, "layout") == "board";
    let show_board = board_first != s.domain.board;
    let labels = if board_first { ["Board", "List"] } else { ["List", "Board"] };
    let pick = SegmentedControl::new(&labels, Some(s.domain.board as usize)).label("Items shown as").fit().on_pick(|s: &mut App, i, _| s.domain.board = i == 1).build(k);
    let hidden = strings(spec.get("hidden"));
    let toggle = Switch::new(s.show_closed).label(format!("Show {}", hidden.join(", "))).on_toggle(|s: &mut App| s.show_closed = !s.show_closed).build(k);
    let bar = row().w_full().px(24.0).py(8.0).gap(16.0).center_y().child(pick).child(toggle);
    let body = if show_board {
        board(s, p, spec, d, k)
    } else {
        let closed = p.live.closed();
        let shown: Vec<&Value> = arr(Some(d), "items").into_iter().filter(|it| s.show_closed || !closed.contains(str_at(it, "id").as_str())).collect();
        let group = spec.get("group").and_then(Value::as_str).unwrap_or("owner");
        let mut columns = strings(spec.get("columns"));
        columns.extend(hidden);
        col().key("item-list").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(grouped(s, p, shown, group, &columns, k).px(24.0).pb(24.0))
    };
    with_detail(s, p, col().w_full().h_full().child(bar).child(body), k)
}

// ---- the runner ---------------------------------------------------------------------------------------------------

/// A step status's colour in a strip.
fn strip_ink(k: &Kit, status: &str) -> rune::Color {
    match status {
        "done" => k.c.success.fg,
        "failed" | "oom" => k.c.danger.fg,
        "running" => k.c.info.fg,
        "stopped" => k.c.warning.fg,
        "skipped" => k.c.layer_01,
        _ => k.c.layer_03,
    }
}

/// A card's steps as a strip: one segment per step, coloured by its status, its name and time on hover.
fn strip(c: &Value, k: &Kit) -> El {
    row().w_full().gap(2.0).children(arr(Some(c), "steps").into_iter().map(|st| {
        let status = str_at(st, "status");
        let time = step_secs(st).map(|x| format!(" · {}", dur(x))).unwrap_or_default();
        rune::el().h(8.0).grow(1.0).radius(2.0).fill(strip_ink(k, &status)).help(format!("{} · {status}{time}", str_at(st, "step")))
    }))
}

/// What a card carries, in a line: its commits and items.
fn carries_line(c: &Value) -> String {
    let n = arr(c.get("carries"), "commits").len();
    let items = c.pointer("/carries/items").map(|v| strings(Some(v))).unwrap_or_default();
    let mut parts = vec![format!("{n} commit{}", if n == 1 { "" } else { "s" })];
    if !items.is_empty() {
        parts.push(items.join(", "));
    }
    parts.join(" · ")
}

/// A lane (a pre-check slot or the batch): the cards in it, the first with its step strip and what it carries.
fn lane(s: &App, name: &str, here: Vec<&Value>, kinds: Option<&Value>, k: &Kit) -> El {
    let busy = !here.is_empty();
    let head = row().w_full().justify_between().center_y().child(text(name.to_string()).font(k.ty.small_medium).color(k.c.text_muted));
    let c = col().w(340.0).gap(6.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, if busy { k.c.border_focus } else { k.c.border_weak });
    let Some(&first) = here.first() else { return c.child(head).child(text("free").font(k.ty.small).color(k.c.text_faint)) };
    let step = str_at(first, "step");
    let running = arr(Some(first), "steps").into_iter().find(|st| str_at(st, "status") == "running");
    let now = running.and_then(step_secs).map(|x| format!(" · {}", dur(x))).unwrap_or_default();
    let done = arr(Some(first), "steps").into_iter().filter(|st| str_at(st, "status") == "done").count();
    let total = arr(Some(first), "steps").into_iter().filter(|st| str_at(st, "status") != "skipped").count();
    let mut body = c
        .child(head.child(pill(&str_at(first, "stage"), &str_at(first, "stage"), k)))
        .child(row().w_full().gap(8.0).center_y().child(text(str_at(first, "id")).font(k.ty.ui_medium).color(k.c.text_base).truncate().shrink()).child(kind_chip(kinds, &str_at(first, "build_kind"), k)))
        .child(text(format!("{} · {}", str_at(first, "agent"), str_at(first, "branch"))).font(k.ty.small).color(k.c.text_muted).truncate())
        .child(strip(first, k))
        .child(text(format!("{step}{now} · {done}/{total} steps done")).font(k.ty.small).color(k.c.text_base).wrap())
        .child(text(carries_line(first)).font(k.ty.small).color(k.c.text_faint).truncate());
    if here.len() > 1 {
        body = body.child(row().w_full().gap(6.0).flex_wrap().child(text("with").font(k.ty.small).color(k.c.text_faint)).children(here[1..].iter().map(|c| link(str_at(c, "agent"), Detail::Card(str_at(c, "id")), k))));
    }
    let d = Detail::Card(str_at(first, "id"));
    let shown = s.domain.detail.as_ref() == Some(&d);
    k.interactive(if shown { body.fill(k.c.surface_raised) } else { body }).on_press(move |s: &mut App| s.domain.detail = Some(d.clone()))
}

/// The lanes of SPEC with the cards of DATA in them.
fn lanes(s: &App, spec: Option<&Value>, d: Option<&Value>, k: &Kit) -> El {
    let cards = arr(d, "cards");
    let kinds = d.and_then(|d| d.pointer("/pipeline/kinds"));
    let names = spec.map(|v| strings(v.get("lanes"))).unwrap_or_default();
    row().w_full().gap(10.0).flex_wrap().children(names.into_iter().map(|n| {
        let here = cards.iter().copied().filter(|c| str_at(c, "lane") == n).collect();
        lane(s, &n, here, kinds, k)
    }))
}

/// A card in a list: its place, agent, branch, items, commits, stage and age; it opens the card.
fn card_row(s: &App, c: &Value, place: Option<usize>, k: &Kit) -> El {
    let stage = str_at(c, "stage");
    let at = str_at(c, "at");
    let mut r = row().gap(10.0).center_y();
    if let Some(i) = place {
        r = r.child(text(format!("{}", i + 1)).font(k.ty.small).color(k.c.text_faint).w(20.0));
    }
    let who = if str_at(c, "kind") == "check" { str_at(c, "id") } else { str_at(c, "agent") };
    let r = r
        .child(text(who).font(k.ty.small_medium).color(k.c.text_base).w(150.0).truncate())
        .child(text(str_at(c, "branch")).font(k.ty.small).color(k.c.text_muted).truncate().grow(1.0).shrink())
        .child(text(carries_line(c)).font(k.ty.small).color(k.c.text_faint).truncate().max_w(220.0))
        .child(pill(&stage, &stage, k))
        .child(text(crate::ask::ago(&at)).font(k.ty.small).color(k.c.text_faint).w(72.0));
    pressable(s, r, Detail::Card(str_at(c, "id")), k)
}

/// The queue entries still in the runner, in its order.
fn queue<'a>(d: Option<&'a Value>) -> Vec<&'a Value> {
    arr(d, "cards").into_iter().filter(|c| str_at(c, "kind") == "entry" && !matches!(str_at(c, "stage").as_str(), "landed" | "rejected")).collect()
}

/// One outcome on the timeline: when, what happened, why, what it carried, its log.
fn outcome(s: &App, p: &Proj, o: &Value, k: &Kit) -> El {
    let (kind, name, result) = (str_at(o, "kind"), str_at(o, "name"), str_at(o, "result"));
    let batch = str_at(o, "batch");
    let step = str_at(o, "step");
    let title = match kind.as_str() {
        "landed" => format!("{name} landed in batch {batch}"),
        "promoted" => format!("batch {batch} on dev: {name}"),
        "check" if step.is_empty() => format!("check {name}: {result}"),
        "check" => format!("check {name}: {result} at {step}"),
        "rejected" => format!("{name} rejected ({batch})"),
        "deferred" => format!("{name} deferred (batch {batch})"),
        _ => format!("{name}: {kind}"),
    };
    let (_, ink, _) = tone(k, &result);
    let card = if kind == "check" { format!("check {name}") } else { format!("queue {name}") };
    let head = text(title).font(k.ty.small_medium).color(k.c.text_base).wrap();
    let head = if find_card(p, &card).is_some() { link_text(head, Detail::Card(card), k) } else { head };
    let mut body = col().grow(1.0).shrink().gap(4.0).child(head);
    let reason = str_at(o, "reason");
    if !reason.is_empty() && kind != "landed" && kind != "promoted" {
        body = body.child(text(reason).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    if let Some(h) = o.get("heads").and_then(Value::as_object) {
        body = body.child(text(h.iter().map(|(r, c)| format!("{r} {}", c.as_str().unwrap_or(""))).collect::<Vec<_>>().join(" · ")).font(k.ty.mono).color(k.c.text_faint).wrap());
    }
    body = body.children(arr(Some(o), "commits").into_iter().map(|c| commit_row(s, c, k)));
    if let Some(log) = o.get("log").and_then(Value::as_str) {
        let label = if step.is_empty() { "Open the log".to_string() } else { format!("Open {step}.log") };
        body = body.child(row().child(open_button(&label, log.to_string(), k)));
    }
    let at = str_at(o, "at");
    row()
        .w_full()
        .gap(10.0)
        .child(text(local(&at)).font(k.ty.small).color(k.c.text_faint).w(84.0).help(crate::ask::ago(&at)))
        .child(rune::el().w(3.0).h_full().radius(1.5).fill(ink))
        .child(body)
}

/// HEAD made a link that opens D.
fn link_text(head: El, d: Detail, k: &Kit) -> El {
    k.interactive(row().radius(k.radius.sm).child(head)).on_press(move |s: &mut App| s.domain.detail = Some(d.clone()))
}

/// The runner's phase in a line.
fn phase(d: Option<&Value>) -> String {
    let since = d.map(|d| str_at(d, "since")).unwrap_or_default();
    let phase = d.map(|d| str_at(d, "runner")).unwrap_or_default();
    if since.is_empty() { format!("runner: {phase}") } else { format!("runner: {phase} · since {} ({})", local(&since), crate::ask::ago(&since)) }
}

// ---- the pipeline ------------------------------------------------------------------------------------------------

/// A station's or a landing's STATE as a pill's words and tone: building, green, red, idle, no tests.
fn station_pill(state: &str, k: &Kit) -> El {
    let label = match state {
        "running" => "building",
        "none" => "no tests",
        "waiting" => "not yet",
        other => other,
    };
    pill(label, state, k)
}

/// Build kind KEY of KINDS (source.py's PIPELINE kinds) in a small chip, its trade-off on hover: "typecheck · -O0".
fn kind_chip(kinds: Option<&Value>, key: &str, k: &Kit) -> El {
    let kind = kinds.and_then(|v| v.get(key));
    let name = kind.map(|v| str_at(v, "name")).unwrap_or_else(|| key.to_string());
    let flags = kind.map(|v| str_at(v, "flags")).unwrap_or_default();
    let tradeoff = kind.map(|v| str_at(v, "tradeoff")).unwrap_or_default();
    let ink = if key == "profiling" { k.c.warning.fg } else if key == "release" { k.c.success.fg } else { k.c.text_muted };
    row()
        .gap(5.0)
        .px(6.0)
        .py(1.0)
        .radius(k.radius.sm)
        .border(1.0, k.c.border_weak)
        .center_y()
        .child(text(name.trim_end_matches(" build").to_string()).font(k.ty.small_medium).color(ink))
        .child(text(flags).font(k.ty.small).color(k.c.text_faint))
        .help(format!("{name}: {tradeoff}"))
}

/// The heads of a ref or build: the main repository's (MAIN, the pipeline's `main`) first and largest, the other
/// repositories' after it, as the runner names them.
fn heads_lines(h: Option<&Value>, main: &str, k: &Kit) -> El {
    let Some(h) = h.and_then(Value::as_object) else { return col() };
    let head = h.get(main).and_then(Value::as_str).unwrap_or("");
    let rest: Vec<String> = h.iter().filter(|(r, _)| *r != main).map(|(r, c)| format!("{r} {}", c.as_str().unwrap_or(""))).collect();
    let mut c = col().gap(2.0);
    if !head.is_empty() {
        c = c.child(row().gap(6.0).center_y().child(text(main.to_string()).font(k.ty.small).color(k.c.text_faint)).child(text(head.to_string()).font(k.ty.mono).color(k.c.text_base)));
    }
    c.child(text(rest.join(" · ")).font(k.ty.small).color(k.c.text_faint).wrap())
}

/// One station of the pipeline: a build stage with its kind (what it builds, at what flags, the trade-off) and what
/// it does now, or a ref (landed, dev) with its heads and when it last moved.
fn station(st: &Value, kinds: Option<&Value>, main: &str, k: &Kit) -> El {
    let (state, title) = (str_at(st, "state"), str_at(st, "title"));
    let kind_key = str_at(st, "kind");
    let is_ref = kind_key.is_empty();
    let (_, ink, border) = tone(k, &state);
    let lit = matches!(state.as_str(), "running" | "red");
    let mut c = col()
        .w(0.0)
        .grow(1.0)
        .min_w(150.0)
        .gap(6.0)
        .padding(12.0)
        .radius(k.radius.md)
        .fill(if is_ref { k.c.bg_base } else { k.c.layer_01 })
        .border(1.0, if lit { border } else { k.c.border_weak });
    let head = row().w_full().gap(6.0).center_y().justify_between();
    c = if is_ref {
        c.child(head.child(row().gap(6.0).center_y().child(text("branch").font(k.ty.small).color(k.c.text_faint)).child(text(title).font(k.ty.ui_medium).color(k.c.text_base))))
            .child(heads_lines(st.get("heads"), main, k))
    } else {
        let kind = kinds.and_then(|v| v.get(&kind_key));
        c.child(head.child(text(title).font(k.ty.ui_medium).color(k.c.text_base)).child(station_pill(&state, k)))
            .child(
                col()
                    .gap(1.0)
                    .child(row().gap(6.0).center_y().child(text(kind.map(|v| str_at(v, "name")).unwrap_or_default()).font(k.ty.small_medium).color(if lit { ink } else { k.c.text_base })).child(text(kind.map(|v| str_at(v, "flags")).unwrap_or_default()).font(k.ty.mono).color(k.c.text_muted)))
                    .child(text(kind.map(|v| str_at(v, "tradeoff")).unwrap_or_default()).font(k.ty.small).color(k.c.text_faint)),
            )
    };
    if !arr(Some(st), "steps").is_empty() {
        c = c.child(strip(st, k));
    }
    c = c.child(text(str_at(st, "note")).font(k.ty.small).color(if state == "red" { k.c.danger.fg } else { k.c.text_muted }).wrap());
    let at = str_at(st, "at");
    if !at.is_empty() {
        c = c.child(text(format!("{} {}", if is_ref { "moved" } else { "last ended" }, crate::ask::ago(&at))).font(k.ty.small).color(k.c.text_faint).help(local(&at)));
    }
    if let Some(log) = st.get("log").and_then(Value::as_str) {
        c = c.child(row().child(open_button("Open the log", log.to_string(), k)));
    }
    c
}

/// The arrow between two stations: lit while the next one builds.
fn flow_arrow(next: &str, k: &Kit) -> El {
    let ink = if next == "running" { k.c.info.fg } else { k.c.text_faint };
    col().w(18.0).center().child(text("→").font(k.ty.ui_medium).color(ink))
}

/// The pipeline as stations left to right (check → landed → O2 → staging → dev, say), from the view's `pipeline`.
fn pipeline_band(d: Option<&Value>, k: &Kit) -> El {
    let p = d.and_then(|d| d.get("pipeline"));
    let kinds = p.and_then(|p| p.get("kinds"));
    let main = p.and_then(|p| p.get("main")).and_then(Value::as_str).unwrap_or("");
    let stations = arr(p, "stations");
    if stations.is_empty() {
        return col();
    }
    let mut r = row().w_full().gap(6.0).items_stretch();
    for (i, st) in stations.iter().enumerate() {
        if i > 0 {
            r = r.child(flow_arrow(&str_at(st, "state"), k));
        }
        r = r.child(station(st, kinds, main, k));
    }
    col().w_full().gap(8.0).child(heading("Pipeline", None, k)).child(r)
}

/// A landing's way through the stations: a dot per station joined by lines, each coloured by how far it got.
fn track(m: &Value, ids: &[String], k: &Kit) -> El {
    let ink = |state: &str| match state {
        "done" => k.c.success.fg,
        "running" => k.c.info.fg,
        "red" => k.c.danger.fg,
        "none" => k.c.layer_02,
        _ => k.c.layer_03,
    };
    let at = |id: &str| m.pointer(&format!("/track/{id}")).and_then(Value::as_str).unwrap_or("waiting").to_string();
    row().children(ids.iter().enumerate().map(|(i, id)| {
        let state = at(id);
        let line = |on: bool| rune::el().h(2.0).grow(1.0).fill(if on { ink(&state) } else { rune::Color::TRANSPARENT });
        let dot = rune::el().w(10.0).h(10.0).radius(5.0).fill(ink(&state)).help(format!("{id}: {}", if state == "waiting" { "not yet" } else { &state }));
        row().w(STATION_W).center_y().child(line(i > 0)).child(dot).child(line(i + 1 < ids.len() && at(&ids[i + 1]) != "waiting"))
    }))
}

const STATION_W: f32 = 76.0;

/// The landings through the pipeline, newest first: who landed, the main repository's head, and how far each has got.
fn movement(d: Option<&Value>, k: &Kit) -> El {
    let p = d.and_then(|d| d.get("pipeline"));
    let stations = arr(p, "stations");
    let ids: Vec<String> = stations.iter().map(|st| str_at(st, "id")).collect();
    let titles: Vec<String> = stations.iter().map(|st| str_at(st, "title")).collect();
    let moves = arr(p, "movement");
    let main = p.and_then(|p| p.get("main")).and_then(Value::as_str).unwrap_or("");
    let lead = |a: El, b: El, c: El| row().w_full().gap(10.0).center_y().child(a.w(84.0)).child(b.w(150.0).truncate()).child(c.w(100.0));
    let header = lead(rune::el(), rune::el(), rune::el()).child(row().children(titles.into_iter().map(|t| col().w(STATION_W).center().child(text(t).font(k.ty.small).color(k.c.text_faint).truncate()))));
    let rows = moves.into_iter().map(|m| {
        let at = str_at(m, "at");
        lead(
            text(local(&at)).font(k.ty.small).color(k.c.text_faint).help(crate::ask::ago(&at)),
            text(str_at(m, "name")).font(k.ty.small_medium).color(k.c.text_base),
            text(m.get("heads").and_then(|h| h.get(main)).and_then(Value::as_str).unwrap_or("").to_string()).font(k.ty.mono).color(k.c.text_muted),
        )
        .child(track(m, &ids, k))
        .px(10.0)
    });
    let c = col().w_full().gap(6.0).child(heading("Landings on their way to dev", None, k)).child(header.px(10.0));
    c.children(rows)
}

/// The runner view: the pipeline and its landings, the check slots, the queue in order, the day's checks and the
/// outcomes.
pub fn runner(s: &App, p: &Proj, spec: &Value, d: &Value, k: &Kit) -> El {
    let q = queue(Some(d));
    let checks: Vec<&Value> = arr(Some(d), "cards").into_iter().filter(|c| str_at(c, "kind") == "check").collect();
    let outcomes = arr(Some(d), "outcomes");
    let body = col()
        .w_full()
        .gap(14.0)
        .px(24.0)
        .py(12.0)
        .child(pipeline_band(Some(d), k))
        .child(movement(Some(d), k))
        .child(heading("Check slots", None, k))
        .child(text(phase(Some(d))).font(k.ty.small).color(k.c.text_muted).wrap())
        .child(lanes(s, Some(spec), Some(d), k))
        .child(section("Queue", q.iter().enumerate().map(|(i, c)| card_row(s, c, Some(i), k)).collect(), "nothing waits", k))
        .child(section("Checks today", checks.into_iter().map(|c| card_row(s, c, None, k)).collect(), "no checks today", k))
        .child(section("Outcomes", outcomes.into_iter().map(|o| outcome(s, p, o, k)).collect(), "nothing yet", k));
    with_detail(s, p, col().key("runner").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(body), k)
}

// ---- home ---------------------------------------------------------------------------------------------------------

/// The overview: every open item (grouped as the items view groups) and the runner now (lanes, queue, the latest
/// outcomes), from the views SPEC names (`items`, `runner`).
pub fn overview(s: &App, p: &Proj, spec: &Value, k: &Kit) -> El {
    let (items_id, runner_id) = (str_at(spec, "items"), str_at(spec, "runner"));
    let items_spec = view_spec(p, &items_id);
    let runner_data = data(p, &runner_id);
    let closed = p.live.closed();
    let open: Vec<&Value> = arr(data(p, &items_id), "items").into_iter().filter(|it| !closed.contains(str_at(it, "id").as_str())).collect();
    let n = open.len();
    let group = items_spec.and_then(|v| v.get("group")).and_then(Value::as_str).unwrap_or("owner");
    let columns = items_spec.map(|v| strings(v.get("columns"))).unwrap_or_default();
    let by_column = columns.iter().map(|c| format!("{} {c}", open.iter().filter(|it| item_state(p, it) == *c).count())).collect::<Vec<_>>().join(" · ");
    let questions = crate::ask::open_count(s, p);
    let left = col()
        .key("home-items")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .scroll(ScrollSpec { stick_bottom: false })
        .child(col().w_full().gap(6.0).px(24.0).py(12.0).child(heading("Open items", Some(n), k)).child(text(by_column).font(k.ty.small).color(k.c.text_faint)).child(grouped(s, p, open, group, &columns, k)));
    let q = queue(runner_data);
    let latest: Vec<El> = arr(runner_data, "outcomes").into_iter().take(8).map(|o| outcome(s, p, o, k)).collect();
    let mut right = col().w_full().gap(12.0).px(24.0).py(12.0).child(heading("Runner now", None, k)).child(text(phase(runner_data)).font(k.ty.small).color(k.c.text_muted).wrap());
    if questions > 0 {
        let label = format!("{questions} open question{} to you", if questions == 1 { "" } else { "s" });
        right = right.child(k.interactive(row().px(10.0).py(6.0).radius(k.radius.sm).fill(k.c.info.bg).border(1.0, k.c.info.border).child(text(label).font(k.ty.small_medium).color(k.c.info.fg))).on_press(|s: &mut App| s.sel = Sel::Questions));
    }
    let right = right
        .child(lanes(s, view_spec(p, &runner_id), runner_data, k))
        .child(section("Queue", q.iter().enumerate().map(|(i, c)| card_row(s, c, Some(i), k)).collect(), "nothing waits", k))
        .child(section("Latest outcomes", latest, "nothing yet", k));
    let right = col().key("home-runner").grow(1.0).min_w(0.0).h_full().scroll(ScrollSpec { stick_bottom: false }).child(right);
    with_detail(s, p, row().w_full().h_full().child(left).child(Divider::vertical().build(k)).child(right), k)
}

// ---- commits ------------------------------------------------------------------------------------------------------

/// The commits the filters let through, newest first.
fn filtered(s: &App) -> Vec<&Commit> {
    let d = &s.domain;
    s.commits()
        .iter()
        .filter(|c| d.agent.as_ref().is_none_or(|a| c.agent == *a) && d.repo.as_ref().is_none_or(|r| c.repo == *r) && d.item.as_ref().is_none_or(|i| c.items.contains(i)))
        .collect()
}

/// A filter's chips: All, then each of OPTIONS; the one chosen marked.
fn chips(label: &str, options: Vec<String>, f: Filter, current: Option<&String>, k: &Kit) -> El {
    let chip = |name: String, value: Option<String>| {
        let on = current == value.as_ref();
        let c = row().px(8.0).py(3.0).radius(k.radius.sm).border(1.0, if on { k.c.border_focus } else { k.c.border_weak }).child(text(name).font(k.ty.small).color(if on { k.c.text_base } else { k.c.text_muted }));
        k.interactive(if on { c.fill(k.c.layer_02) } else { c }).on_press(move |s: &mut App| *s.domain.filter(f) = value.clone())
    };
    let all = chip("all".into(), None);
    row()
        .key(format!("filter-{label}"))
        .w_full()
        .px(24.0)
        .gap(6.0)
        .child(text(label.to_string()).font(k.ty.small_medium).color(k.c.text_faint).w(56.0).pt(3.0))
        .child(row().grow(1.0).shrink().min_w(0.0).gap(6.0).flex_wrap().child(all).children(options.into_iter().map(|o| chip(o.clone(), Some(o)))))
}

/// The agents' commits with filters (agent, repository, item), each with its state and item chips; a row opens the
/// commit, whose panel shows it in its session.
pub fn commits(s: &App, p: &Proj, k: &Kit) -> El {
    let mut agents: Vec<String> = s.commits().iter().map(|c| c.agent.clone()).collect();
    agents.sort();
    agents.dedup();
    let mut repos: Vec<String> = s.commits().iter().map(|c| c.repo.clone()).collect();
    repos.sort();
    repos.dedup();
    let d = &s.domain;
    let mut filters = col().w_full().gap(6.0).py(8.0).child(chips("repo", repos, Filter::Repo, d.repo.as_ref(), k)).child(chips("agent", agents, Filter::Agent, d.agent.as_ref(), k));
    if let Some(i) = &d.item {
        filters = filters.child(chips("item", vec![i.clone()], Filter::Item, Some(i), k));
    }
    let n = filtered(s).len();
    let kit = k.clone();
    let rows = col()
        .key("commit-rows")
        .w_full()
        .growth(rune::GrowthSpec::default())
        .virtual_rows(
            n,
            56.0,
            |s: &App, i| std::borrow::Cow::Owned(filtered(s).get(i).map(|c| c.hash.clone()).unwrap_or_default()),
            move |s: &App, i| {
                let k = &kit;
                let Some(c) = filtered(s).get(i).map(|c| (*c).clone()) else { return col() };
                let mut line = vec![c.agent.clone(), c.repo.clone(), local(&c.at)];
                if !c.refs.is_empty() {
                    line.push(c.refs.clone());
                }
                let items = c.items.iter().map(|it| {
                    let it2 = it.clone();
                    k.interactive(row().px(6.0).py(1.0).radius(k.radius.sm).border(1.0, k.c.border_weak).child(text(it.clone()).font(k.ty.small).color(k.c.text_accent)))
                        .on_press(move |s: &mut App| s.domain.item = Some(it2.clone()))
                });
                let body = col()
                    .gap(2.0)
                    .child(
                        row()
                            .w_full()
                            .gap(8.0)
                            .center_y()
                            .child(text(c.hash[..9.min(c.hash.len())].to_string()).font(k.ty.mono).color(k.c.text_muted))
                            .child(text(c.subject.clone()).font(k.ty.ui).color(k.c.text_base).truncate().grow(1.0).shrink())
                            .children(items)
                            .child(pill(&c.state, &c.state, k)),
                    )
                    .child(text(line.join(" · ")).font(k.ty.small).color(k.c.text_faint).truncate());
                pressable(s, body.px(14.0), Detail::Commit(c.hash.clone()), k).key(c.hash.clone())
            },
        );
    // the view holds the last days; the week before the oldest held comes when asked (coordd's `Older`)
    let more = match (&p.older_from, p.older_pending) {
        (_, true) => text("reading older commits…").font(k.ty.small).color(k.c.text_faint).into(),
        (Some(from), false) => {
            let pi = s.cur;
            Button::new(&format!("Show older (before {})", local(from))).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
                if let Some(p) = s.projects.get_mut(pi) {
                    p.ask_older();
                }
            }).build(k)
        }
        (None, false) => col(),
    };
    let list = col().key("commit-list").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows).child(row().px(14.0).py(12.0).child(more));
    with_detail(s, p, col().w_full().h_full().child(filters).child(list), k)
}

// ---- decisions ----------------------------------------------------------------------------------------------------

/// The decisions: the numbered rulings (newest first; one opens in the panel) or the owner's answered questions
/// with their threads; the search narrows either.
pub fn decisions(s: &App, p: &Proj, d: &Value, k: &Kit) -> El {
    let pick = SegmentedControl::new(&["Rulings", "Answered questions"], Some(s.domain.answered as usize)).label("Decisions shown").fit().on_pick(|s: &mut App, i, _| s.domain.answered = i == 1).build(k);
    let search = TextInput::new(&s.domain.query).placeholder("Search").width(Some(280.0)).on_edit(|s: &mut App| &mut s.domain.query).build(k);
    let q = s.domain.query.text.to_lowercase();
    let hit = |t: &str| q.is_empty() || t.to_lowercase().contains(&q);
    let list = if s.domain.answered {
        let cards: Vec<El> = crate::ask::questions(p, true)
            .into_iter()
            .filter(|m| m.state.as_deref() == Some("answered"))
            .filter(|m| hit(&m.body) || p.live.mail.replies(&m.id).iter().any(|r| hit(&r.body)))
            .map(|m| crate::ask::card(s, p, m, k))
            .collect();
        if cards.is_empty() { col().child(text("no answered questions").font(k.ty.body).color(k.c.text_faint)) } else { col().w_full().gap(12.0).children(cards) }
    } else {
        let mut rulings = arr(Some(d), "rulings");
        rulings.retain(|r| hit(&str_at(r, "title")) || hit(&str_at(r, "body")));
        rulings.reverse();
        col().w_full().gap(2.0).children(rulings.into_iter().map(|r| {
            let n = str_at(r, "number");
            let body = row()
                .gap(10.0)
                .center_y()
                .child(text(n.clone()).font(k.ty.mono).color(k.c.text_muted).w(40.0))
                .child(text(str_at(r, "title")).font(k.ty.ui).color(k.c.text_base).truncate().grow(1.0).shrink())
                .child(text(str_at(r, "when")).font(k.ty.small).color(k.c.text_faint));
            pressable(s, body, Detail::Ruling(n), k)
        }))
    };
    let bar = row().w_full().px(24.0).py(8.0).gap(16.0).center_y().child(pick).child(search);
    let list = col().key("decision-list").w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list.px(24.0).pb(24.0));
    with_detail(s, p, col().w_full().h_full().child(bar).child(list), k)
}
