//! The Inspect drawer: what an agent's model is sent, beside its chat (drawer.rs), in seven tabs over one footer.
//!
//! - **Diff**: only what its newest request adds to the one before, with its tokens estimated.
//! - **Docket**: the rules of its harness's bridge under coordd-v2, moment by moment (programs/agent.rs).
//! - **System**: its system prompt's parts, each On or Off or his own text (sidebar.rs `parts_rows`).
//! - **Tools**: the tools of its newest request and those switched off, a switch each where its harness takes one.
//! - **Policy & Context**: its context ring, its model, effort and permissions (Safe, Guarded or Yolo).
//! - **Messages**: its newest request's messages, each whole.
//! - **Raw JSON**: the raw pane, its newest request in a window of its own by Pop out.
//!
//! The footer counts what is staged, Reset drops it, Branch from turn picks where it goes on from, Apply sends it.

use crate::App;
use coord_types::{AgentView, HarnessSwitchesView};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, Switch};

/// The drawer's tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Tab {
    #[default]
    Diff,
    Docket,
    System,
    Tools,
    Policy,
    Messages,
    Raw,
}

impl Tab {
    const ALL: [Tab; 7] = [Tab::Diff, Tab::Docket, Tab::System, Tab::Tools, Tab::Policy, Tab::Messages, Tab::Raw];
    const WORDS: [&'static str; 7] = ["Diff", "Docket", "System", "Tools", "Policy & Context", "Messages", "Raw JSON"];
}

/// How many characters a token is taken to be, for an estimate where no count is given.
const CHARS_A_TOKEN: usize = 4;

/// The tab agent NAME's drawer in project PI shows.
pub fn tab(s: &App, pi: usize, name: &str) -> Tab {
    s.drawers.tabs.get(&(pi, name.to_string())).copied().unwrap_or_default()
}

/// The Inspect drawer of agent A in project PI: its tabs, the one picked, and the footer.
pub fn view(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let name = a.name.clone();
    let at = tab(s, pi, &name);
    // a button each, as wide as its words, the one shown lit as the drawers' toggles are (drawer.rs): six equal
    // segments, each as wide as "Policy & Context", ran past the drawer's edge (the still, 25 Nov)
    let tabs = row().gap(2.0).center_y().key(format!("inspect-tabs-{name}")).children(Tab::ALL.iter().zip(Tab::WORDS).enumerate().map(|(i, (t, w))| {
        let (key, t) = ((pi, name.clone()), *t);
        Button::new(w)
            .variant(if t == at { ButtonVariant::Outline } else { ButtonVariant::Ghost })
            .on_press(move |s: &mut App| {
                s.drawers.tabs.insert(key.clone(), t);
            })
            .build(k)
            .key(format!("inspect-tab-{i}-{name}"))
    }));
    // the System tab's rows are made whichever tab shows: they say what his draft makes, which the footer counts and
    // sends (prompt_edit.rs `made`)
    *s.prompt_edit.made.borrow_mut() = None;
    let system = crate::sidebar::folder_part_in(s, pi, &name, &crate::lens::Filter::SystemPrompt { agent: name.clone() }, true, k);
    let scrolled = |c: El| col().w_full().h_full().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c);
    let body = match at {
        Tab::Diff => scrolled(diff(s, pi, a, k)),
        Tab::Docket => scrolled(crate::programs::agent::tab(s, a, k)),
        Tab::System => system,
        Tab::Tools => scrolled(tools(s, pi, a, k)),
        Tab::Policy => scrolled(policy(s, pi, a, k)),
        Tab::Messages => scrolled(crate::raw::newest_messages(s, pi, &name, k)),
        Tab::Raw => raw(s, pi, &name, k),
    };
    col()
        .key(format!("inspect-{pi}-{name}"))
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(row().w_full().px(12.0).pb(8.0).child(tabs))
        .child(rune_kit::Divider::horizontal().build(k))
        .child(col().w_full().grow(1.0).min_h(0.0).key(format!("inspect-body-{name}")).child(body))
        .child(footer(s, pi, a, k))
}

/// N tokens with its sign, as a change is said: "+1.2k", "−300", "±0".
fn signed(n: i64) -> String {
    match n {
        0 => "±0".into(),
        n if n > 0 => format!("+{}", crate::circle::tokens(n as u64)),
        n => format!("−{}", crate::circle::tokens(n.unsigned_abs())),
    }
}

/// Characters C as tokens, estimated.
fn est(c: usize) -> u64 {
    c.div_ceil(CHARS_A_TOKEN) as u64
}

/// Diff: how much the newest request adds, then the request, what it repeats folded.
fn diff(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let log = s.raw.logs.get(&(pi, a.name.clone()));
    let mut head = col().w_full().px(16.0).pt(10.0).gap(2.0);
    let words = match log.and_then(crate::raw::new_chars) {
        Some((new, all, true)) => Some(format!("≈ {} new tokens of ≈ {}", crate::circle::tokens(est(new)), crate::circle::tokens(est(all)))),
        Some((_, all, false)) => Some(format!("First request: ≈ {} tokens", crate::circle::tokens(est(all)))),
        None => None,
    };
    if let Some(w) = words {
        let help = format!("Estimated: {CHARS_A_TOKEN} characters a token");
        head = head.child(note(w).key(format!("inspect-diff-tokens-{}", a.name)).help(help));
    }
    if let Some(c) = &a.context {
        head = head.child(note(format!("Its context: {}", crate::circle::figures(a, c, crate::circle::whole(a)))));
    }
    col().w_full().pb(12.0).child(head).child(crate::raw::newest(s, pi, &a.name, k))
}

/// Harness switches staged for an agent; each key set replaces the agent's own (`Request::HarnessSwitches`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Switches {
    pub tools_off: Option<Vec<String>>,
}

/// Whether project PI's coordd takes harness switches and the harness can take this one (CAN).
fn takes(s: &App, pi: usize, can: bool) -> bool {
    can && s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CONTROL_PROTOCOL)
}

/// The switches staged for agent NAME in project PI.
fn switches(s: &App, pi: usize, name: &str) -> Switches {
    s.drawers.switches.get(&(pi, name.to_string())).cloned().unwrap_or_default()
}

/// Applies F to the switches staged under KEY, unstaging any key back at what the agent has (NOW).
fn stage(s: &mut App, key: &(usize, String), now: &HarnessSwitchesView, f: impl FnOnce(&mut Switches)) {
    let mut w = s.drawers.switches.remove(key).unwrap_or_default();
    f(&mut w);
    if w.tools_off.as_ref().is_some_and(|t| t.len() == now.tools_off.len() && t.iter().all(|x| now.tools_off.contains(x))) {
        w.tools_off = None;
    }
    if w != Switches::default() {
        s.drawers.switches.insert(key.clone(), w);
    }
}

/// How many of agent A's switches are staged: each tool switched.
fn switched(s: &App, pi: usize, a: &AgentView) -> usize {
    let w = switches(s, pi, &a.name);
    let now = &a.switches.tools_off;
    let tools = w.tools_off.map_or(0, |t| t.iter().filter(|x| !now.contains(x)).count() + now.iter().filter(|x| !t.contains(x)).count());
    tools
}

/// Tools: each tool of its newest request and each one off, a switch each where its harness takes one.
fn tools(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let mut c = col().w_full().px(16.0).py(10.0).gap(6.0);
    let now = a.switches.clone();
    let off = switches(s, pi, &a.name).tools_off.unwrap_or_else(|| now.tools_off.clone());
    // a tool that is off is not in the request: listed after its tools
    let mut list = crate::raw::newest_tools(s, pi, &a.name).unwrap_or_default();
    for t in now.tools_off.iter().chain(&off) {
        if !list.iter().any(|(n, _)| n == t) {
            list.push((t.clone(), String::new()));
        }
    }
    if list.is_empty() {
        return c.child(note(crate::raw::not_yet(s.raw.logs.get(&(pi, a.name.clone()))).unwrap_or_default()));
    }
    let can = takes(s, pi, a.capabilities.tool_disabling);
    let said = match now.from.get("tools_off").map(String::as_str) {
        _ if !takes(s, pi, true) => "Needs a newer engine.".to_string(),
        _ if !can => "Its harness takes no tool switched off.".to_string(),
        Some(level) if level != "agent" => format!("Applies at next start; off ones set for its {level}."),
        _ => "A switch takes effect at its next start.".to_string(),
    };
    c = c.child(note(said).key("inspect-tools-note"));
    for (i, (tool, about)) in list.iter().enumerate() {
        let on = !off.contains(tool);
        let (key, t, base) = ((pi, a.name.clone()), tool.clone(), now.clone());
        let switch = Switch::new(on)
            .hide_label()
            .label(tool.clone())
            .disabled(!can)
            .on_toggle(move |s: &mut App| {
                stage(s, &key, &base, |w| {
                    let mut off = w.tools_off.take().unwrap_or_else(|| base.tools_off.clone());
                    if off.contains(&t) {
                        off.retain(|x| *x != t);
                    } else {
                        off.push(t.clone());
                    }
                    w.tools_off = Some(off);
                })
            })
            .build(k)
            .key(format!("inspect-tool-switch-{i}"));
        let state = match (on, on == now.tools_off.contains(tool)) {
            (true, true) => " · on from its next start",
            (false, true) => " · off from its next start",
            (false, false) => " · off",
            (true, false) => "",
        };
        let words = col()
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .child(text(format!("{tool}{state}")).font(k.ty.small_medium).color(if on { k.c.text_base } else { k.c.text_muted }))
            .children((!about.is_empty()).then(|| text(about.clone()).font(k.ty.small).color(k.c.text_faint).truncate().help(about.clone())));
        c = c.child(row().w_full().gap(10.0).center_y().key(format!("inspect-tool-{i}")).child(switch).child(words));
    }
    c
}

/// Policy & Context: its context and where it compacts, then its settings.
fn policy(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let head = |t: &str| text(t.to_string()).font(k.ty.small_medium).color(k.c.text_muted);
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).wrap();
    let mut c = col().w_full().pt(10.0).gap(8.0).child(row().px(16.0).child(head("Context")));
    match (&a.context, crate::circle::control(s, pi, a, k)) {
        (Some(ctx), Some(ring)) => {
            let words = col().grow(1.0).shrink().min_w(0.0).gap(2.0).child(text(crate::circle::figures(a, ctx, crate::circle::whole(a))).font(k.ty.small).color(k.c.text_base).wrap()).child(note("Drag the ring to set compaction".into()));
            c = c.child(row().w_full().px(16.0).gap(12.0).center_y().key(format!("inspect-context-{}", a.name)).child(ring).child(words));
        }
        _ => c = c.child(row().px(16.0).child(note("Its harness gives no context figures yet.".into()))),
    }
    c.child(row().px(16.0).pt(6.0).child(head("Model and policy")))
        .child(crate::settings::body(s, pi, a, true, k).key(format!("inspect-settings-{}", a.name)))
}

/// Raw JSON: the raw pane, its newest request in a window of its own by Pop out.
fn raw(s: &App, pi: usize, name: &str, k: &Kit) -> El {
    let span = crate::raw::newest_span(s, pi, name);
    let who = name.to_string();
    let pop = Button::new("Pop out ↗")
        .variant(ButtonVariant::Ghost)
        .disabled(span.is_none())
        .help("Open its newest request")
        .on_press(move |s: &mut App| {
            let Some((since, until)) = crate::raw::newest_span(s, pi, &who) else { return };
            let title = crate::popup::title(&format!("what {who}'s model was sent"), "", "its newest request");
            crate::sent_window::open_turn(s, pi, &who, &format!("newest-{since}"), since, until, title, None);
        })
        .build(k)
        .key(format!("inspect-popout-{name}"));
    col().w_full().h_full().min_h(0.0).child(row().w_full().px(12.0).pt(6.0).justify_end().child(pop)).child(col().w_full().grow(1.0).min_h(0.0).child(crate::raw::page(s, pi, name, k)))
}

/// What agent NAME in project PI has staged: its parts, then its model, effort and harness switches.
fn staged(s: &App, pi: usize, name: &str) -> (usize, usize) {
    let switches = s.projects.get(pi).and_then(|p| p.live.agents.iter().find(|a| a.name == name)).map_or(0, |a| switched(s, pi, a));
    (crate::prompt_edit::draft_of(s, pi, name).map_or(0, |d| d.len()), crate::settings::staged(s, pi, name) + switches)
}

/// The footer: what is staged and what it changes in tokens, Reset, Branch from turn, Apply (& Step); above them the
/// engine's refusal of what was sent, or its word that it took it.
fn footer(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let name = a.name.clone();
    let (parts, settings) = staged(s, pi, &name);
    let n = parts + settings;
    let e = &s.prompt_edit;
    let made = e.made.borrow().clone().filter(|m| m.pi == pi && m.name == name);
    let running = e.running.is_some();
    let mut words = match n {
        0 => "Nothing staged".to_string(),
        1 => "1 change staged".to_string(),
        n => format!("{n} changes staged"),
    };
    if let Some(m) = made.as_ref().filter(|_| parts > 0) {
        words += &format!(" · ≈ {} tokens", signed(m.grows / CHARS_A_TOKEN as i64));
    }
    let staged_el = text(words).font(k.ty.small).color(if n > 0 { k.c.text_base } else { k.c.text_faint }).truncate().help(format!("Estimated: {CHARS_A_TOKEN} characters a token")).key(format!("inspect-staged-{name}"));
    // why Apply cannot send the parts: the engine's or the window's word, or every part empty
    let why = match (parts > 0, &made) {
        (false, _) => None,
        (true, _) if crate::prompt_edit::why_not(s, pi, &name).is_some() => crate::prompt_edit::why_not(s, pi, &name),
        (true, Some(m)) if m.version.is_none() => Some("Nothing to send: every part is empty.".to_string()),
        (true, None) => Some("Its system prompt is still being read.".to_string()),
        _ => None,
    };
    let step = s.projects.get(pi).and_then(|p| crate::slow::waiting_on(p, &name));
    let go_words = match (running, step.is_some()) {
        (true, _) => "Sending…",
        (false, true) => "Apply & Step",
        (false, false) => "Apply",
    };
    let help = match &why {
        Some(w) => w.clone(),
        None if step.is_some() => "Send staged and let its step go".to_string(),
        None => "Send staged, after its turn".to_string(),
    };
    let who = name.clone();
    let apply = Button::new(go_words).variant(ButtonVariant::Contrast).disabled(n == 0 || running || why.is_some()).help(help).on_press(move |s: &mut App| apply(s, pi, &who)).build(k).key(format!("inspect-apply-{name}"));
    let who = name.clone();
    let reset = Button::new("Reset").variant(ButtonVariant::Ghost).disabled(n == 0 || running).help("Drop everything staged").on_press(move |s: &mut App| reset(s, pi, &who)).build(k).key(format!("inspect-reset-{name}"));
    let open = s.drawers.branching.as_ref().is_some_and(|(p, m)| *p == pi && *m == name);
    let after = crate::prompt_edit::after_words(s, pi, &name);
    let branch_words = if after.starts_with("After") { format!("Branch: {after} ▾") } else { "Branch from turn ▾".to_string() };
    let key = (pi, name.clone());
    let branch = Button::new(branch_words)
        .variant(if open { ButtonVariant::Outline } else { ButtonVariant::Ghost })
        .help("Apply from now or a past turn")
        .on_press(move |s: &mut App| s.drawers.branching = if s.drawers.branching.as_ref() == Some(&key) { None } else { Some(key.clone()) })
        .build(k)
        .key(format!("inspect-branch-{name}"));
    let mut c = col().w_full().px(12.0).py(8.0).gap(6.0).border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_weak).key(format!("inspect-footer-{name}"));
    if open {
        c = c.child(col().w_full().max_h(220.0).scroll(ScrollSpec { stick_bottom: false }).key(format!("inspect-turns-{name}")).child(crate::prompt_edit::turn_list(s, pi, &name, k)));
    }
    // the engine's refusal of the parts sent, said where Apply was pressed
    let mine = e.parts.as_ref().is_some_and(|(p, m)| *p == pi && *m == name) || parts == 0;
    if let Some(err) = e.error.as_ref().filter(|_| e.of.is_none() && mine) {
        c = c.child(text(crate::sidebar::first_line(err)).font(k.ty.small).color(k.c.danger.fg).wrap().key(format!("inspect-error-{name}")));
    } else if let Some((_, _, said)) = e.sent.as_ref().filter(|(p, m, _)| *p == pi && *m == name && n == 0) {
        c = c.child(text(said.clone()).font(k.ty.small).color(k.c.text_faint).wrap().key(format!("inspect-sent-{name}")));
    }
    if let Some(said) = s.streams.said.get(&(pi, name.clone())) {
        c = c.child(text(crate::accounts::plain(said)).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    // before its first turn nothing goes on from a turn, and nothing staged has a turn end to take it at
    if no_turn(s, pi, a) {
        let none = text("No turn yet.").font(k.ty.small).color(k.c.text_faint).single_line().key(format!("inspect-no-turn-{name}"));
        return c.child(row().w_full().gap(6.0).center_y().child(col().grow(1.0).shrink().min_w(0.0).child(staged_el)).child(none).child(reset));
    }
    let turned = s.projects.get(pi).and_then(|p| p.convs.get(&name)).is_none_or(|v| !v.loaded || v.start > 0 || !crate::prompt_edit::turns(&v.entries).is_empty());
    c.child(row().w_full().gap(6.0).center_y().child(col().grow(1.0).shrink().min_w(0.0).child(staged_el)).child(reset).children(turned.then_some(branch)).child(apply))
}

/// Whether agent A of project PI has had no turn: no tokens spent, and its conversation, where read, empty.
fn no_turn(s: &App, pi: usize, a: &AgentView) -> bool {
    a.usage.is_empty() && s.projects.get(pi).and_then(|p| p.convs.get(&a.name)).is_none_or(|v| v.start == 0 && v.entries.is_empty())
}

/// Drops everything staged for agent NAME in project PI.
fn reset(s: &mut App, pi: usize, name: &str) {
    crate::prompt_edit::take_up(s, pi, name);
    crate::prompt_edit::drop_parts(s);
    s.prompt_edit.after = None;
    s.streams.staged.remove(&(pi, name.to_string()));
    s.drawers.switches.remove(&(pi, name.to_string()));
    s.drawers.branching = None;
}

/// Sends what is staged for agent NAME in project PI, then lets go the slow-mode halt that waits on it.
fn apply(s: &mut App, pi: usize, name: &str) {
    s.streams.said.remove(&(pi, name.to_string()));
    crate::settings::apply(s, pi, name);
    if let Some(w) = s.drawers.switches.remove(&(pi, name.to_string())) {
        let req = coord_types::Request::HarnessSwitches { name: name.to_string(), safety_check: None, tools_off: w.tools_off, parts_off: None };
        s.request_in(pi, req, crate::Pending::Effort { agent: name.to_string() });
    }
    let parts = crate::prompt_edit::draft_of(s, pi, name).is_some_and(|d| !d.is_empty());
    let made = s.prompt_edit.made.borrow().clone().filter(|m| m.pi == pi && m.name == name);
    if let Some((text, replace)) = made.and_then(|m| m.version).filter(|_| parts) {
        crate::prompt_edit::take_up(s, pi, name);
        crate::prompt_edit::send_parts(s, pi, name, text, replace);
    }
    if let Some((id, n)) = s.projects.get(pi).and_then(|p| crate::slow::waiting_on(p, name)) {
        crate::slow::approve_in(s, pi, &id, n);
    }
    s.drawers.branching = None;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::prompt_parts::Pick;
    use crate::view_cost::{click as press, words_at as words};
    use rune::runtime::Runtime;

    /// Claude Code's own main text, made up as 2.1.331 writes it, the standing text appended at its end.
    const STANDING: &str = "Work in your own worktree. End each commit with an Item line.";

    /// A project with agent orch, its Inspect drawer open on TAB: its system prompt as coordd gives it (Claude Code's
    /// own, the standing text appended), two requests in its raw log, its conversation's turns, its context. All made up.
    pub(crate) fn orch(tab: Tab) -> (std::sync::MutexGuard<'static, ()>, App) {
        use docket_machine::graph_slice::Row;
        let (held, mut s) = crate::view_cost::test_app();
        s.notice = None;
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        p.live.agents = vec![serde_json::from_value(serde_json::json!({"id": "AGT-1", "name": "orch", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5:high", "state": "idle",
            "since": "2026-11-24T10:00:00Z", "last_active": "2026-11-24T10:02:00Z", "tools": [], "items": [], "context": {"used": 243000, "window": 350000}})).unwrap()];
        let entry = |offset: u64, m: &str, kind: &str, text: &str| coord_types::Entry { offset, at: format!("2026-11-24T{m}:00Z"), kind: kind.into(), tool: None, text: text.into(), patch: None, images: Vec::new() };
        let c = p.convs.entry("orch".into()).or_default();
        c.entries = vec![
            entry(0, "09:00", "user", "Read the parser and say where it loses positions"),
            entry(400, "09:04", "assistant", "It drops them in the lexer's error path."),
            entry(900, "09:30", "user", "Fix the lexer's error path and add a test"),
            entry(1300, "09:31", "tool", "cargo test -p parser"),
            entry(1700, "09:33", "result", "test result: ok. 41 passed"),
            entry(2100, "09:34", "assistant", "Fixed, with a test for each error kind."),
            entry(2600, "10:00", "user", "Now write the docs for it"),
        ];
        (c.loaded, c.session, c.next) = (true, "/made-up/s1.jsonl".into(), 3000);
        c.keys = (0..c.entries.len()).map(|i| format!("e{i}")).collect();
        let started = vec![Row { text: "the standing text, appended: carried in 40 requests, 12 turns, 3 steps".into(), of: "text:st".into(), ..Default::default() }];
        let own = vec![Row { text: "block 1: 1 version".into(), of: "text:c1".into(), rows: vec![Row { text: "version 1: carried in 40 requests, 12 turns, 3 steps".into(), of: "text:c1".into(), ..Default::default() }], ..Default::default() }];
        let rows = vec![Row { text: "started with: 1 version".into(), rows: started, ..Default::default() }, Row { text: "Claude Code's own: 1 block, 1 version".into(), rows: own, ..Default::default() }];
        s.folders.requests.insert((0, "orch".into(), crate::sidebar::PROMPT.into()), (String::new(), Ok(rows)));
        let main = format!(
            "You are an agent working with the user toward their goals, using your own judgment along the way.\n\nWhen you use a pronoun for someone and their pronouns have not been stated, use they/them.\n\n# Environment\n - Platform: linux\n - Shell: bash\n\n{STANDING}"
        );
        s.folders.texts.insert((0, "text:c1".into()), Ok(main));
        s.folders.texts.insert((0, "text:st".into()), Ok(STANDING.into()));
        s.projects = vec![p];
        s.cur = 0;
        s.sel = crate::Sel::Agent("orch".into());
        s.drawers.open.insert((0, "orch".into()), crate::drawer::Drawer::Inspect);
        s.drawers.tabs.insert((0, "orch".into()), tab);
        let tools = ["[tools] 2", "[tool Bash] {\"description\": \"Run a shell command and return its output.\\nMore lines.\"}", "[tool Read] {\"description\": \"Read a file from the local filesystem.\"}"];
        let mut lines = vec!["══ request 2026-11-24 10:00:01.000 · claude-opus-5-5 · status 200 ══", "[system]\nYou are Claude Code, Anthropic's official CLI for Claude. (made up)"];
        lines.extend(tools);
        lines.extend(["[messages] 1", "[0 user text]\nFix the lexer's error path and add a test", "[params] {\"max_tokens\": 32000}", "[reply tool_use]\n{\"name\": \"Bash\", \"input\": {\"command\": \"cargo test -p parser\"}}"]);
        lines.extend(["══ request 2026-11-24 10:00:09.000 · claude-opus-5-5 · status 200 ══", "[system]\nYou are Claude Code, Anthropic's official CLI for Claude. (made up)"]);
        lines.extend(tools);
        lines.extend([
            "[messages] 3",
            "[0 user text]\nFix the lexer's error path and add a test",
            "[1 assistant tool_use]\n{\"name\": \"Bash\", \"input\": {\"command\": \"cargo test -p parser\"}}",
            "[2 user tool_result]\ntest result: ok. 41 passed; 0 failed",
            "[params] {\"max_tokens\": 32000}",
            "[reply text]\nFixed, with a test for each error kind.",
        ]);
        crate::raw::hold(&mut s, 0, "orch", &lines, vec![1_000, 9_000]);
        (held, s)
    }

    /// The key of the element on RT's page that starts with PREFIX and ends with SUFFIX.
    fn keyed(rt: &Runtime<App>, prefix: &str, suffix: &str) -> String {
        rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with(prefix) && k.ends_with(suffix)).unwrap_or_else(|| panic!("{prefix}…{suffix} drawn"))
    }

    /// The drawer opens on Diff: the newest request alone, what it repeats of the one before folded, its new tokens
    /// estimated. On System a part switched Off and another edited by a click on its text are staged, counted in the
    /// footer with their change in tokens; a part's Reset drops it; Apply says the engine's refusal (here: no pipe) in
    /// the footer; Reset drops all of it. Tools lists the newest request's tools, Messages its messages.
    #[test]
    fn the_inspect_drawer_stages_his_changes_and_says_what_comes_of_them() {
        let (_held, s) = orch(Tab::Diff);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert!(rt.find("inspect-tabs-orch").is_some() && rt.find("inspect-newest-0-orch").is_some(), "Diff first");
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        for fold in ["▸ system prompt, as before", "▸ 2 tools, as before", "▸ 1 message as in the request before"] {
            assert!(texts.iter().any(|t| t == fold), "{fold} folded: {texts:?}");
        }
        assert!(!texts.iter().any(|t| t.contains("Run a shell command")), "the tools it repeats are not shown");
        let new = words(&rt, "inspect-diff-tokens-orch").expect("its tokens");
        assert!(new.starts_with("≈ ") && new.contains(" new tokens of ≈ "), "{new}");
        assert_eq!(words(&rt, "inspect-staged-orch").as_deref(), Some("Nothing staged"));

        press(&mut rt, "inspect-tab-2-orch");
        assert_eq!(tab(&rt.state, 0, "orch"), Tab::System);
        crate::sidebar::open_part(&mut rt.state, 0, "orch", "Pronouns");
        crate::sidebar::open_part(&mut rt.state, 0, "orch", "Environment");
        rt.invalidate();
        rt.frame(0.016);
        rt.frame(0.016);
        assert!(!rt.elements().into_iter().filter_map(|e| e.key).any(|k| k.starts_with("part-pick-")), "no Stock · Modified · Custom · Off");
        let on = keyed(&rt, "part-on-", "|Pronouns");
        press(&mut rt, &on);
        assert_eq!(rt.state.prompt_edit.picks.get("Pronouns").map(|p| p.0), Some(Pick::Off));
        let staged = words(&rt, "inspect-staged-orch").unwrap();
        assert!(staged.starts_with("1 change staged · ≈ −"), "{staged}");
        let edit = keyed(&rt, "part-edit-", "|Environment");
        press(&mut rt, &edit);
        assert_eq!(rt.state.prompt_edit.picks.get("Environment").map(|p| (p.0, p.1.text.starts_with("# Environment"))), Some((Pick::Modified, true)));
        let modified = keyed(&rt, "part-modified-", "|Environment");
        assert_eq!(words(&rt, &modified).as_deref(), Some("Modified"));
        assert!(words(&rt, "inspect-staged-orch").unwrap().starts_with("2 changes staged"));
        let reset = keyed(&rt, "part-reset-", "|Environment");
        press(&mut rt, &reset);
        assert!(!rt.state.prompt_edit.picks.contains_key("Environment") && words(&rt, "inspect-staged-orch").unwrap().starts_with("1 change staged"));

        press(&mut rt, "inspect-apply-orch");
        let err = words(&rt, "inspect-error-orch").expect("the refusal in the footer");
        assert!(err.starts_with("Disconnected"), "{err}");
        press(&mut rt, "inspect-reset-orch");
        assert!(crate::prompt_edit::draft_of(&rt.state, 0, "orch").is_none_or(|d| d.is_empty()));
        assert_eq!(words(&rt, "inspect-staged-orch").as_deref(), Some("Nothing staged"));

        press(&mut rt, "inspect-tab-3-orch");
        assert!(rt.find("inspect-tool-0").is_some() && rt.find("inspect-tool-1").is_some() && rt.find("inspect-tool-2").is_none(), "its two tools");
        press(&mut rt, "inspect-tab-5-orch");
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        assert!(texts.iter().any(|t| t.contains("41 passed")) && !texts.iter().any(|t| t.starts_with("▸ ")), "its messages whole");
    }

    /// Orch on TAB with a harness that takes tools off and its safety check, WebSearch off for its project.
    fn switchable(tab: Tab) -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = orch(tab);
        let a = &mut s.projects[0].live.agents[0];
        (a.capabilities.tool_disabling, a.capabilities.safety_toggle) = (true, true);
        a.switches = serde_json::from_value(serde_json::json!({"safety_check": true, "tools_off": ["WebSearch"], "from": {"tools_off": "project"}})).unwrap();
        (held, s)
    }

    /// A tool off is listed; each switch stages the whole list, back at what it has unstages; Apply sends one
    /// HarnessSwitches; Policy has no safety check of its own beside the Permissions row; with a harness that takes
    /// none, no switch stages anything.
    #[test]
    #[cfg(not(windows))]
    fn tools_stage_harness_switches_and_apply_sends_them_and_policy_has_one_permissions_control() {
        let (_held, mut s) = switchable(Tab::Tools);
        let (child, file) = crate::map::deep::tests::piped(&mut s, 0, "inspect-switches");
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert!(words(&rt, "inspect-tools-note").unwrap().contains("set for its project"), "where they were set");
        assert!(rt.find("inspect-tool-2").is_some() && rt.find("inspect-tool-3").is_none(), "Bash, Read, and WebSearch off");
        let staged = |rt: &Runtime<App>| rt.state.drawers.switches.get(&(0, "orch".into())).cloned();
        press(&mut rt, "inspect-tool-switch-0");
        assert_eq!(staged(&rt).and_then(|w| w.tools_off), Some(vec!["WebSearch".to_string(), "Bash".into()]), "the whole list");
        assert!(words(&rt, "inspect-staged-orch").unwrap().starts_with("1 change staged"));
        press(&mut rt, "inspect-tool-switch-0");
        assert_eq!(staged(&rt), None, "back as it is: nothing staged");
        press(&mut rt, "inspect-tool-switch-2");
        press(&mut rt, "inspect-tab-4-orch");
        assert!(rt.find("inspect-safety-row-orch").is_none(), "no second safety check");
        assert!(words(&rt, "inspect-staged-orch").unwrap().starts_with("1 change staged"));
        press(&mut rt, "inspect-apply-orch");
        assert_eq!(staged(&rt), None, "sent");
        rt.state.projects[0].link = None;
        let asked: Vec<serde_json::Value> = crate::map::deep::tests::sent(child, &file).into_iter().map(|v| v["req"].clone()).filter(|r| r["op"] == "harnessswitches").collect();
        assert_eq!(asked, [serde_json::json!({"op": "harnessswitches", "name": "orch", "tools_off": []})]);
        rt.update(|s, _| s.projects[0].live.agents[0].capabilities = Default::default());
        rt.invalidate();
        rt.frame(0.016);
        press(&mut rt, "inspect-tab-3-orch");
        press(&mut rt, "inspect-tool-switch-0");
        assert_eq!(staged(&rt), None, "a harness that takes none stages nothing");
    }

    /// Branch from turn opens the turns it can go on from; one picked names the button.
    #[test]
    fn branch_from_turn_picks_where_it_goes_on_from() {
        let (_held, s) = orch(Tab::System);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        press(&mut rt, "inspect-branch-orch");
        assert!(rt.find("inspect-turns-orch").is_some() && rt.find("prompt-edit-turn-2600").is_some());
        press(&mut rt, "prompt-edit-turn-2600");
        assert_eq!(rt.state.prompt_edit.after, Some(("/made-up/s1.jsonl".into(), 2600)));
        assert!(crate::prompt_edit::after_words(&rt.state, 0, "orch").starts_with("After "));
    }

    /// An agent with no turn yet (N-1890.p1, P1 #11): its footer has no Branch from turn and no Apply, and says why;
    /// one with turns has both. The Inspect toggle's tip shows while it is shut, and once pressed open, no tip covers
    /// the tabs it opened.
    #[test]
    fn a_fresh_agent_has_no_branch_or_apply_and_the_open_toggle_no_tip() {
        let tip = "What its model is sent";
        let (_held, mut s) = orch(Tab::Diff);
        s.projects[0].convs.remove("orch");
        s.drawers.open.remove(&(0, "orch".to_string()));
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        let (r, _) = rt.presented("drawer-inspect-orch").expect("the Inspect toggle");
        let shows = |rt: &mut Runtime<App>| {
            rt.pointer_move(1.0, 1.0);
            rt.frame(0.016);
            rt.pointer_move(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.frame(0.016);
            let wait = rt.help_delay + 0.1;
            rt.frame(wait);
            rt.frame(0.3);
            rt.help_tooltip().is_some_and(|(t, _)| t == tip)
        };
        assert!(shows(&mut rt), "its tip while shut");
        press(&mut rt, "drawer-inspect-orch");
        assert!(!shows(&mut rt), "open: no tip over its tabs");
        assert!(rt.find("inspect-branch-orch").is_none() && rt.find("inspect-apply-orch").is_none(), "no turn: nothing to branch from or apply");
        assert_eq!(words(&rt, "inspect-no-turn-orch").as_deref(), Some("No turn yet."));
        // the test lock let go before the next window takes it
        drop((rt, _held));
        let (_held, s) = orch(Tab::Diff);
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert!(rt.find("inspect-branch-orch").is_some() && rt.find("inspect-apply-orch").is_some(), "its turns: both");
    }

    /// A change in tokens is said with its sign.
    #[test]
    fn a_change_in_tokens_has_its_sign() {
        assert_eq!(signed(0), "±0");
        assert_eq!(signed(12), "+12");
        assert_eq!(signed(-300), "−300");
        assert_eq!(est(0), 0);
        assert_eq!(est(9), 3);
    }

    pub(crate) fn shoot(name: &str, head: fn() -> App) {
        shoot_at(name, (1400.0, 900.0), head);
    }

    /// The still NAME of the window HEAD makes, at SIZE.
    pub(crate) fn shoot_at(name: &str, size: (f32, f32), head: fn() -> App) {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        let snap = out.join(name).display().to_string();
        let wh = format!("{}x{}", size.0, size.1);
        let a: [&str; 6] = ["--snap", &snap, "--size", &wh, "--at", "1"];
        let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", size, head, crate::view) };
        rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
    }

    /// System with Pronouns off and Environment his own, shown beside Claude Code's (Diff), staged in the footer.
    fn system() -> App {
        let mut s = orch(Tab::System).1;
        crate::prompt_edit::pick_part(&mut s, 0, "orch", "Pronouns", Pick::Off, "", Pick::Stock);
        crate::prompt_edit::pick_part(&mut s, 0, "orch", "Environment", Pick::Modified, "# Environment\n - Platform: linux\n - Shell: bash\n - Worktree: ~/src/walk, its own branch", Pick::Stock);
        s.prompt_edit.diff.insert((0, "orch".into(), "Environment".into()));
        for part in ["Pronouns", "Environment"] {
            crate::sidebar::open_part(&mut s, 0, "orch", part);
        }
        s
    }

    /// The stills, all made up: `STILL_OUT=DIR yaaihh-tests inspect::tests::NAME --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn diff_still() {
        shoot("inspect-diff.png", || orch(Tab::Diff).1);
    }

    /// Orch before its first turn: no Branch from turn, no Apply (inspect-fresh.png).
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn fresh_still() {
        shoot("inspect-fresh.png", || {
            let (held, mut s) = orch(Tab::Diff);
            drop(held);
            s.projects[0].convs.remove("orch");
            s
        });
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn system_still() {
        shoot("inspect-system.png", system);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn branch_still() {
        shoot("inspect-branch.png", || {
            let mut s = system();
            s.drawers.branching = Some((0, "orch".into()));
            s.prompt_edit.after = Some(("/made-up/s1.jsonl".into(), 2600));
            s
        });
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn tools_still() {
        shoot("inspect-tools.png", || {
            let mut s = switchable(Tab::Tools).1;
            s.drawers.switches.insert((0, "orch".into()), Switches { tools_off: Some(vec!["WebSearch".into(), "Bash".into()]) });
            s
        });
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn policy_still() {
        shoot("inspect-policy.png", || switchable(Tab::Policy).1);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn messages_still() {
        shoot("inspect-messages.png", || orch(Tab::Messages).1);
    }

    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn raw_still() {
        shoot("inspect-raw.png", || orch(Tab::Raw).1);
    }
}
