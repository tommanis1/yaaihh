//! The agents page (design amendments, 7 Nov evening; redrawn by teams for N-472). **Active**: the project's agents by
//! their teams as parts.rs nests them, the breakdown the sidebar held under Agents until his N-1104 p863
//! (paper-research N-1446 p2008): each team's heading folds and says how many of its agents run and how many are
//! stopped; under it the tiles of its running agents (a process, as coordd sees it: any state but stopped) and its
//! stopped ones on one quiet line. Each agent's tile heads with its one line, its name and the label of the thread
//! it works on (an idle one dim, with since when; one that waits on the owner says so, the page's only accent), over
//! the tail of its chat as it grows; the tile's Chat/Terminal switch shows on the tile under the pointer. A click
//! opens the agent full size (its agent page, chat.rs, with a way back); a drag onto Agents in the sidebar pins it. **All agents**:
//! every agent the registry has, stopped and closed ones too, as a table sorted by a header click, with its model,
//! tokens and cost; a click opens the agent page. Tokens are coordd's (each agent's view); cost is from the price
//! table (`App::prices`: an extension's, so it agrees with what that extension shows, else the bundled catalog),
//! shown as spend ("$X / N tokens", tokens alone without a price, the split by type on hover: spend.rs).

use crate::{App, Proj};
use coord_types::{AgentView, ModelUsage, Request};
use crate::spend::{Spend, spend};
use prices::CostTokens;
use rune::{Align, El, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Cell, Kit, SegmentedControl, SortDir, Switch, Table, TableColumn, TableState};
use std::borrow::Cow;
use std::cmp::Ordering;

/// Entries of a tile's chat tail.
const TAIL: usize = 8;

/// The page's tabs, `App::agents_tab` the one shown; History names a place on the page by it (history.rs).
pub(crate) const TABS: [&str; 2] = ["Active", "All agents"];

pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let tabs = SegmentedControl::new(&TABS, Some(s.agents_tab)).fit().label("Agents").on_pick(|s: &mut App, i, _| s.agents_tab = i).build(k);
    let pi = s.place_of(p);
    // the All table's rows and state are the focused project's (`all_rows`): a pane of another project shows its cards
    let body = if s.agents_tab == 0 || pi != s.cur { grid(s, p, k) } else { all(s, k) };
    // without the price table every spend shows tokens alone; say why
    let mut top = row().w_full().px(24.0).py(10.0).gap(12.0).center_y().child(tabs);
    if let Some(line) = price_line(&s.prices, being_read_for()) {
        let (words, ink, why) = match line {
            PriceLine::Faint(w, why) => (w, k.c.text_faint, why),
            PriceLine::Fault(w) => (w, k.c.danger.fg, None),
        };
        let t = text(words).font(k.ty.small).color(ink).truncate();
        top = top.child(match why {
            Some(why) => t.help(why),
            None => t,
        });
    }
    // a plain standing agent, to try things on (owner m875; projects.rs `NewAgent`)
    let new = Button::new("New agent").variant(ButtonVariant::Neutral).disabled(p.down.is_some()).on_press(crate::projects::new_agent_open).build(k);
    top = top.child(rune::spacer());
    if let Some(hold) = hold(pi, p, k) {
        top = top.child(hold);
    }
    if let Some(esc) = esc_all(pi, p, k) {
        top = top.child(esc);
    }
    top = top.child(new);
    let mut page = col().w_full().h_full().child(top);
    if let Some(line) = held_line(pi, p, k) {
        page = page.child(line);
    }
    page.child(body)
}

/// Hold (line 327, N-1324; consult-lead, posts 7 and 12; the owner, 10 Nov: "keep them interupted / esc'ed so i can be
/// the one to sedn them messages"): his switch for agents' mail, beside Esc all. On, of what is written from then on
/// only his posts reach agents, until Release. It sends no Escape, as Esc all holds nothing; its hover says how the two
/// go together. Shown on a coordd from `MAIL_HOLD_PROTOCOL`.
fn hold(pi: usize, p: &Proj, k: &Kit) -> Option<El> {
    if p.down.is_some() || p.protocol < coord_types::MAIL_HOLD_PROTOCOL {
        return None;
    }
    let on = p.live.mail_hold.is_some();
    // its words on it (paper-research N-1446 p3207, 5: "Hold" alone said nothing of what it holds)
    let help = "Only your messages reach agents";
    let sw = Switch::new(on).label(HOLD).on_toggle(move |s: &mut App| {
        s.request_in(pi, Request::MailHold { on: !on }, crate::Pending::Sent);
    });
    Some(sw.build(k).help(help))
}

/// The Hold switch's words.
pub(crate) const HOLD: &str = "Hold agents' mail";

/// The line under the page's heading while agents' mail is held (consult-lead, N-1324 posts 7 and 12): since when, that
/// what he sends still reaches them, and Release. Muted, not red.
fn held_line(pi: usize, p: &Proj, k: &Kit) -> Option<El> {
    let since = p.live.mail_hold.as_deref().filter(|_| p.down.is_none() && p.protocol >= coord_types::MAIL_HOLD_PROTOCOL)?;
    let release = Button::new("Release").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        s.request_in(pi, Request::MailHold { on: false }, crate::Pending::Sent);
    });
    Some(row().w_full().px(24.0).pb(8.0).gap(12.0).center_y().child(text(held_words(since)).font(k.ty.small).color(k.c.text_muted)).child(rune::spacer()).child(release.build(k)))
}

/// The held line's words for a hold since SINCE (RFC 3339, coordd's).
pub(crate) fn held_words(since: &str) -> String {
    format!("Agents' mail held since {}", crate::when::at(since))
}

/// Agent A's Brain switch on its card in project PI ([`brain_switch`]), and beside it ([`brain_words`]): off, since
/// when; its last pass's time, which opens that pass.
fn brain(pi: usize, p: &Proj, a: &AgentView, k: &Kit) -> Option<El> {
    let sw = brain_switch(pi, p, a, "Brain", "", k)?;
    let b = a.brain.as_ref()?;
    let words = |w: String| text(w).font(k.ty.small).color(k.c.text_muted);
    let (off, last) = brain_words(b);
    // pressable only when it names a ticket: words that look pressable and do nothing are not drawn
    let last = match b.last_id.clone().and_then(|id| docket_machine::parse_ticket(&id).map(|n| (n, id))) {
        Some((n, id)) => k.interactive(words(last)).help("Open its last pass").on_press(move |s: &mut App| crate::find::open_at(s, n, &id, None)),
        None => words(last),
    };
    let parts = std::iter::once(sw).chain(off.map(words)).chain([last]);
    Some(row().key(format!("brain-{}", a.name)).w_full().gap(10.0).center_y().children(parts))
}

/// His switch for agent A's brain in project PI, labelled LABEL, its help after SAID: drawn only when coordd is at
/// `BRAIN_PROTOCOL` and a binding opens `brain` for A. It draws what coordd read from the record; a press asks coordd
/// (`Request::Brain`), and a refusal is the notice. Off stops new passes only: one at work runs to its end.
pub(crate) fn brain_switch(pi: usize, p: &Proj, a: &AgentView, label: &str, said: &str, k: &Kit) -> Option<El> {
    let b = a.brain.as_ref().filter(|_| p.down.is_none() && p.protocol >= coord_types::BRAIN_PROTOCOL)?;
    let (on, agent) = (b.on, a.name.clone());
    let help = if on {
        "Reads your posts, may add a thought"
    } else {
        "Off: no new passes"
    };
    let sw = Switch::new(on).label(label).on_toggle(move |s: &mut App| {
        s.request_in(pi, Request::Brain { agent: agent.clone(), on: !on }, crate::Pending::Sent);
    });
    Some(sw.build(k).help(format!("{said}{help}")))
}

/// What Brain says beside its switch, on his clock: off, since he switched it off ("off since 11:20"); its last pass's
/// time ("last pass 10:42"), or "no pass yet".
pub(crate) fn brain_words(b: &coord_types::BrainView) -> (Option<String>, String) {
    let off = (!b.on).then(|| b.since.as_deref().map_or_else(|| "off".to_string(), |t| format!("off since {}", crate::when::at(t))));
    (off, b.last_pass.as_deref().map_or_else(|| "no pass yet".to_string(), |t| format!("last pass {}", crate::when::at(t))))
}

/// The passes of AGENT's brain in project P that spoke, oldest first, each with when it spoke (turn.rs `nanos`): the
/// tickets of kind `brain` for it (its form's `agent`) with the post the pass made ([`spoke`]): Ahead's Next at a turn
/// end, consolidation's Kept at a compaction (proto-mind's H-1613), Attend's Said at his posts. Same, Quiet and a
/// failure say nothing and are none of them. The snapshot's and those a fetch holds (find.rs `views`; lib.rs
/// `asks_on_pipe_up` asks for every pass once a pipe is up, as one closed a day ago leaves the snapshot), each once, the
/// snapshot's first. What the agent's chat draws at the time each spoke, on its card and its page (chat.rs `items`;
/// his N-1104 p625, "in the ui see when it intervers hshow dont tell"; paper-research N-1446 p2926).
pub(crate) fn passes(s: &App, p: &Proj, agent: &str) -> Vec<(u64, u64)> {
    let mut out: Vec<(u64, u64)> = brain_views(s, p, agent).filter_map(|v| Some((v.n, crate::turn::nanos(&spoke(v)?.at)?))).collect();
    out.sort_by_key(|&(n, t)| (t, n));
    out
}

/// Every pass of AGENT's brain in project P, at work or ended, each once: the tickets of kind `brain` for it (its form's
/// `agent`), the snapshot's and those a fetch holds, the snapshot's first. What [`passes`] and the Map's satellite
/// (map/brain.rs) read.
pub(crate) fn brain_views<'a>(s: &'a App, p: &'a Proj, agent: &'a str) -> impl Iterator<Item = &'a coord_types::TicketView> + 'a {
    brains(s, p).filter(move |v| v.form.get("agent").and_then(serde_json::Value::as_str) == Some(agent))
}

/// Every pass of every agent's brain in project P, each once, the snapshot's first ([`brain_views`] without the agent).
pub(crate) fn brains<'a>(s: &'a App, p: &'a Proj) -> impl Iterator<Item = &'a coord_types::TicketView> + 'a {
    let mut seen = std::collections::HashSet::new();
    crate::find::views(s, p).filter(move |v| v.kind == "brain" && seen.insert(v.n))
}

/// The post pass V made when it spoke: its own (by its id), the first; none while it is at work or when it said
/// nothing.
pub(crate) fn spoke(v: &coord_types::TicketView) -> Option<&docket_machine::Post> {
    v.posts.iter().find(|p| p.by == v.id)
}

/// Esc all (line 200, N-1324; consult-lead, post 7): one press stops every running turn of project PI, as the one-agent
/// Esc does for one, and nothing else. The engine picks who is in a turn when it takes the press, so the request names
/// nobody. A ghost button at the page's head, no confirm, shown only while one of the project's agents is in a turn
/// and only on a coordd from `ESC_ALL_PROTOCOL`; its hover says what it does not do. No window text names a command.
fn esc_all(pi: usize, p: &Proj, k: &Kit) -> Option<El> {
    let in_turn = p.live.agents.iter().any(|a| a.state == "working" && matches!(a.kind.as_str(), "claude" | "codex"));
    if !in_turn || p.down.is_some() || p.protocol < coord_types::ESC_ALL_PROTOCOL {
        return None;
    }
    let help = esc_all_help(p);
    let b = Button::new("Esc all").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| {
        s.request_in(pi, Request::EscAll, crate::Pending::EscAll);
    });
    Some(b.build(k).help(help))
}

/// The agent page's spin-off presses on the bar (his walk item 19, N-1794: "with an agent selected he wants a quick way
/// to make a spin-off of it, and later either to join it back when he closes it or to delete it; 'delete' by that
/// name"). Agent NAME of the current project is one of:
/// - a spin-off (a copy of another agent's conversation, `AgentView::from`): Join back (what it found goes to the agent
///   it came from, then it is gone from his lists), Close (its session ends; it stays, stopped, and starts again on
///   its own conversation) and Delete (gone, nothing handed back). Join back waits for a running turn to end, said on
///   its hover; Close shows only while it runs;
/// - any other Claude Code agent: Spin off, a copy of its whole conversation as it stands, even in a turn.
/// While one of them runs, its press says so and the others wait. None for a project not up, and none of the spin-off's
/// three from a coordd before [`coord_types::SPIN_PROTOCOL`].
pub fn spin_presses(s: &App, name: &str, k: &Kit) -> Vec<El> {
    use coord_types::SpinEnd;
    let Some(p) = s.proj().filter(|p| p.down.is_none()) else { return Vec::new() };
    let Some(a) = p.live.agents.iter().find(|a| a.name == name) else { return Vec::new() };
    let ending = p.spin_ending.as_ref().filter(|e| e.name == name).map(|e| e.how);
    let busy = p.forking.is_some() || p.spin_ending.is_some();
    match a.from.clone().filter(|_| p.protocol >= coord_types::SPIN_PROTOCOL) {
        Some(from) => {
            let in_turn = a.state == "working";
            let press = |words: &str, doing: &str, how: SpinEnd, variant: ButtonVariant| {
                let (n, f) = (name.to_string(), from.clone());
                Button::new(if ending == Some(how) { doing } else { words })
                    .variant(variant)
                    .disabled(busy || (how == SpinEnd::Join && in_turn))
                    .on_press(move |s: &mut App| crate::projects::spin_end(s, &n, &f, how))
                    .build(k)
                    .key(format!("spin-{}", words.split(' ').next().unwrap_or(words).to_lowercase()))
            };
            let join_help = if in_turn { format!("Join back after its turn") } else { format!("Hand its findings to {from}") };
            let mut out = vec![press("Join back", "Joining back…", SpinEnd::Join, ButtonVariant::Neutral).help(join_help)];
            if a.state != "stopped" {
                out.push(press("Close", "Closing…", SpinEnd::Close, ButtonVariant::Ghost).help(format!("End its session, keep it")));
            }
            out.push(press("Delete", "Deleting…", SpinEnd::Delete, ButtonVariant::Ghost).help(format!("Delete, hand nothing back")));
            out
        }
        None if a.kind == "claude" || a.kind.is_empty() => {
            let forking = p.forking.is_some();
            let n = name.to_string();
            let b = Button::new(if forking { "Spinning off…" } else { "Spin off" })
                .variant(ButtonVariant::Neutral)
                .disabled(busy)
                .on_press(move |s: &mut App| crate::projects::fork(s, &n, None))
                .build(k)
                .key("spin-off");
            vec![b.help(format!("Copy its whole conversation"))]
        }
        None => Vec::new(),
    }
}

/// Esc all's hover (consult-lead, N-1324 post 7): what it does not do, and, on a coordd that has Hold (line 327), how
/// to keep them stopped, and what Hold cannot keep: a message already handed to the session (sitting in Claude Code's
/// queue) goes in right after the interrupt (docket-core5, 51 of 52; consult-3 N-818 post 78, 3).
fn esc_all_help(p: &Proj) -> String {
    let then = if p.protocol >= coord_types::MAIL_HOLD_PROTOCOL { " · Hold mail keeps them" } else { "" };
    format!("Interrupt all turns{then}")
}

/// The notice after Esc all (consult-lead, N-1324 post 7): to how many agents the Escape went, by coordd's reply OUT
/// ("being stopped: A, B; already being stopped: C"), and when, NOW; a refusal as it came.
pub(crate) fn esc_all_said(ok: bool, out: &str, now: chrono::DateTime<chrono::Local>) -> String {
    if !ok {
        return out.to_string();
    }
    let count = |head: &str| out.split("; ").find_map(|part| part.strip_prefix(head)).map_or(0, |names| names.split(", ").filter(|n| !n.is_empty()).count());
    let agents = |n: usize| if n == 1 { "1 agent".to_string() } else { format!("{n} agents") };
    let at = now.format("%H:%M");
    match (count("being stopped: "), count("already being stopped: ")) {
        (0, 0) => format!("No agent was in a turn · {at}"),
        (0, m) => format!("Esc already sent to {} · {at}", agents(m)),
        (n, _) => format!("Esc sent to {} · {at}", agents(n)),
    }
}

/// What the Agents page says of its prices beside the tabs, if anything.
#[derive(Debug, PartialEq)]
enum PriceLine {
    /// Not his to act on: the table is on its way, or which table the costs come from; why, if any, on its hover
    /// (consult-4, N-585.p44: reveal little, the detail behind the pointer).
    Faint(String, Option<String>),
    /// No table: every cost shows tokens alone.
    Fault(String),
}

/// How long the prices may be on their way before the page says so (consult-4, N-585.p34: a slow read on the WSL
/// machine kept the costs away without a word).
const QUIET_WHILE_READ: std::time::Duration = std::time::Duration::from_secs(5);

/// How long the page has seen the prices on their way (the first draw that saw them so starts the clock).
fn being_read_for() -> std::time::Duration {
    static SINCE: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    SINCE.get_or_init(std::time::Instant::now).elapsed()
}

/// The line for PRICES, read for FOR: none while they are on their way for a few seconds, then a faint "being read";
/// a table's note faint (the bundled prices; what its first `: ` is followed by, the why, on the hover); no table at all
/// as a fault.
fn price_line(prices: &Result<prices::PriceTable, String>, waited: std::time::Duration) -> Option<PriceLine> {
    match prices {
        Err(e) if e == prices::BEING_READ => (waited >= QUIET_WHILE_READ).then(|| PriceLine::Faint("prices are being read; costs come with them".into(), None)),
        Err(e) => Some(PriceLine::Fault(format!("no price table: {e}"))),
        Ok(t) => t.note().map(|n| match n.split_once(": ") {
            Some((said, why)) => PriceLine::Faint(said.to_string(), Some(why.to_string())),
            None => PriceLine::Faint(n.to_string(), None),
        }),
    }
}

/// The agents with a running process.
pub fn active(p: &Proj) -> impl Iterator<Item = &AgentView> {
    p.live.agents.iter().filter(|a| a.state != "stopped")
}

fn grid(s: &App, p: &Proj, k: &Kit) -> El {
    let closed: std::collections::HashSet<String> = s.closed_parts.iter().filter(|(id, _)| *id == p.id).map(|(_, x)| x.clone()).collect();
    let sections: Vec<El> = match crate::parts::rows(&p.live.agents, &p.live.parts, &closed) {
        Some(rows) => team_rows(s, p, rows, k),
        // a coordd that names no parts: the running agents in one group
        None => teams(&p.live.agents, &p.live.parts)
            .into_iter()
            .map(|(team, agents)| {
                let working = agents.iter().filter(|a| a.state == "working").count();
                // the team's heading says the count once; its tiles under it
                let head = row()
                    .gap(8.0)
                    .center_y()
                    .child(text(team.clone()).font(k.ty.ui_medium).color(k.c.text_base))
                    .child(text(format!("{working} of {} working", agents.len())).font(k.ty.small).color(k.c.text_muted));
                let tiles: Vec<El> = agents.into_iter().map(|a| tile(s, p, a, k)).collect();
                col().key(format!("team-{team}")).w_full().gap(10.0).child(head).child(row().w_full().flex_wrap().gap(12.0).children(tiles))
            })
            .collect(),
    };
    if sections.is_empty() {
        // a new project's: the next step after the project (paper-research N-1446 p2424, 0)
        let words = if p.live.agents.is_empty() { "No agent yet: New agent starts one." } else { "No agent running" };
        return col().padding(24.0).child(text(words).font(k.ty.body).color(k.c.text_muted));
    }
    col()
        .key("grid")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(col().w_full().gap(10.0).px(24.0).pb(24.0).children(sections))
}

/// The Active tab by the teams as parts.rs nests them, ROWS (his N-1104 p863: the breakdown of agents and teams moved
/// here from the sidebar; paper-research N-1446 p2008, p2016): each team's heading with its fold and its count (its
/// nested teams' agents too), its running agents' tiles under it, its stopped ones on one quiet line (a press shows
/// All agents), a team nobody is in yet saying so; who leads the others first, under Leads. A team with nobody running
/// stands too. His folds are the ones he made in the sidebar (`App::closed_parts`).
fn team_rows(s: &App, p: &Proj, rows: Vec<crate::parts::Row>, k: &Kit) -> Vec<El> {
    use crate::parts::Row;
    let indent = |depth: u8| f32::from(depth.saturating_sub(1)) * 24.0;
    let (mut out, mut tiles, mut at): (Vec<El>, Vec<El>, u8) = (Vec::new(), Vec::new(), 1);
    for r in rows {
        if let Row::Agent { a, depth } = r {
            // who leads, before any team: under Leads, as the page had them
            if out.is_empty() && tiles.is_empty() {
                out.push(text("Leads").font(k.ty.ui_medium).color(k.c.text_base).key("team-Leads"));
            }
            at = depth;
            tiles.push(tile(s, p, a, k));
            continue;
        }
        if !tiles.is_empty() {
            out.push(row().w_full().pl(indent(at)).flex_wrap().gap(12.0).children(std::mem::take(&mut tiles)));
        }
        let first = out.is_empty();
        out.push(match r {
            Row::Part { name, path, depth, running, stopped, open } => {
                let key = (p.id.clone(), path.clone());
                let head = row()
                    .key(format!("part-{path}"))
                    .w_full()
                    .pl(indent(depth))
                    // a team at the top stands apart from the one above
                    .pt(if depth == 1 && !first { 14.0 } else { 0.0 })
                    .gap(8.0)
                    .center_y()
                    .child(text(if open { "▾" } else { "▸" }).font(k.ty.small).color(k.c.text_muted))
                    .child(text(name).font(k.ty.ui_medium).color(k.c.text_base))
                    .child(text(crate::parts::count(running, stopped)).font(k.ty.small).color(k.c.text_muted));
                k.interactive(head).help(if open { "Fold this team" } else { "Unfold this team" }).on_press(move |s: &mut App| {
                    if !s.closed_parts.remove(&key) {
                        s.closed_parts.insert(key.clone());
                    }
                })
            }
            Row::Stopped { names, depth } => {
                let line = text(format!("stopped: {}", names.join(" · "))).font(k.ty.small).color(k.c.text_muted).truncate().shrink().min_w(0.0);
                k.interactive(row().key(format!("stopped-{}", names.join("-"))).w_full().pl(indent(depth)).child(line)).help("All agents lists them").on_press(|s: &mut App| s.agents_tab = 1)
            }
            Row::Empty { depth } => row().w_full().pl(indent(depth)).child(text("none yet").font(k.ty.small).color(k.c.text_faint)),
            Row::Agent { .. } => continue,
        });
    }
    if !tiles.is_empty() {
        out.push(row().w_full().pl(indent(at)).flex_wrap().gap(12.0).children(tiles));
    }
    out
}

/// The running agents by team, in parts.rs's order, each team with its running agents; who leads the others first,
/// under "Leads". One group, "Agents", when coordd names no parts (the Active tab's then). Teams with nobody running
/// are left out (their stopped agents are under All agents).
pub fn teams<'a>(agents: &'a [AgentView], parts: &[(String, i64)]) -> Vec<(String, Vec<&'a AgentView>)> {
    let Some(rows) = crate::parts::rows(agents, parts, &std::collections::HashSet::new()) else {
        let all: Vec<&AgentView> = agents.iter().filter(|a| a.state != "stopped").collect();
        return if all.is_empty() { Vec::new() } else { vec![("Agents".to_string(), all)] };
    };
    let mut out: Vec<(String, Vec<&AgentView>)> = Vec::new();
    let mut team = "Leads".to_string();
    for r in rows {
        match r {
            crate::parts::Row::Part { name, .. } => team = name,
            crate::parts::Row::Agent { a, .. } => match out.last_mut() {
                Some((t, v)) if *t == team => v.push(a),
                _ => out.push((team.clone(), vec![a])),
            },
            _ => {}
        }
    }
    out
}

/// What A works on: the ticket coordd names in its view (`AgentView.on`, docket-machine `label::agent_on`, the same
/// ticket its label names), drawn as that ticket's label and id. A coordd from before the field sends none; then its
/// label of the agent ("WORD of THREAD") gives THREAD, with its id when the window holds a thread of that label. None
/// when it is on no ticket.
pub fn working_on(p: &Proj, a: &AgentView) -> Option<crate::refs::Ref> {
    let thread = p.labels.get(&a.name).and_then(|l| l.split_once(" of ")).map(|(_, t)| t.to_string());
    if let Some(id) = a.on.as_ref().filter(|id| !id.is_empty()) {
        let label = p.labels.get(id).map(str::to_string).or(thread);
        return Some(match label {
            Some(label) => crate::refs::Ref { label, id: id.clone() },
            None => crate::refs::Ref { label: id.clone(), id: String::new() },
        });
    }
    let thread = thread?;
    let id = p.live.tickets.values().find(|t| p.labels.get(&t.id) == Some(thread.as_str())).map(|t| t.id.clone()).unwrap_or_default();
    Some(crate::refs::Ref { label: thread, id })
}

/// An agent held on a rate limit (`AgentView::limit_until`), as its row says it (local time), coordd's own state words
/// shortened (proto-lead N-1397 p160), and never a time gone by as still to come (proto-lead N-1468, p167):
/// - its reset still ahead: "rate-limited · resumes 05:11";
/// - past it, with his mail hold on, which keeps its turn till he lets go (`resume_due`): "rate-limited · resets 05:11 ·
///   held";
/// - past it, with no hold: the seconds of a hand-over or an esc's stop: "rate-limited · resets 05:11 · resuming".
///
/// Its row, the sidebar, the Docket's worker box, the All agents table and the circle's panel all read it here; the
/// window wakes at the reset to say so then ([`next_reset`]). None when it is not held.
pub fn held(p: &Proj, a: &AgentView) -> Option<String> {
    held_at(a, chrono::Utc::now().timestamp(), p.live.mail_hold.is_some())
}

/// [`held`] at NOW (seconds), HOLD whether his mail hold is on.
fn held_at(a: &AgentView, now: i64, hold: bool) -> Option<String> {
    let t = a.limit_until?;
    let at = crate::when::secs(t);
    // the provider's passing failure, not a limit: coordd puts what stopped it first in its words (core.rs `Limit::what`;
    // the owner's walk item 14)
    let failure = ["the provider overloaded", "a server error"].into_iter().find(|w| a.limit_why.as_deref().is_some_and(|y| y.starts_with(w)));
    if let Some(w) = failure {
        return Some(match (t > now, hold) {
            (true, _) => format!("stopped: {w} · tries again at {at}"),
            (false, true) => format!("stopped: {w} · held"),
            (false, false) => format!("stopped: {w} · resuming"),
        });
    }
    Some(match (t > now, hold) {
        (true, _) => format!("rate-limited · resumes {at}"),
        (false, true) => format!("rate-limited · resets {at} · held"),
        (false, false) => format!("rate-limited · resets {at} · resuming"),
    })
}

/// The soonest reset still ahead of NOW (seconds) among the agents held on a rate limit, of every project: coordd
/// sends nothing then, so the window asks to wake at it (`App::wake_at`) and [`held`] says the reset has come.
pub fn next_reset(s: &App, now: i64) -> Option<i64> {
    s.projects.iter().flat_map(|p| &p.live.agents).filter_map(|a| a.limit_until).filter(|t| *t > now).min()
}

/// An idle agent's words: "idle since 14:13" (local time), or "idle"; one held on a rate limit says so instead
/// ([`held`]), so it does not read as idle with nothing due.
pub fn idle_since(p: &Proj, a: &AgentView) -> String {
    if let Some(words) = held(p, a) {
        return words;
    }
    if chrono::DateTime::parse_from_rfc3339(&a.since).is_err() {
        return "idle".into();
    }
    format!("idle since {}", crate::when::at(&a.since))
}

/// What an agent whose terminal shows a dialog waiting for an answer says, wherever it is drawn (`AgentView::dialog`).
pub const WAITING: &str = "Waiting on terminal prompt";

/// How many of P's agents wait on a terminal prompt.
pub fn waiting(p: &Proj) -> usize {
    p.live.agents.iter().filter(|a| a.dialog.is_some()).count()
}

/// [`WAITING`] for N agents: the words alone for one.
pub fn waiting_words(n: usize) -> String {
    if n == 1 { WAITING.to_string() } else { format!("{n} waiting on a terminal prompt") }
}

/// What the sidebar says under A, the same reading as the page's line (N-447 p58: the engine's own phrase shown
/// nowhere): "waits on you", "idle since …" (or that it is held on a rate limit, whatever its state: [`held`]), or the
/// thread it works on; none when nothing is known.
pub fn side_line(p: &Proj, a: &AgentView) -> Option<String> {
    // stuck on its terminal before anything else: nothing it is handed reaches it until the prompt is answered
    if a.dialog.is_some() {
        return Some(WAITING.into());
    }
    if crate::waits::agent_waits(p, &a.name) {
        return Some("waits on you".into());
    }
    if a.deaf.is_some() {
        return Some("takes no messages".into());
    }
    if a.state == "idle" || a.limit_until.is_some() {
        return Some(idle_since(p, a));
    }
    working_on(p, a).map(|r| r.label)
}

/// An agent's one line: its name, and the label of the thread it works on with the id dim after it; an idle agent
/// dim, with since when; one held on a rate limit, idle or waiting, says so ([`held`]), wrapped in its tile, the
/// 429's own words on hover;
/// one that waits on the owner says so in the accent.
fn line(pi: usize, p: &Proj, a: &AgentView, k: &Kit) -> El {
    let idle = a.state == "idle";
    // the name whole; the thread's label is what gets cut
    let name = text(a.name.clone()).font(k.ty.ui_medium).color(if idle { k.c.text_faint } else { k.c.text_base });
    let mut r = row().w_full().gap(8.0).center_y().child(name);
    if let Some(title) = &a.dialog {
        return r.child(text(WAITING).font(k.ty.small).color(k.c.text_accent).help(title.clone()).key(format!("agent-waiting-{}", a.name)));
    }
    // the one thing on the page in the accent: something of its waits on him (waits.rs)
    if crate::waits::agent_waits(p, &a.name) {
        return r.child(text("waits on you").font(k.ty.small).color(k.c.text_accent));
    }
    // its session takes no messages: that needs him too, in coordd's words with what he can do, wrapped in its tile
    // (`AgentView::deaf`, paper-research N-1446 p2424, p2427), what he can do a press in them (p2527, 2)
    if let Some(why) = &a.deaf {
        return r.child(deaf_links(text(why.clone()).font(k.ty.small).color(k.c.text_accent).wrap().shrink().min_w(0.0), why, pi, a, k));
    }
    if idle || a.limit_until.is_some() {
        // a held agent's words are longer than a tile: they wrap, not cut, so "your hold" is read (N-1397 p167)
        let words = text(idle_since(p, a)).font(k.ty.small).color(k.c.text_faint).wrap().shrink().min_w(0.0);
        return r.child(match a.limit_why.clone() {
            Some(why) => words.help(why),
            None => words,
        });
    }
    if let Some(on) = working_on(p, a) {
        r = r.child(crate::refs::el(&on, k.ty.small, k.c.text_muted, k).shrink());
    }
    r
}

fn tile(s: &App, p: &Proj, a: &AgentView, k: &Kit) -> El {
    let pi = s.place_of(p);
    let hovered = s.agents_hover.as_deref() == Some(a.name.as_str());
    let jobs_open = s.agents_jobs.as_deref() == Some(a.name.as_str());
    let mut head = row().w_full().gap(8.0).center_y().child(line(pi, p, a, k).grow(1.0).min_w(0.0));
    // the tile's switch appears on the tile under the pointer only; so does Jobs, which opens its last jobs on the
    // build runner as a press on its job line does, there too when no job is on the line (N-1334)
    if hovered {
        if !a.jobs.is_empty() {
            let who = a.name.clone();
            head = head.child(Button::new("Jobs").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| toggle_jobs(s, &who)).build(k));
        }
        head = head.child(switch_slot(s, pi, &a.name, k));
    }
    // what it does this moment, the same line as under his message in With you (N-516); a press on its job line opens
    // the last jobs
    let working = crate::streams::working_now(a, k).map(|w| match crate::jobs::shown(a) {
        Some(_) => {
            let who = a.name.clone();
            k.interactive(w).help(if jobs_open { "Close its last jobs" } else { "Its last jobs" }).on_press(move |s: &mut App| toggle_jobs(s, &who))
        }
        None => w,
    });
    let name = a.name.clone();
    let dragged = a.name.clone();
    let hover = a.name.clone();
    let seen = a.name.clone();
    let menu_at = (a.name.clone(), a.parent.clone());
    k.interactive(
        col()
            .key(format!("tile-{}", a.name))
            // three across a common window, so a team reads at a glance (N-472); the chat's tail scrolls within
            .w(420.0)
            .h(200.0)
            .padding(12.0)
            .gap(6.0)
            .radius(k.radius.md)
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_weak)
            .child(head)
            .children(brain(pi, p, a, k))
            .children(working)
            .children(jobs_open.then(|| crate::jobs::list(a, k)))
            // only what keeps it from starting, which he must see; where it stands is on its own page. A session that
            // takes no messages is said once, by its head line (`line`)
            .children(where_lines(a).into_iter().filter(|(t, warn)| *warn && a.deaf.as_ref() != Some(t)).map(|(t, _)| text(t).font(k.ty.small).color(k.c.text_muted).truncate()))
            .child(tail(s, p, a, k)),
    )
    .on_hover_change(move |s: &mut App, inside: bool, _: &mut rune::Tx| {
        if inside {
            s.agents_hover = Some(hover.clone());
        } else if s.agents_hover.as_deref() == Some(hover.as_str()) {
            s.agents_hover = None;
        }
    })
    .on_press(move |s: &mut App| s.select_agent(&name))
    // dragged onto Agents in the sidebar it is pinned there, onto a pane it opens there (panes.rs `drop_zone`)
    .on_drag(move |s: &mut App, d: &mut rune::Drag, _: &mut rune::Tx| _ = s.drag_agent(pi, &dragged, d))
    // the acts he has at this agent (acts.rs)
    .on_secondary_press(move |s: &mut App, at, _| { let c = s.cur; crate::acts::open(s, c, crate::acts::at_agent(&menu_at.0, &menu_at.1), at) })
    .on_visible(move |s: &mut App, on: bool, _: &mut rune::Tx| {
        let key = (pi, seen.clone());
        if on {
            s.agents_seen.insert(key);
            s.follow_tiles();
        } else {
            s.agents_seen.remove(&key);
        }
    })
}

/// Open WHO's last jobs on its tile, or close them; another tile's close.
fn toggle_jobs(s: &mut App, who: &str) {
    s.agents_jobs = if s.agents_jobs.as_deref() == Some(who) { None } else { Some(who.to_string()) };
}

/// A tile's "Chat | Terminal" switch: the tile shows the chat's tail or the agent's terminal, scaled to fit (term.rs;
/// the full view has its own switch, chat.rs).
fn switch_slot(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let on = s.term.tiles.contains(&(pi, name.to_string()));
    let who = name.to_string();
    SegmentedControl::new(&["Chat", "Terminal"], Some(on as usize))
        .fit()
        .label("Show")
        .on_pick(move |s: &mut App, i, tx| {
            // the tile shows the other at once, as the agent page's switch does (streams.rs)
            tx.instant();
            let key = (pi, who.clone());
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
    let pi = s.place_of(p);
    if s.term.tiles.contains(&(pi, a.name.clone())) {
        return col().w_full().grow(1.0).min_h(0.0).child(crate::term::view(s, k, pi, &a.name, false));
    }
    let Some(c) = p.convs.get(&a.name).filter(|c| c.loaded) else {
        let why = p.convs.get(&a.name).and_then(|c| c.refused.as_ref()).map_or("reading the session…".to_string(), |w| format!("not read: {w}"));
        return col().grow(1.0).child(text(why).font(k.ty.small).color(k.c.text_faint).wrap());
    };
    let items = crate::chat::items(s, pi, &a.name, c);
    let from = items.len().saturating_sub(TAIL);
    let (n, last) = (items.len(), crate::chat::last_row(&items));
    // its brain's passes stand among them at the times they spoke (chat.rs `with_passes`; his N-1104 p625)
    let entries = (from..n).map(|i| crate::chat::item_row(s, pi, &a.name, &items[i], Some(i) == last, k).key(format!("{}-{}", a.name, crate::chat::item_key(c, &items[i]))));
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

/// TEXT, a line ending in A's `deaf` words (its own line, or under his post "try-1 · not delivered: …"), with each act
/// coordd names for them (`AgentView::deaf_do`, docket-core's core-deafdo) a press on its words there (paper-research
/// N-1446 p2527, 2): `DEAF_ACCOUNT` opens its account choice and `DEAF_RESTART` restarts it ([`deaf_act`]). coordd's words stand as they are and nothing of
/// them is matched here: an act's words are its own, found from the line's end back, the last act first; an act this
/// window does not know, or whose words are not there, stays words, as the whole line is from a coordd before the field.
pub fn deaf_links(el: El, line: &str, pi: usize, a: &AgentView, k: &Kit) -> El {
    let mut el = el;
    let mut end = line.len();
    let mut any = false;
    // underlined at rest, so they read as presses before the pointer finds them (a link's resting look is its spans')
    let mut under = Vec::new();
    for d in a.deaf_do.iter().rev() {
        if ![coord_types::DEAF_ACCOUNT, coord_types::DEAF_RESTART].contains(&d.act.as_str()) || d.words.is_empty() {
            continue;
        }
        let Some(at) = line[..end].rfind(d.words.as_str()) else { continue };
        let help = if d.act == coord_types::DEAF_ACCOUNT { "Change its account in Session settings" } else { "Restart its session" };
        el = el.link(rune::Link::new(at..at + d.words.len(), d.act.clone()).role("button").help(help).hover(rune::LinkPaint::new().color(k.c.text_base).underline(k.c.text_base)));
        under.push(rune::Span::new(at, at + d.words.len()).underline(k.c.text_accent));
        (end, any) = (at, true);
    }
    if !any {
        return el;
    }
    under.reverse();
    let el = el.spans(under);
    let name = a.name.clone();
    el.on_link(move |s: &mut App, act: &str, _: &mut rune::Tx| deaf_act(s, pi, &name, act))
}

/// His press on an act of agent NAME's `deaf` words in project PI ([`deaf_links`]): its page opens with its Session
/// settings open, where its Account row is his choice of account (`DEAF_ACCOUNT`), and where Restart's answer is said,
/// the restart sent as that Restart sends it (`DEAF_RESTART`).
pub fn deaf_act(s: &mut App, pi: usize, name: &str, act: &str) {
    s.open_in(pi, name);
    crate::settings::open(s, pi, name);
    if act == coord_types::DEAF_RESTART {
        s.streams.said.remove(&(pi, name.to_string()));
        s.request_in(pi, Request::Restart { name: name.to_string() }, crate::Pending::Effort { agent: name.to_string() });
    }
}

/// Where A stands and what keeps it from starting, one line each (warn: the owner should look): no box (`no_box`), a
/// session that takes no messages (`deaf`, with what he can do), the layer it stands in (`stands`), its last move
/// between views (`moved`), with the files it left uncommitted.
pub fn where_lines(a: &AgentView) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    if let Some(title) = &a.dialog {
        out.push((format!("{WAITING}: {title}"), true));
    }
    if let Some(w) = &a.no_box {
        out.push((format!("not started, no box: {w}"), true));
    }
    if let Some(w) = &a.deaf {
        out.push((w.clone(), true));
    }
    let place = |v: &Option<String>| match v.as_deref() {
        None => "its own folder".to_string(),
        Some(l) if l.starts_with("line@") => "the line's view".to_string(),
        Some(l) => format!("layer {l}"),
    };
    match &a.moved {
        Some(m) => {
            out.push((format!("moved {} from {} to {}: {}", crate::when::at(&m.at), place(&m.from), place(&m.to), m.why), false));
            if !m.left.is_empty() {
                out.push((format!("left uncommitted in {}: {}", place(&m.from), m.left.join(", ")), true));
            }
        }
        None if a.stands.is_some() => out.push((format!("stands in {}", place(&a.stands)), false)),
        None => {}
    }
    out
}


/// An agent's tokens by type, summed over its models, and their cost by the price table (which prices each
/// model's tokens per type). The cost is there only when every model has a price: a model without one is never
/// counted as $0, and a partial sum would pass for the whole, so then the tokens show alone.
pub(crate) fn spend_of(s: &App, usage: &[ModelUsage]) -> Spend {
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
            ..Default::default()
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

/// The registry's agents in the table's order (the sort the owner chose, kept in `App::all_sort`): but those his
/// Archive holds (archive.rs `agent_hidden`: now, stopped before its moment; there, registered after it).
fn all_rows(s: &App) -> Vec<&AgentView> {
    let mut v: Vec<&AgentView> = s.proj().map(|p| p.live.agents.iter().filter(|a| !crate::archive::agent_hidden(s, a)).collect()).unwrap_or_default();
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
                "account" => crate::accounts::account_cell(s, a).0.cmp(&crate::accounts::account_cell(s, b).0),
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
    // a column a cut makes read wrong is never narrower than its longest words at the table's font and padding: a
    // name cut reads as another's (73's table, 1400 wide: AGT-004, AGT-008 and AGT-009 each "paper-resea…"), so Name
    // and Parent fit their longest; Role fits its longest ("lead · team 20/20 by o…" leaves out whose team it is);
    // State fits the first words of each ("waiting…" says nothing), the rest cut, on its hover (docket-edge8's walk,
    // N-1446 p2353 #4); Account fits its longest, as its cell says it ("Claude…" on three rows read as one account:
    // proto-window's walk of 76, N-1446 p2731; his N-1104 p425). The table scrolls sideways when they are wider than
    // the page, below.
    let style = rune_kit::TableStyle::from_kit(k);
    let agents = s.proj().map_or(&[][..], |p| &p.live.agents[..]);
    let name_w = words_w(&style, 110.0, agents.iter().map(|a| a.name.as_str()));
    let parent_w = words_w(&style, 80.0, agents.iter().map(|a| a.parent.as_str()));
    let role_w = words_w(&style, 70.0, agents.iter().map(role_text));
    let account_w = words_w(&style, 80.0, agents.iter().map(|a| crate::accounts::account_cell(s, a).0));
    let state_w = s.proj().map_or(80.0, |p| words_w(&style, 80.0, agents.iter().map(|a| state_head(&state_cell(p, a)))));
    // each column once: its id, title, width (its own, or a share by weight of the room the fixed ones leave), least,
    // and whether it holds numbers (at its end)
    enum W {
        Fixed(f32),
        Share(f32),
    }
    let specs = [
        ("name", "Name", W::Share(1.4), name_w, false),
        ("id", "Id", W::Fixed(84.0), 64.0, false),
        ("parent", "Parent", W::Share(1.0), parent_w, false),
        ("role", "Role", W::Fixed(role_w), 70.0, false),
        ("kind", "Kind", W::Fixed(70.0), 56.0, false),
        ("model", "Model", W::Share(1.4), 110.0, false),
        ("account", "Account", W::Share(1.0), account_w, false),
        ("state", "State", W::Share(1.0), state_w, false),
        ("created", "Created", W::Fixed(104.0), 90.0, false),
        ("last", "Last active", W::Fixed(104.0), 90.0, false),
        ("sessions", "Sessions", W::Fixed(80.0), 64.0, true),
        ("spend", "Spend", W::Fixed(150.0), 100.0, true),
    ];
    let columns: Vec<TableColumn> = specs
        .iter()
        .map(|(id, title, w, least, end)| {
            let c = TableColumn::new(*id, *title).min(*least).sortable();
            let c = match w {
                W::Fixed(w) => c.width(*w),
                W::Share(g) => c.flex(*g),
            };
            if *end { c.align(Align::End) } else { c }
        })
        .collect();
    // the least the table is wide, its columns side by side: each at its width, a shared one at its least, one he
    // dragged at the width he gave it as the table clamps it (rune-kit table.rs `sizes`). Its own: the sideways
    // scroll lays it out at the width it says it needs, which leaves out the shared columns' leasts, and they ran past
    // its end where no scroll reached (the leg's 1390 of columns in a table 1124 wide)
    let wide: f32 = specs
        .iter()
        .zip(&columns)
        .map(|((id, _, w, least, _), c)| match (s.all_table.column_width(id), w) {
            (Some(d), _) => c.clamp(d),
            (None, W::Fixed(w)) => *w,
            (None, W::Share(_)) => *least,
        })
        .sum();
    let table = columns
        .into_iter()
        .fold(Table::new(|s: &App| &s.all_table, |s: &mut App| &mut s.all_table, |s: &App| all_rows(s).len(), row_id).name("All agents"), |t, c| t.column(c))
        .cell(move |s: &App, i, column| {
            let a = all_rows(s)[i];
            match column {
                "name" => Cell::text(&a.name),
                "id" => Cell::text(&a.id).mono(),
                "parent" => Cell::text(&a.parent).muted(),
                "role" => Cell::text(role_text(a)).muted(),
                "kind" => Cell::text(&a.kind).muted(),
                // his name for it, as New agent lists it (paper-research N-1446 p3207, 3); sorted by its id
                "model" => Cell::text(&crate::projects::plain_model(&a.model)),
                // where it runs, and its last move on hover (0.2 D21)
                "account" => match crate::accounts::account_cell(s, a) {
                    (t, None) => Cell::text(t),
                    (t, Some(h)) => Cell::el(text(t).font(kit.ty.text_small).color(kit.c.text_base).truncate().shrink().min_w(0.0).help(h)),
                },
                // not started for want of a box: the state says why
                "state" => match (&a.no_box, s.proj()) {
                    (Some(_), Some(p)) => Cell::text(state_cell(p, a)),
                    (None, Some(p)) => Cell::text(state_cell(p, a)).muted(),
                    (_, None) => Cell::text(""),
                },
                "created" => Cell::text(crate::when::at(&a.created)).muted(),
                "last" => Cell::text(a.last_active.as_deref().map(crate::when::at).unwrap_or_default()).muted(),
                "sessions" => Cell::text(a.sessions.to_string()).mono(),
                _ => Cell::el(spend(&kit, &spend_of(s, &a.usage))),
            }
        })
        .on_sort(|s: &mut App, column, dir, _| s.all_sort = Some((column.to_string(), dir)))
        .on_select(|s: &mut App, name, _| s.select_agent(name))
        .zebra(true)
        .build(s, k)
        // as wide as the page when its columns fit: the sideways scroll lays the table out as a row, where this grow
        // takes the room left, since `min_w(wide)` keeps rune from giving it `min-width: 100%` (accounts-lead, N-1446
        // p2595; the leg `all_agents_fills_a_page_wider_than_its_columns`)
        .grow(1.0)
        .min_h(0.0)
        .min_w(wide);
    col()
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .px(24.0)
        .pb(16.0)
        .child(
            col()
                .key("all-frame")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .radius(k.radius.md)
                .border(1.0, k.c.border_weak)
                .clip_rounded(k.radius.md)
                // the columns at their widths, the table as wide as the page or as `wide`, scrolled sideways then: a
                // column past the page's edge was cut off with no way to it (Spend, 1400 wide, on the walk's agents)
                // its right edge faded while columns are hidden past it, the left while scrolled (rune's own): nothing
                // else says the table scrolls while no bar shows (paper-research, N-1446 p2594)
                .child(col().key("all-wide").w_full().grow(1.0).min_h(0.0).scroll_x().scroll_edge_fade(24.0).child(table)),
        )
}

/// The width of a column that shows each of WORDS whole: the longest at the table's font, with the cell's padding,
/// and never under LEAST.
fn words_w<T: AsRef<str>>(style: &rune_kit::TableStyle, least: f32, words: impl Iterator<Item = T>) -> f32 {
    let longest = words.map(|t| rune::text::measure(t.as_ref(), &[], style.font, None).0).fold(0.0, f32::max);
    (longest + 2.0 * style.cell_pad + 2.0).max(least)
}

/// A's state in its All agents cell: `streams::state_words`, or that it is not started for want of a box, and why.
fn state_cell(p: &Proj, a: &AgentView) -> String {
    match &a.no_box {
        Some(w) => format!("not started, no box: {w}"),
        None => crate::streams::state_words(p, a),
    }
}

/// The first words of a state, which its column shows whole: up to its first " · " or ": " ("waiting on its job",
/// "rate-limited", "not started, no box"), with room for the "…" of the rest cut; the whole when it is all.
fn state_head(words: &str) -> String {
    match [" · ", ": "].iter().filter_map(|sep| words.find(sep)).min() {
        Some(i) => format!("{} …", &words[..i]),
        None => words.to_string(),
    }
}

impl App {
    /// The tiles' chats: the session of each tile on screen read once (agent events read on, main.rs `take`); a tile
    /// scrolled to is read when it shows, one never shown is not asked for.
    pub fn follow_tiles(&mut self) {
        let cur = self.cur;
        let seen = &self.agents_seen;
        let Some(p) = self.projects.get_mut(cur) else { return };
        let names: Vec<String> = active(p).filter(|a| seen.contains(&(cur, a.name.clone()))).map(|a| a.name.clone()).collect();
        for n in names {
            p.open_conv(&n);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The prices on their way say nothing for a few seconds, then a faint line; the bundled prices after a failed read
    /// say so, faint, with why on the hover; no table at all is a fault (consult-4, N-585.p34).
    #[test]
    fn the_page_says_which_prices_its_costs_come_from() {
        let short = std::time::Duration::from_secs(1);
        let long = QUIET_WHILE_READ;
        assert_eq!(price_line(&Err(prices::BEING_READ.into()), short), None);
        assert_eq!(price_line(&Err(prices::BEING_READ.into()), long), Some(PriceLine::Faint("prices are being read; costs come with them".into(), None)));
        assert_eq!(price_line(&Err("gone".into()), short), Some(PriceLine::Fault("no price table: gone".into())));
        assert_eq!(price_line(&Ok(prices::PriceTable::new(Default::default())), long), None);
        let bundled = prices::PriceTable::new(Default::default()).noted("costs from the bundled prices: no WSL".into());
        assert_eq!(price_line(&Ok(bundled), short), Some(PriceLine::Faint("costs from the bundled prices".into(), Some("no WSL".into()))));
    }

    /// No box, the layer it stands in, its last move and what it left: one line each, the ones to look at warned.
    #[test]
    fn where_an_agent_stands_is_a_line_each() {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "AGT-001", "name": "a", "parent": "", "role": "", "kind": "", "model": "", "state": "", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        assert!(where_lines(&a).is_empty());
        a.stands = Some("L-3".into());
        assert_eq!(where_lines(&a), vec![("stands in layer L-3".to_string(), false)]);
        a.no_box = Some("no sub-uid left".into());
        a.moved = Some(coord_types::AgentMoved { at: "x".into(), from: None, to: Some("L-3".into()), why: "step 4 of C-3".into(), left: vec!["a.txt".into()] });
        assert_eq!(where_lines(&a), vec![("not started, no box: no sub-uid left".to_string(), true), ("moved x from its own folder to layer L-3: step 4 of C-3".to_string(), false), ("left uncommitted in its own folder: a.txt".to_string(), true)]);
        // a session that takes no messages: warned, with coordd's words (paper-research N-1446 p2424)
        (a.no_box, a.moved, a.deaf) = (None, None, Some("its session takes no messages (Claude Code says “Channels are not currently available”); pick an account for this agent".into()));
        assert_eq!(where_lines(&a)[0], ("its session takes no messages (Claude Code says “Channels are not currently available”); pick an account for this agent".to_string(), true));
    }
}

#[cfg(test)]
mod team_tests {
    use coord_types::AgentView;

    /// His window on agent SEL of project research, whose agents are `lead` (working) and `lead-fork`, its spin-off
    /// (idle), both pinned, the spin-off's pin under its source's, on a coordd at PROTOCOL.
    fn spun(sel: &str, protocol: u32) -> crate::App {
        let (_held, mut s) = crate::view_cost::test_app();
        s.notice = None;
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = protocol;
        let mut fork = agent("lead-fork", "", "owner", "idle");
        fork.kind = "claude".into();
        fork.from = Some("lead".into());
        let mut lead = agent("lead", "", "owner", "working");
        lead.kind = "claude".into();
        p.live.agents = vec![lead, fork];
        s.projects = vec![p];
        s.cur = 0;
        s.pins.put(crate::sidebar::Pin { project: "research".into(), id: String::new(), agent: "lead".into(), thread: false, lens: None }, None);
        s.pins.spun_off("research", "lead", "lead-fork");
        s.sel = crate::Sel::Agent(sel.into());
        s
    }

    /// His walk item 19 (N-1794): a plain agent's bar has Spin off; its spin-off's has Join back, Close and Delete,
    /// Delete by that name, and its pin says what it is a spin-off of. Join back waits while the spin-off is in a
    /// turn. A coordd before SPIN_PROTOCOL shows none of the three.
    #[test]
    fn an_agent_spins_off_and_its_spin_off_joins_back_closes_or_is_deleted() {
        let texts = |rt: &rune::runtime::Runtime<crate::App>, key: &str| -> Vec<String> {
            let inside = format!("/{key}/");
            rt.elements().into_iter().filter(|e| !e.exiting && (e.key.as_deref() == Some(key) || e.path.contains(&inside))).filter_map(|e| e.text.clone()).collect()
        };
        let mut rt = crate::view_cost::window(spun("lead", coord_types::SPIN_PROTOCOL));
        crate::picture::docket::tests::frames(&mut rt);
        assert!(texts(&rt, "spin-off").iter().any(|t| t == "Spin off"), "a plain agent: Spin off");
        assert!(rt.find("spin-delete").is_none() && rt.find("spin-join").is_none());
        drop(rt);
        let mut rt = crate::view_cost::window(spun("lead-fork", coord_types::SPIN_PROTOCOL));
        crate::picture::docket::tests::frames(&mut rt);
        for (key, words) in [("spin-join", "Join back"), ("spin-close", "Close"), ("spin-delete", "Delete")] {
            assert!(texts(&rt, key).iter().any(|t| t == words), "{key}: {words}: {:?}", texts(&rt, key));
        }
        assert!(rt.find("spin-off").is_none(), "a spin-off is not spun off again from its bar");
        assert!(rt.elements().iter().any(|e| e.path.contains("/sidebar/") && e.text.as_deref().is_some_and(|t| t.contains("spin-off of lead"))), "its pin says so");
        drop(rt);
        let mut s = spun("lead-fork", coord_types::SPIN_PROTOCOL);
        s.projects[0].live.agents[1].state = "working".into();
        assert_eq!(super::spin_presses(&s, "lead-fork", &rune_kit::Kit::neutral(false)).len(), 3, "in a turn: still three, Join back disabled");
        let old = spun("lead-fork", coord_types::SPIN_PROTOCOL - 1);
        assert_eq!(super::spin_presses(&old, "lead-fork", &rune_kit::Kit::neutral(false)).len(), 1, "an older coordd: Spin off alone, no Join back, Close, Delete");
    }

    /// The picture for N-1794: the spin-off's page with Join back, Close and Delete on its bar, its pin under its
    /// source's saying "spin-off of lead". `STILL_OUT=DIR … agents::tests::spin_off_still --ignored --exact`.
    #[test]
    #[ignore]
    fn spin_off_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let shots: [(&str, fn() -> crate::App); 2] = [("spin-off.png", || spun("lead-fork", coord_types::SPIN_PROTOCOL)), ("spin-source.png", || spun("lead", coord_types::SPIN_PROTOCOL))];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }


    fn agent(name: &str, part: &str, parent: &str, state: &str) -> AgentView {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({
            "id": "", "name": name, "parent": parent, "kind": "", "model": "", "state": state, "since": "2026-11-18T12:20:00Z",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null
        }))
        .unwrap();
        a.part = part.into();
        a
    }

    /// The breakdown of agents and teams on the Agents page (his N-1104 p863: "what is now on the sidebar with the
    /// breakdown of agents and teams should be moved to the page when i click on agents"; paper-research N-1446 p2008,
    /// p2016): Active holds the teams nested as parts.rs nests them, each heading with how many of its agents run and
    /// how many are stopped, a team with nobody running too; who leads first, under Leads; the running agents' tiles
    /// under their team; the stopped ones on one line, which shows All agents. A press on a heading folds the team, in
    /// the folds the sidebar kept (`App::closed_parts`), and a press again unfolds it.
    #[test]
    fn the_agents_page_holds_the_teams_with_their_counts_and_folds_and_the_tiles_under_them() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = vec![
            agent("paper-research", "", "owner", "working"),
            agent("consult-2", "Consultants", "owner", "stopped"),
            agent("docket-core", "yaaihh project/Engine", "paper-research", "idle"),
            agent("docket-core3", "yaaihh project/Engine", "docket-core", "stopped"),
            agent("docket-edge", "yaaihh project/App", "paper-research", "working"),
        ];
        p.live.parts = vec![("Consultants".to_string(), 1), ("yaaihh project/Engine".to_string(), 2), ("yaaihh project/App".to_string(), 3)];
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agents;
        let mut rt = crate::view_cost::window(s);
        crate::picture::docket::tests::frames(&mut rt);
        let els = |rt: &rune::runtime::Runtime<crate::App>| rt.elements().into_iter().filter(|e| !e.exiting && !e.path.contains("/sidebar/")).collect::<Vec<_>>();
        let drawn = els(&rt);
        let y = |key: &str| drawn.iter().find(|e| e.key.as_deref() == Some(key)).map(|e| e.rect[1]);
        let says = |key: &str| -> Vec<String> {
            let inside = format!("/{key}/");
            drawn.iter().filter(|e| e.path.contains(&inside)).filter_map(|e| e.text.clone()).collect()
        };
        let order: Vec<Option<f32>> = ["team-Leads", "tile-paper-research", "part-Consultants", "stopped-consult-2", "part-yaaihh project", "part-yaaihh project/Engine", "tile-docket-core", "stopped-docket-core3", "part-yaaihh project/App", "tile-docket-edge"]
            .into_iter()
            .map(|k| y(k))
            .collect();
        assert!(order.iter().all(Option::is_some) && order.windows(2).all(|w| w[0] < w[1]), "Leads, then each team with its own beneath it: {order:?}");
        for (key, count) in [("part-Consultants", "1 stopped"), ("part-yaaihh project", "2 running · 1 stopped"), ("part-yaaihh project/Engine", "1 running · 1 stopped"), ("part-yaaihh project/App", "1 running")] {
            assert!(says(key).iter().any(|t| t == count), "{key} says {count}: {:?}", says(key));
        }
        assert!(says("stopped-consult-2").iter().any(|t| t.contains("consult-2")), "the stopped on one line: {:?}", says("stopped-consult-2"));
        let press = |rt: &mut rune::runtime::Runtime<crate::App>, key: &str| {
            let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            crate::picture::docket::tests::frames(rt);
        };
        press(&mut rt, "part-yaaihh project");
        let key = ("research".to_string(), "yaaihh project".to_string());
        assert!(rt.state.closed_parts.contains(&key), "folded, in the sidebar's folds");
        let drawn = els(&rt);
        let has = |k: &str| drawn.iter().any(|e| e.key.as_deref() == Some(k));
        assert!(has("part-yaaihh project") && !has("tile-docket-core") && !has("tile-docket-edge") && !has("part-yaaihh project/App") && has("tile-paper-research"), "its team folded, the rest as it was");
        press(&mut rt, "part-yaaihh project");
        assert!(!rt.state.closed_parts.contains(&key) && rt.find("tile-docket-edge").is_some(), "unfolded again");
        press(&mut rt, "stopped-consult-2");
        assert_eq!(rt.state.agents_tab, 1, "the stopped line shows All agents");
    }

    /// The agents of [`all_agents_draws_each_name_whole`], each on an account as coordd's line names it, two of whose
    /// labels start as a third's does.
    const NAMED: [(&str, &str); 4] = [
        ("paper-research", "Claude Max · personal"),
        ("paper-research-fork", "Claude Max · second seat"),
        ("paper-research-fork2", "Claude Team · seat 2"),
        ("docket-core", "google personal account"),
    ];

    fn named_agents() -> (std::sync::MutexGuard<'static, ()>, crate::App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = NAMED
            .iter()
            .map(|(n, account)| {
                let mut a = agent(n, "", "owner", "working");
                a.account = Some(account.to_string());
                a
            })
            .collect();
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agents;
        s.agents_tab = 1;
        (held, s)
    }

    /// All agents draws each name whole, in a window 1400 wide (docket-edge8's walk of 73, N-1446 p2353: AGT-004,
    /// AGT-008 and AGT-009 each read "paper-resea…", so three rows read as one agent's), and each account whole
    /// (proto-window's walk of 76, N-1446 p2731: "Claude…" and "Team s…" said which account none of them was on; his
    /// N-1104 p425: "which agents is in which account").
    #[test]
    fn all_agents_draws_each_name_whole() {
        let (_held, s) = named_agents();
        let mut rt = crate::view_cost::window(s);
        crate::picture::docket::tests::frames(&mut rt);
        let font = rune_kit::TableStyle::from_kit(&rune_kit::Kit::neutral(false)).font;
        for words in NAMED.iter().flat_map(|(name, account)| [*name, *account]) {
            let e = rt.elements().into_iter().find(|e| !e.exiting && e.path.contains("/all-frame/") && e.text.as_deref() == Some(words)).unwrap_or_else(|| panic!("{words} in the table"));
            let need = rune::text::measure(words, &[], font, None).0;
            assert!(e.rect[2] + 0.5 >= need, "{words} whole: {} wide for {need}", e.rect[2]);
        }
    }

    /// All agents, 1400 wide, on [`NAMED`]'s agents and accounts (his N-1104 p425).
    /// `STILL_OUT=DIR … agents::team_tests::all_agents_accounts_still --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn all_agents_accounts_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> crate::App {
            let (_held, mut s) = named_agents();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            s
        }
        let args = vec!["--snap".to_string(), out.join("all-agents-accounts.png").display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    /// The walk's agents (N-1446 p2353 #4) on All agents: paper-research a lead with its team, docket-core a lead
    /// waiting on its job, proto-window a window of paper-research.
    fn walks_agents() -> (std::sync::MutexGuard<'static, ()>, crate::App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        let mut lead = agent("paper-research", "", "owner", "working");
        lead.role = "lead".into();
        lead.team = Some(coord_types::TeamGrant { granted: 20, used: 20, by: "owner".into() });
        let mut core = agent("docket-core", "", "paper-research", "waiting on shell bash_7");
        core.role = "lead".into();
        core.team = Some(coord_types::TeamGrant { granted: 8, used: 6, by: "paper-research".into() });
        let at = (chrono::Utc::now() - chrono::Duration::minutes(6)).to_rfc3339();
        core.background = serde_json::from_value(serde_json::json!([{"id": "bash_7", "tool": "Bash", "what": "cargo test -p coordd", "at": at, "about": "Run the tests"}])).unwrap();
        let mut window = agent("proto-window", "", "paper-research", "idle");
        window.role = "window".into();
        window.member_of = Some("paper-research".into());
        p.live.agents = vec![lead, core, window];
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agents;
        s.agents_tab = 1;
        (held, s)
    }

    /// All agents on a page wider than its columns (accounts-lead's hold, N-1446 p2595): the table fills the page as
    /// before the sideways scroll, its last column ending at the frame's edge, and a sideways wheel moves nothing.
    #[test]
    fn all_agents_fills_a_page_wider_than_its_columns() {
        let (_held, s) = walks_agents();
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (2400.0, 900.0), rune::Env::new(false, 1.0));
        rt.frame(0.0);
        crate::picture::docket::tests::frames(&mut rt);
        let rect = |rt: &rune::runtime::Runtime<crate::App>, end: &str| rt.elements().into_iter().find(|e| !e.exiting && e.path.contains("/all-frame/") && e.path.ends_with(end)).unwrap_or_else(|| panic!("{end}")).rect;
        let frame = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("all-frame")).unwrap().rect;
        let spend = rect(&rt, "/all-agents/header/spend");
        assert!((spend[0] + spend[2] - (frame[0] + frame[2])).abs() < 2.0, "Spend ends at the frame's edge: {spend:?} in {frame:?}");
        rt.wheel_xy(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0, -4000.0, 0.0);
        for _ in 0..30 {
            rt.frame(0.016);
        }
        assert_eq!(rect(&rt, "/all-agents/header/spend"), spend, "nothing to scroll to");
    }

    /// All agents, 1400 wide, on the walk's roles and states (docket-edge8's walk of 73, N-1446 p2353 #4: "lead · team
    /// 20/20 by o…", "waiting…"): each role whole, each parent whole, each state's first words whole, the rest of a
    /// waiting one cut; and every column in reach, the table wider than the page scrolled sideways to Spend, which the
    /// page's edge cut off.
    #[test]
    fn all_agents_draws_roles_and_states_whole_and_scrolls_to_spend() {
        let (_held, s) = walks_agents();
        let mut rt = crate::view_cost::window(s);
        crate::picture::docket::tests::frames(&mut rt);
        let font = rune_kit::TableStyle::from_kit(&rune_kit::Kit::neutral(false)).font;
        let measure = |t: &str| rune::text::measure(t, &[], font, None).0;
        let cell = |rt: &rune::runtime::Runtime<crate::App>, name: &str, column: &str| {
            let at = format!("/all-agents/viewport/rows/{name}/{column}/");
            rt.elements().into_iter().find(|e| !e.exiting && e.path.contains("/all-frame/") && e.path.contains(&at) && e.text.is_some()).unwrap_or_else(|| panic!("{name}'s {column} in the table"))
        };
        let words = |rt: &rune::runtime::Runtime<crate::App>, name: &str| {
            let p = &rt.state.projects[0];
            let a = p.live.agents.iter().find(|a| a.name == name).unwrap();
            (super::role_text(a), super::state_cell(p, a))
        };
        assert!(words(&rt, "docket-core").1.starts_with("waiting on its job · Run the tests · "), "{:?}", words(&rt, "docket-core").1);
        for name in ["paper-research", "docket-core", "proto-window"] {
            let (role, state) = words(&rt, name);
            let e = cell(&rt, name, "role");
            assert_eq!(e.text.as_deref(), Some(role.as_str()));
            assert!(e.rect[2] + 0.5 >= measure(&role), "{name}'s role whole: {} wide for {} ({role})", e.rect[2], measure(&role));
            let e = cell(&rt, name, "parent");
            let parent = e.text.clone().unwrap();
            assert!(e.rect[2] + 0.5 >= measure(&parent), "{name}'s parent whole: {} wide for {} ({parent})", e.rect[2], measure(&parent));
            let e = cell(&rt, name, "state");
            assert_eq!(e.text.as_deref(), Some(state.as_str()), "the cell holds all its words, for its hover");
            let head = super::state_head(&state);
            assert!(e.rect[2] + 0.5 >= measure(&head), "{name}'s state, its first words whole: {} wide for {} ({head})", e.rect[2], measure(&head));
        }
        assert_eq!(super::state_head("waiting on its job · Run the tests · 6 m"), "waiting on its job …");
        assert_eq!((super::state_head("not started, no box: no image"), super::state_head("idle")), ("not started, no box …".to_string(), "idle".to_string()));
        // in reach: wider than the page, then scrolled to its end
        let rect = |rt: &rune::runtime::Runtime<crate::App>, end: &str| rt.elements().into_iter().find(|e| !e.exiting && e.path.contains("/all-frame/") && e.path.ends_with(end)).unwrap_or_else(|| panic!("{end}")).rect;
        let frame = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("all-frame")).unwrap().rect;
        let (name, spend) = (rect(&rt, "/all-agents/header/name"), rect(&rt, "/all-agents/header/spend"));
        assert!((name[0] - frame[0]).abs() < 2.0 && spend[0] + spend[2] > frame[0] + frame[2], "the walk's columns are wider than the page: name {name:?}, spend {spend:?}, frame {frame:?}");
        rt.wheel_xy(frame[0] + frame[2] / 2.0, frame[1] + frame[3] / 2.0, -4000.0, 0.0);
        for _ in 0..30 {
            rt.frame(0.016);
        }
        let (name, spend) = (rect(&rt, "/all-agents/header/name"), rect(&rt, "/all-agents/header/spend"));
        assert!(spend[0] >= frame[0] && spend[0] + spend[2] <= frame[0] + frame[2] + 0.5, "Spend in the page, scrolled to: {spend:?} in {frame:?}");
        assert!(name[0] < frame[0], "Name scrolled out on the left: {name:?}");
        assert!(rect(&rt, "/all-agents/viewport/rows/docket-core/spend")[0] >= frame[0], "the rows scroll with the header");
    }

    #[test]
    fn the_tiles_follow_the_teams_in_the_sidebars_order() {
        let agents = vec![
            agent("paper-research", "", "owner", "working"),
            agent("docket-edge", "yaaihh project/App", "paper-research", "working"),
            agent("docket-core", "yaaihh project/Engine", "paper-research", "idle"),
            agent("docket-core2", "yaaihh project/Engine", "docket-core", "working"),
            agent("docket-core3", "yaaihh project/Engine", "docket-core", "stopped"),
        ];
        let parts = vec![("yaaihh project/Engine".to_string(), 1), ("yaaihh project/App".to_string(), 2)];
        let got: Vec<(String, Vec<&str>)> = super::teams(&agents, &parts).into_iter().map(|(t, v)| (t, v.into_iter().map(|a| a.name.as_str()).collect())).collect();
        assert_eq!(
            got,
            vec![
                ("Leads".to_string(), vec!["paper-research"]),
                ("Engine".to_string(), vec!["docket-core", "docket-core2"]),
                ("App".to_string(), vec!["docket-edge"]),
            ]
        );
    }

    /// The line names the ticket coordd says it is on, by that ticket's label; a coordd without the field leaves the
    /// label's "WORD of THREAD" to be read.
    #[test]
    fn the_line_names_the_ticket_the_view_says_it_is_on() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let label = |id: &str, l: &str| serde_json::json!({"id": id, "is": "x", "label": l}).to_string();
        p.labels.took(true, &[label("pr", "lead"), label("N-510", "Agents and context"), label("c2", "to of Builds")].join("\n"));
        let mut a = agent("pr", "", "owner", "working");
        a.on = Some("N-510".into());
        let r = super::working_on(&p, &a).unwrap();
        assert_eq!((r.label.as_str(), r.id.as_str()), ("Agents and context", "N-510"), "a role's label, but on N-510");
        a.on = Some("N-9".into());
        assert_eq!(super::working_on(&p, &a).unwrap().label, "N-9", "a ticket without a label held: its id");
        let mut old = agent("c2", "", "", "working");
        assert_eq!(super::working_on(&p, &old).unwrap().label, "Builds", "no field: the label read");
        old.name = "pr".into();
        assert!(super::working_on(&p, &old).is_none(), "on nothing: a role names no ticket");
    }

    /// An agent held on a rate limit (proto-lead N-1397 p160): the sidebar and its line say so and until when, in his
    /// local time, whether its state is idle or waiting on something, not "idle since …"; so do the All agents table
    /// and the circle's panel (`streams::state_words`). Its view decoded as coordd sends it; one not held, as before.
    /// Never a time gone by as still to come (proto-lead N-1468, N-1397 p167): at a fixed time, before the reset, and
    /// after it with his mail hold on and with none; and so through its row, his hold read from the project.
    #[test]
    fn an_agent_held_on_a_rate_limit_says_so_and_until_when() {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let ahead = chrono::Utc::now().timestamp() + 18 * 60;
        let words = format!("rate-limited · resumes {}", crate::when::secs(ahead));
        for state in ["idle", "waiting on N-12"] {
            let a = agent("w2", "", "lead", state);
            assert_ne!(super::side_line(&p, &a), Some(words.clone()), "{state}: not held");
            let mut v = serde_json::to_value(&a).unwrap();
            v["limit_until"] = ahead.into();
            v["limit_why"] = "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (reset after 18m48s)".into();
            let a: AgentView = serde_json::from_value(v).unwrap();
            assert_eq!(super::held(&p, &a), Some(words.clone()), "{state}");
            assert_eq!(super::side_line(&p, &a), Some(words.clone()), "{state}: the sidebar");
            assert_eq!(super::idle_since(&p, &a), words, "{state}: its line");
            assert_eq!(crate::streams::state_words(&p, &a), words, "{state}: the table and the circle's panel");
        }
        // at a fixed time
        let until = 1_795_000_000;
        let (a, at) = (AgentView { limit_until: Some(until), ..agent("w2", "", "lead", "idle") }, crate::when::secs(until));
        assert_eq!(super::held_at(&a, until - 60, true), Some(format!("rate-limited · resumes {at}")), "before the reset");
        assert_eq!(super::held_at(&a, until, true), Some(format!("rate-limited · resets {at} · held")));
        assert_eq!(super::held_at(&a, until + 3600, false), Some(format!("rate-limited · resets {at} · resuming")));
        // the provider overloaded, or a server error, as coordd words it (the owner's walk item 14): what stopped it,
        // and when it tries again
        for w in ["the provider overloaded", "a server error"] {
            let a = AgentView { limit_until: Some(until), limit_why: Some(format!("{w}: API Error: 529 Overloaded")), ..agent("w2", "", "lead", "idle") };
            assert_eq!(super::held_at(&a, until - 60, false), Some(format!("stopped: {w} · tries again at {at}")));
            assert_eq!(super::held_at(&a, until, true), Some(format!("stopped: {w} · held")));
            assert_eq!(super::held_at(&a, until + 5, false), Some(format!("stopped: {w} · resuming")));
        }
        // through its row: his hold, then none
        let gone = chrono::Utc::now().timestamp() - 40 * 60;
        let past = AgentView { limit_until: Some(gone), ..agent("w2", "", "lead", "idle") };
        p.live.mail_hold = Some("2026-11-21T04:00:00Z".into());
        let said = format!("rate-limited · resets {} · held", crate::when::secs(gone));
        assert_eq!((super::side_line(&p, &past), crate::streams::state_words(&p, &past)), (Some(said.clone()), said));
        p.live.mail_hold = None;
        assert_eq!(super::idle_since(&p, &past), format!("rate-limited · resets {} · resuming", crate::when::secs(gone)));
        let idle = agent("w3", "", "lead", "idle");
        assert_eq!((super::held(&p, &idle), super::side_line(&p, &idle)), (None, Some(super::idle_since(&p, &idle))));
        assert!(super::idle_since(&p, &idle).starts_with("idle since "), "{}", super::idle_since(&p, &idle));
    }

    /// The window wakes at a held agent's reset, coordd sending nothing then (proto-reader, proto-lead N-1397 p167):
    /// while one is held the next wake it asks for is the soonest reset, once; at it, the poll builds the view again,
    /// and the row reads that the reset has come; then the next reset, and none once none is ahead. rune counts the
    /// wait from its clock, read before the poll reads the time: a wake that comes a second early (`until - 1`) asks
    /// again, its new deadline not before `until` (consult-3 N-1468 p11; proto-window's steps, p15). So for the
    /// Agents page's 10 s tick (`App::wake_every`): kept on rune's clock, a poll just before it asks nothing more,
    /// and the wake's own poll, at it by rune's clock, asks the next.
    #[test]
    fn the_window_wakes_at_a_held_agents_reset_and_its_row_says_it_came() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = crate::view_cost::project_for_tests();
        let until = 1_795_000_000;
        let held = |name: &str, t: i64| AgentView { limit_until: Some(t), ..agent(name, "", "lead", "idle") };
        p.live.agents = vec![held("w4", until + 600), held("w2", until), agent("w3", "", "lead", "idle")];
        s.projects = vec![p];
        let mut tx = rune::Tx::default();
        // NOW the wall's second, CLOCK rune's: here rune's clock 100 is the wall's `until - 60`
        let poll = |s: &mut crate::App, now: i64, clock: f64, tx: &mut rune::Tx| s.wake_at(super::next_reset(s, now), now, clock, tx);
        assert!(!poll(&mut s, until - 60, 100.0, &mut tx), "before it: nothing came");
        assert_eq!(s.tick_at, Some((until, 160.0)), "the next wake asked: the soonest reset");
        assert!(!poll(&mut s, until - 30, 130.0, &mut tx));
        assert_eq!(s.tick_at, Some((until, 160.0)), "asked once");
        // the wake comes by rune's clock with the wall's second still `until - 1`: asked again, not before `until`
        assert!(!poll(&mut s, until - 1, 160.0, &mut tx), "early: nothing came yet");
        assert_eq!(s.tick_at, Some((until, 161.0)), "asked again, a second on");
        assert!(poll(&mut s, until, 161.0, &mut tx), "its reset came: the view built again");
        let w2 = s.projects[0].live.agents.iter().find(|a| a.name == "w2").unwrap();
        let at = crate::when::secs(until);
        assert_eq!(super::held_at(w2, until, true), Some(format!("rate-limited · resets {at} · held")));
        assert_eq!(super::held_at(w2, until, false), Some(format!("rate-limited · resets {at} · resuming")));
        assert_eq!(s.tick_at, Some((until + 600, 761.0)), "then the next reset");
        assert!(poll(&mut s, until + 600, 761.0, &mut tx));
        assert_eq!(s.tick_at, None, "none ahead: none asked");
        assert!(!poll(&mut s, until + 700, 861.0, &mut tx));
        // the 10 s tick on rune's clock
        let ten = std::time::Duration::from_secs(10);
        s.wake_every_at(ten, 100.0, &mut tx);
        assert_eq!(s.ticks.get(&ten), Some(&110.0), "asked");
        s.wake_every_at(ten, 109.9, &mut tx);
        assert_eq!(s.ticks.get(&ten), Some(&110.0), "a poll just before it: still asked, nothing more");
        s.wake_every_at(ten, 110.0, &mut tx);
        assert_eq!(s.ticks.get(&ten), Some(&120.0), "its wake's poll asks the next");
    }
}

#[cfg(test)]
mod esc_all_tests {
    use crate::{App, Sel};
    use coord_types::AgentView;

    fn agent(name: &str, kind: &str, state: &str) -> AgentView {
        serde_json::from_value(serde_json::json!({
            "id": "", "name": name, "parent": "owner", "kind": kind, "model": "", "state": state, "since": "2026-11-20T07:00:00Z",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null
        }))
        .unwrap()
    }

    /// An App on the Agents page of project `research`, its agents AGENTS, its coordd at PROTOCOL.
    fn page(agents: Vec<AgentView>, protocol: u32) -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = protocol;
        p.live.agents = agents;
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agents;
        (held, s)
    }

    fn drawn(rt: &rune::runtime::Runtime<App>, label: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| e.label.as_deref() == Some(label) || e.text.as_deref() == Some(label)).map(|e| e.rect)
    }

    /// Line 200 (consult-lead, N-1324 post 7): two agents in a turn and one idle. Esc all is drawn at the page's head;
    /// a press sends one request that names nobody (coordd picks who is in a turn, not the window's snapshot), and the
    /// notice says the count coordd's reply gave, with the time.
    #[test]
    fn esc_all_sends_one_request_naming_nobody_and_says_the_count() {
        use std::io::BufRead;
        let (_held, mut s) = page(vec![agent("w1", "claude", "working"), agent("w2", "codex", "working"), agent("w3", "claude", "idle")], coord_types::ESC_ALL_PROTOCOL);
        // the pipe: what the window writes comes back on cat's stdout
        let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let mut back = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        let r = drawn(&rt, "Esc all").expect("drawn while two are in a turn");
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        // the page asks for its own reads too (an agent's transcript): the press's request is the escall among them
        let mut others = Vec::new();
        let (sent, line) = loop {
            let mut line = String::new();
            back.read_line(&mut line).unwrap();
            let sent: serde_json::Value = serde_json::from_str(&line).unwrap();
            if sent["req"]["op"] == "escall" {
                break (sent, line);
            }
            others.push(line);
            assert!(others.len() < 20, "no escall among {others:?}");
        };
        assert_eq!(sent["req"], serde_json::json!({"op": "escall"}), "one request, naming nobody: {line}");
        let waits: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| matches!(w, crate::Pending::EscAll)).map(|(id, _)| *id).collect();
        assert_eq!(waits, [sent["id"].as_u64().unwrap()], "one press, one request waiting for its reply");
        rt.state.take(0, crate::link::Incoming::Reply(waits[0], coord_types::Reply { ok: true, out: "being stopped: w1, w2".into(), post: None }));
        let said = rt.state.notice.clone().unwrap_or_default();
        assert!(said.starts_with("Esc sent to 2 agents · "), "{said}");
        let _ = child.kill();
        let _ = child.wait();
    }

    /// With nobody in a turn there is no Esc all (the engine's "nothing stopped" is for the shell); nor on a coordd from
    /// before `ESC_ALL_PROTOCOL`, which would refuse the request.
    #[test]
    fn no_esc_all_with_nobody_in_a_turn_or_an_older_coordd() {
        // each page in its own scope: `page` holds the tests' one lock (`view_cost::test_app`) until its guard goes
        {
            let (_held, s) = page(vec![agent("w1", "claude", "idle"), agent("run", "program", "working")], coord_types::ESC_ALL_PROTOCOL);
            let rt = crate::view_cost::window(s);
            assert!(drawn(&rt, "Esc all").is_none(), "nobody in a turn: no button");
        }
        let (_held, s) = page(vec![agent("w1", "claude", "working")], coord_types::ESC_ALL_PROTOCOL - 1);
        let rt = crate::view_cost::window(s);
        assert!(drawn(&rt, "Esc all").is_none(), "an older coordd: no button");
    }

    #[test]
    fn the_notice_counts_what_the_reply_names() {
        let now = chrono::Local::now();
        let at = now.format("%H:%M").to_string();
        assert_eq!(super::esc_all_said(true, "being stopped: a, b, c; already being stopped: d", now), format!("Esc sent to 3 agents · {at}"));
        assert_eq!(super::esc_all_said(true, "being stopped: a", now), format!("Esc sent to 1 agent · {at}"));
        assert_eq!(super::esc_all_said(true, "already being stopped: d, e", now), format!("Esc already sent to 2 agents · {at}"));
        assert_eq!(super::esc_all_said(true, "No agent is in a turn: nothing was stopped.", now), format!("No agent was in a turn · {at}"));
        assert_eq!(super::esc_all_said(false, "coord esc --all: no", now), "coord esc --all: no");
    }

    /// The request a press of LABEL in S's window writes (a `cat` pipe stands for the link).
    fn pressed(mut s: App, label: &str) -> serde_json::Value {
        use std::io::BufRead;
        let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let mut back = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        let r = drawn(&rt, label).unwrap_or_else(|| panic!("{label} is not drawn"));
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        // the page's own reads (an agent's transcript) go on the pipe too: the press's request is the first other one
        let req = loop {
            let mut line = String::new();
            back.read_line(&mut line).unwrap();
            let req = serde_json::from_str::<serde_json::Value>(&line).unwrap()["req"].clone();
            if req["op"] != "transcript" {
                break req;
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        req
    }

    /// Line 327 (consult-lead, N-1324 posts 7 and 12): Hold is at the page's head on a coordd that has it, whoever is
    /// in a turn, and its press sends `mailhold` on. While the hold is on, the muted line under the heading says since
    /// when and that what he sends still reaches them, and Release sends `mailhold` off.
    #[test]
    fn hold_switches_agents_mail_on_and_release_lets_it_go() {
        {
            let (_held, s) = page(vec![agent("w1", "claude", "idle")], coord_types::MAIL_HOLD_PROTOCOL);
            assert_eq!(pressed(s, "Hold agents' mail"), serde_json::json!({"op": "mailhold", "on": true}));
        }
        let since = "2026-11-20T06:20:00Z";
        {
            let (_held, mut s) = page(vec![agent("w1", "claude", "working")], coord_types::MAIL_HOLD_PROTOCOL);
            s.projects[0].live.mail_hold = Some(since.into());
            let rt = crate::view_cost::window(s);
            assert!(drawn(&rt, &super::held_words(since)).is_some(), "the held line");
            assert!(drawn(&rt, "Hold agents' mail").is_some() && drawn(&rt, "Esc all").is_some(), "Hold, its words saying what it holds (p3207, 5), beside Esc all");
        }
        let (_held, mut s) = page(vec![agent("w1", "claude", "idle")], coord_types::MAIL_HOLD_PROTOCOL);
        s.projects[0].live.mail_hold = Some(since.into());
        assert_eq!(pressed(s, "Release"), serde_json::json!({"op": "mailhold", "on": false}));
    }

    /// On a coordd from before `MAIL_HOLD_PROTOCOL` there is no Hold and no held line, and Esc all's hover keeps its
    /// first words; from it, the hover says how to keep them stopped and, once, what Hold does not keep. The line's
    /// words read his clock.
    #[test]
    fn no_hold_on_an_older_coordd_and_esc_alls_hover_follows() {
        {
            let (_held, mut s) = page(vec![agent("w1", "claude", "working")], coord_types::MAIL_HOLD_PROTOCOL - 1);
            s.projects[0].live.mail_hold = Some("2026-11-20T06:20:00Z".into());
            assert_eq!(super::esc_all_help(&s.projects[0]), "Interrupt all turns");
            let rt = crate::view_cost::window(s);
            assert!(drawn(&rt, "Hold agents' mail").is_none() && drawn(&rt, "Release").is_none(), "an older coordd: no Hold");
        }
        let (_held, s) = page(vec![agent("w1", "claude", "working")], coord_types::MAIL_HOLD_PROTOCOL);
        assert_eq!(super::esc_all_help(&s.projects[0]), "Interrupt all turns · Hold mail keeps them");
        let clock = chrono::DateTime::parse_from_rfc3339("2026-11-20T06:20:00Z").unwrap().with_timezone(&chrono::Local).format("%H:%M").to_string();
        let words = super::held_words("2026-11-20T06:20:00Z");
        assert!(words.starts_with("Agents' mail held since ") && words.contains(&clock) && !words.contains(" · "), "{words}");
    }

    /// Brain on an agent's card (proto-lead N-1446 p857.2, proto-mind N-1475): from `BRAIN_PROTOCOL`, only on an agent
    /// whose program binds a brain. Its press sends `brain` with the agent and the other state; beside it, off since
    /// when, and its last pass's time, which opens that pass. None on an older coordd, none for an agent with no brain.
    #[test]
    fn brain_switches_an_agents_passes_and_says_its_last_pass() {
        let (pass, pressed_off) = ("2026-11-21T09:42:00Z", "2026-11-21T10:20:00Z");
        let with = |on: bool| {
            let mut a = agent("paper-research", "claude", "idle");
            a.brain = Some(coord_types::BrainView { on, since: (!on).then(|| pressed_off.to_string()), last_pass: Some(pass.into()), last_id: Some("B-12".into()), ready: Vec::new() });
            a
        };
        {
            let (_held, s) = page(vec![with(true)], coord_types::BRAIN_PROTOCOL);
            assert_eq!(pressed(s, "Brain"), serde_json::json!({"op": "brain", "agent": "paper-research", "on": false}));
        }
        {
            let (_held, s) = page(vec![with(false)], coord_types::BRAIN_PROTOCOL);
            assert_eq!(pressed(s, "Brain"), serde_json::json!({"op": "brain", "agent": "paper-research", "on": true}));
        }
        // the words beside it, on his clock, written as `when::at` writes a time on any day the test runs ("off since
        // 11:20" on 21 Nov, "off since Sat 11:20" on the 22nd: consult-3 N-1446 p908, proto-reader p912), each ending in its clock
        let clock = |t: &str| chrono::DateTime::parse_from_rfc3339(t).unwrap().with_timezone(&chrono::Local).format("%H:%M").to_string();
        let (off, last) = (format!("off since {}", crate::when::at(pressed_off)), format!("last pass {}", crate::when::at(pass)));
        assert!(off.ends_with(&clock(pressed_off)) && last.ends_with(&clock(pass)), "{off} · {last}");
        assert_eq!(super::brain_words(with(false).brain.as_ref().unwrap()), (Some(off.clone()), last.clone()));
        assert_eq!(super::brain_words(&coord_types::BrainView { on: true, ..Default::default() }), (None, "no pass yet".to_string()));
        // drawn on its card, and on no other: w2 binds no brain
        {
            let (_held, s) = page(vec![with(false), agent("w2", "claude", "idle")], coord_types::BRAIN_PROTOCOL);
            let mut rt = crate::view_cost::window(s);
            rt.frame(0.016);
            let switches = rt.elements().into_iter().filter(|e| e.label.as_deref() == Some("Brain")).count();
            assert!(switches == 1 && drawn(&rt, &off).is_some(), "one switch, off since when ({switches})");
            // its last pass's time opens that pass
            rt.update(|s, _| {
                let v: coord_types::TicketView = serde_json::from_value(serde_json::json!({"id": "B-12", "n": 12, "kind": "brain", "opener": "paper-research", "ctx": {}, "form": {},
                    "title": "a pass", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": pass, "given": [], "posts": []})).unwrap();
                s.projects[0].live.tickets.insert(12, v);
            });
            rt.frame(0.016);
            let at = |rt: &rune::runtime::Runtime<App>| drawn(rt, &last).map(|r| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)).expect("its last pass drawn");
            let (x, y) = at(&rt);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            // the tile under the pointer shows its Show switch in its head, which moves the rows under it
            let (x, y) = at(&rt);
            rt.pointer_move(x, y);
            rt.frame(0.016);
            rt.pointer_down(x, y);
            rt.frame(0.016);
            rt.pointer_up(x, y);
            rt.frame(0.016);
            assert_eq!((&rt.state.sel, rt.state.tickets.shown), (&Sel::Tickets, Some(12)), "the pass opened");
        }
        // an older coordd: no switch
        let (_held, s) = page(vec![with(true)], coord_types::BRAIN_PROTOCOL - 1);
        let rt = crate::view_cost::window(s);
        assert!(drawn(&rt, "Brain").is_none(), "an older coordd");
    }

    /// Stills of Brain on made-up paper-research's card, docket-core's beside it with none
    /// (`STILL_OUT=DIR yaaihh-tests agents::esc_all_tests::brain_stills --ignored --exact`): on, its last pass at 09:42 UTC
    /// (brain-on.png); off since 10:20 UTC (brain-off.png).
    #[test]
    #[ignore]
    fn brain_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn app(on: bool) -> App {
            let (_held, mut s) = page(Vec::new(), coord_types::BRAIN_PROTOCOL);
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut a = agent("paper-research", "claude", "idle");
            a.brain = Some(coord_types::BrainView { on, since: (!on).then(|| "2026-11-21T10:20:00Z".into()), last_pass: Some("2026-11-21T09:42:00Z".into()), last_id: Some("B-12".into()), ready: Vec::new() });
            s.projects[0].live.agents = vec![a, agent("docket-core", "codex", "working")];
            s
        }
        fn on() -> App {
            app(true)
        }
        fn off() -> App {
            app(false)
        }
        for (name, start) in [("brain-on.png", on as fn() -> App), ("brain-off.png", off)] {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1600x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}

#[cfg(test)]
#[path = "agents/waiting_tests.rs"]
mod waiting_tests;
