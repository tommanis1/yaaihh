//! The agents page (design amendments, 7 Nov evening). **Active**: the project's running agents (a process, as coordd
//! sees it: any state but stopped) as live tiles, each with its state, model, tokens and cost, what it runs, and the
//! tail of its chat as it grows; a click opens the agent full size (its agent page, chat.rs, with a way back).
//! **All agents**: every agent the registry has, stopped and closed ones too, as a table sorted by a header click; a
//! click opens the agent page. Tokens are coordd's (each agent's view); cost is from the price table (`App::prices`:
//! an extension's, so it agrees with what that extension shows, else the bundled catalog), shown as spend ("$X / N
//! tokens", tokens alone without a price, the split by type on hover: spend.rs).

use crate::{App, Proj};
use coord_types::{AgentView, ModelUsage};
use crate::spend::{Spend, spend};
use prices::CostTokens;
use rune::{Align, El, ScrollSpec, col, row, text};
use rune_kit::{Badge, BadgeVariant, Cell, Kit, SegmentedControl, SortDir, Table, TableColumn, TableState};
use std::borrow::Cow;
use std::cmp::Ordering;

/// Entries of a tile's chat tail.
const TAIL: usize = 8;

pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let tabs = SegmentedControl::new(&["Active", "All agents"], Some(s.agents_tab)).fit().label("Agents").on_pick(|s: &mut App, i, _| s.agents_tab = i).build(k);
    let body = if s.agents_tab == 0 { grid(s, p, k) } else { all(s, k) };
    // without the price table every spend shows tokens alone; say why
    let mut top = row().w_full().px(24.0).py(10.0).gap(12.0).center_y().child(tabs);
    if let Err(e) = &s.prices {
        top = top.child(text(format!("no price table: {e}")).font(k.ty.small).color(k.c.danger.fg).truncate());
    }
    col().w_full().h_full().child(top).child(body)
}

/// The agents with a running process.
pub fn active(p: &Proj) -> impl Iterator<Item = &AgentView> {
    p.live.agents.iter().filter(|a| a.state != "stopped")
}

fn grid(s: &App, p: &Proj, k: &Kit) -> El {
    let tiles: Vec<El> = active(p).map(|a| tile(s, p, a, k)).collect();
    if tiles.is_empty() {
        return col().padding(24.0).child(text("No agent runs now; All agents lists every one.").font(k.ty.body).color(k.c.text_muted));
    }
    col()
        .key("grid")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(row().w_full().flex_wrap().gap(12.0).px(24.0).pb(24.0).children(tiles))
}

fn tile(s: &App, p: &Proj, a: &AgentView, k: &Kit) -> El {
    let head = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(a.name.clone()).font(k.ty.ui_medium).color(k.c.text_base).truncate())
        .child(state_badge(a, k))
        .child(row().grow(1.0))
        .child(switch_slot(s, &a.name, k));
    let facts = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(text(a.model.clone()).font(k.ty.small).color(k.c.text_muted).truncate())
        .child(spend(k, &spend_of(s, &a.usage)));
    let name = a.name.clone();
    k.interactive(
        col()
            .key(format!("tile-{}", a.name))
            .w(440.0)
            .h(340.0)
            .padding(12.0)
            .gap(6.0)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_weak)
            .child(head)
            .child(facts)
            .child(text(doing(a)).font(k.ty.small).color(k.c.text_faint).truncate())
            .child(tail(s, p, a, k)),
    )
    .on_press(move |s: &mut App| s.open_agent(&name, true))
}

/// A tile's "Chat | Terminal" switch: the tile shows the chat's tail or the agent's terminal, scaled to fit (term.rs;
/// the full view has its own switch, chat.rs).
fn switch_slot(s: &App, name: &str, k: &Kit) -> El {
    let on = s.term.tiles.contains(&(s.cur, name.to_string()));
    let who = name.to_string();
    SegmentedControl::new(&["Chat", "Terminal"], Some(on as usize))
        .fit()
        .label("Show")
        .on_pick(move |s: &mut App, i, _| {
            let key = (s.cur, who.clone());
            if i == 1 {
                s.term.tiles.insert(key);
            } else {
                s.term.tiles.remove(&key);
            }
        })
        .build(k)
}

/// The last entries of A's chat, following new ones (read from coordd as the full view reads them, chat.rs).
fn tail(s: &App, p: &Proj, a: &AgentView, k: &Kit) -> El {
    if s.term.tiles.contains(&(s.cur, a.name.clone())) {
        return col().w_full().grow(1.0).min_h(0.0).child(crate::term::view(s, k, s.cur, &a.name, false));
    }
    let Some(c) = p.convs.get(&a.name).filter(|c| c.loaded) else {
        let why = p.convs.get(&a.name).and_then(|c| c.refused.as_ref()).map_or("reading the session…".to_string(), |w| format!("not read: {w}"));
        return col().grow(1.0).child(text(why).font(k.ty.small).color(k.c.text_faint).wrap());
    };
    let items = crate::chat::items(s, s.cur, c);
    let from = items.len().saturating_sub(TAIL);
    let n = items.len();
    let entries = (from..n).map(|i| crate::chat::item_row(s, s.cur, &a.name, &items[i], i + 1 == n, k).key(format!("{}-{}", a.name, crate::chat::item_key(c, &items[i]))));
    col()
        .key(format!("tail-{}", a.name))
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .radius(k.radius.sm)
        .fill(k.c.bg_base)
        .clip_rounded(k.radius.sm)
        .scroll(ScrollSpec { stick_bottom: true })
        .child(col().w_full().children(entries))
}

fn state_badge(a: &AgentView, k: &Kit) -> El {
    let v = match a.state.as_str() {
        "working" => BadgeVariant::Accent,
        "stopped" => BadgeVariant::Subtle,
        _ => BadgeVariant::Neutral,
    };
    Badge::new(a.state.clone()).variant(v).build(k)
}

/// What A runs now, or how long ago its last tool call began.
fn doing(a: &AgentView) -> String {
    match a.tools.iter().max_by(|x, y| x.at.cmp(&y.at)) {
        Some(t) => format!("running {} · {} · {}", t.tool, ago(&t.at).unwrap_or_default(), t.what),
        None => match a.last_tool.as_deref().and_then(ago) {
            Some(d) => format!("last tool call {d} ago"),
            None => "no tool call yet".into(),
        },
    }
}

/// How long ago AT (RFC 3339) was: "42 s", "5 min", "3 h", "2 d".
fn ago(at: &str) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    let s = (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds().max(0);
    Some(match s {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    })
}

/// AT (RFC 3339) in local time, "11-07 18:10".
fn local(at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(at).map(|t| t.with_timezone(&chrono::Local).format("%m-%d %H:%M").to_string()).unwrap_or_else(|_| at.to_string())
}

/// An agent's tokens by type, summed over its models, and their cost by the price table (which prices each
/// model's tokens per type). The cost is there only when every model has a price: a model without one is never
/// counted as $0, and a partial sum would pass for the whole, so then the tokens show alone.
fn spend_of(s: &App, usage: &[ModelUsage]) -> Spend {
    let mut out = Spend { cost: Some([0.0; 4]), ..Default::default() };
    for u in usage {
        out.input += u.tokens.input as f64;
        out.cache_read += u.tokens.cache_read as f64;
        out.cache_write += u.tokens.cache_write as f64;
        out.output += u.tokens.output as f64;
        out.reasoning += u.tokens.reasoning as f64;
        let t = CostTokens {
            input: u.tokens.prompt(),
            output: u.tokens.output,
            cache_read: u.tokens.cache_read,
            cache_creation: u.tokens.cache_write,
            long_input: u.long.prompt(),
            long_output: u.long.output,
            long_cache_read: u.long.cache_read,
            long_cache_creation: u.long.cache_write,
        };
        let parts = s.prices.as_ref().ok().and_then(|p| p.cost_parts(&u.model, &u.tier, &t));
        out.cost = match (out.cost, parts) {
            (Some(c), Some(p)) => Some(std::array::from_fn(|i| c[i] + p[i])),
            _ => None,
        };
    }
    if usage.is_empty() {
        out.cost = None;
    }
    out
}

/// The registry's agents in the table's order (the sort the owner chose, kept in `App::all_sort`).
fn all_rows(s: &App) -> Vec<&AgentView> {
    let mut v: Vec<&AgentView> = s.proj().map(|p| p.live.agents.iter().collect()).unwrap_or_default();
    if let Some((column, dir)) = &s.all_sort {
        // by dollars, then tokens; agents without a price below the priced ones
        let key = |a: &AgentView| {
            let sp = spend_of(s, &a.usage);
            (sp.cost.map(|c| c.iter().sum::<f64>()), sp.tokens())
        };
        v.sort_by(|a, b| {
            let o = match column.as_str() {
                "name" => rune_kit::natural_cmp(&a.name, &b.name),
                "id" => rune_kit::natural_cmp(&a.id, &b.id),
                "parent" => a.parent.cmp(&b.parent),
                "role" => a.role.cmp(&b.role),
                "kind" => a.kind.cmp(&b.kind),
                "model" => a.model.cmp(&b.model),
                "state" => a.state.cmp(&b.state),
                "created" => a.created.cmp(&b.created),
                "last" => a.last_active.cmp(&b.last_active),
                "sessions" => a.sessions.cmp(&b.sessions),
                _ => key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal),
            };
            dir.apply(o)
        });
    }
    v
}

fn row_id(s: &App, i: usize) -> Cow<'_, str> {
    Cow::Borrowed(all_rows(s)[i].name.as_str())
}

fn all(s: &App, k: &Kit) -> El {
    let kit = k.clone();
    let table = Table::new(|s: &App| &s.all_table, |s: &mut App| &mut s.all_table, |s: &App| s.proj().map_or(0, |p| p.live.agents.len()), row_id)
        .name("All agents")
        .column(TableColumn::new("name", "Name").flex(1.4).min(110.0).sortable())
        .column(TableColumn::new("id", "Id").width(84.0).min(64.0).sortable())
        .column(TableColumn::new("parent", "Parent").flex(1.0).min(80.0).sortable())
        .column(TableColumn::new("role", "Role").width(170.0).min(70.0).sortable())
        .column(TableColumn::new("kind", "Kind").width(70.0).min(56.0).sortable())
        .column(TableColumn::new("model", "Model").flex(1.4).min(110.0).sortable())
        .column(TableColumn::new("state", "State").flex(1.0).min(80.0).sortable())
        .column(TableColumn::new("created", "Created").width(104.0).min(90.0).sortable())
        .column(TableColumn::new("last", "Last active").width(104.0).min(90.0).sortable())
        .column(TableColumn::new("sessions", "Sessions").width(80.0).min(64.0).align(Align::End).sortable())
        .column(TableColumn::new("spend", "Spend").width(150.0).min(100.0).align(Align::End).sortable())
        .cell(move |s: &App, i, column| {
            let a = all_rows(s)[i];
            match column {
                "name" => Cell::text(&a.name),
                "id" => Cell::text(&a.id).mono(),
                "parent" => Cell::text(&a.parent).muted(),
                "role" => Cell::text(role_text(a)).muted(),
                "kind" => Cell::text(&a.kind).muted(),
                "model" => Cell::text(&a.model),
                "state" => Cell::text(&a.state).muted(),
                "created" => Cell::text(local(&a.created)).muted(),
                "last" => Cell::text(a.last_active.as_deref().map(local).unwrap_or_default()).muted(),
                "sessions" => Cell::text(a.sessions.to_string()).mono(),
                _ => Cell::el(spend(&kit, &spend_of(s, &a.usage))),
            }
        })
        .on_sort(|s: &mut App, column, dir, _| s.all_sort = Some((column.to_string(), dir)))
        .on_select(|s: &mut App, name, _| s.open_agent(name, false))
        .zebra(true)
        .build(s, k)
        .grow(1.0)
        .min_h(0.0);
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(24.0)
        .pb(16.0)
        .child(col().key("all-frame").w_full().grow(1.0).min_h(0.0).radius(k.radius.md).border(1.0, k.c.border_weak).clip_rounded(k.radius.md).child(table))
}

impl App {
    /// Open agent NAME's page; FROM_GRID puts a way back to the grid in its toolbar.
    pub fn open_agent(&mut self, name: &str, from_grid: bool) {
        self.back_to_grid = from_grid;
        self.select_agent(name);
    }

    /// The tiles' chats: each active agent's session read once (agent events read on, main.rs `take`).
    pub fn follow_tiles(&mut self) {
        let Some(p) = self.projects.get_mut(self.cur) else { return };
        let names: Vec<String> = active(p).map(|a| a.name.clone()).collect();
        for n in names {
            if !p.convs.get(&n).is_some_and(|c| c.loaded || c.newer_pending) {
                p.read_newer(&n);
            }
        }
    }
}

/// All agents' table at its start: sorted by when each agent was registered.
pub fn table_state() -> (TableState, Option<(String, SortDir)>) {
    let mut t = TableState::new().key("all-agents");
    t.set_sort("created", SortDir::Ascending);
    (t, Some(("created".into(), SortDir::Ascending)))
}

/// A's role and its team (vault designs/teams-and-tiers-2026-11-10.md §6): `lead · team 1/3 by orch`, `member of DEBUG`.
fn role_text(a: &AgentView) -> String {
    let mut t = if a.role.is_empty() { "member".to_string() } else { a.role.clone() };
    if let Some(l) = &a.member_of {
        t += &format!(" of {l}");
    }
    if let Some(g) = &a.team {
        t += &format!(" · team {}/{} by {}", g.used, g.granted, g.by);
    }
    t
}
