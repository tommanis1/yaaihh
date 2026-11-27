//! The session's settings, behind one click in the chat's header (paper-research N-101 p1259 point 5, p1434; the
//! owner: "show and have settable the context size nad change model ... model effort swithc and auto ocmpact"): the
//! model it runs on and its effort, picked and then sent together by one Apply that says beforehand when they take
//! effect (the owner, N-541 post 32: "the model option to choose efrto etc with a save changes and apply button";
//! coordd's `model` and `effort`, each a restart on its own session once its turn has ended), Restart, and the account
//! it runs on, and Safe, Guarded or Yolo (line 25, N-1400). Its context is no part of this (N-804.p16): the circle beside the button in the header is its control,
//! and a click on it opens the context (circle.rs; the owner, N-541 post 32).

use crate::permission::{has_check, ALL, Permission};
use crate::{App, Pending};
use coord_types::{AgentView, Request};
use rune::{El, col, row, text};
use rune_kit::{Kit, SegmentedControl};

/// The Claude models an agent can be moved to (coordd refuses `gpt-*`: a Codex agent is a new agent), with their names.
const MODELS: &[(&str, &str)] = &[("claude-fable-5-2", "Fable 5.2"), ("claude-opus-5-5", "Opus 5.5"), ("claude-sonnet-5-5", "Sonnet 5.5"), ("claude-haiku-5-5", "Haiku 5.5")];

fn key(pi: usize, name: &str) -> String {
    format!("session|{pi}|{name}")
}

/// Whether A's session runs in Yolo, its permission checks skipped (`AgentView.mode`, line 25).
fn yolo(a: &AgentView) -> bool {
    a.mode.as_deref() == Some("yolo")
}

/// Whether A comes back in Yolo: the mode he asked for while its session does not run it yet (`mode_due`), else the
/// one it runs in.
fn asked_yolo(a: &AgentView) -> bool {
    a.mode_due.as_deref().map_or(yolo(a), |m| m == "yolo")
}

/// The permissions A comes back in (YOLO its mode), its harness's check as it is set (none: agy's, `has_check`).
fn permission(a: &AgentView, yolo: bool) -> Permission {
    Permission::of(yolo, a.switches.safety_check && has_check(&a.kind))
}

/// The permissions A's session runs in: its mode and the check it was started with, where coordd says it.
fn runs(a: &AgentView) -> Permission {
    Permission::of(yolo(a), a.safety_check_runs.unwrap_or(a.switches.safety_check) && has_check(&a.kind))
}

/// The choices A's Permissions row offers in project PI: those its harness can be put in, from what it comes back in.
fn choices(s: &App, pi: usize, a: &AgentView) -> Vec<Permission> {
    let takes_check = a.capabilities.safety_toggle && s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CONTROL_PROTOCOL);
    ALL.into_iter().filter(|c| c.reachable(true, takes_check, asked_yolo(a), a.switches.safety_check && has_check(&a.kind))).collect()
}

/// The line under the Permissions row while the mode he asked for is not the one A's session runs in (a restart not
/// yet done, or one that failed: consult-3, N-1400 p5).
fn due_words(a: &AgentView) -> Option<&'static str> {
    let words = |p| match p {
        Permission::Safe => "Safe from its next start",
        Permission::Guarded => "Guarded from its next start",
        Permission::Yolo => "Yolo from its next start",
    };
    let asked = permission(a, asked_yolo(a));
    (asked != runs(a)).then(|| words(asked))
}

/// What to ask again once a restart into what he asked for is over and did not take (consult-3, N-1400 p5; asking
/// again tries again, docket-core N-1400 p8): the mode, or a restart when only the check is due. The Permissions row's
/// picked segment cannot be pressed twice, so the due line offers `Try again`. None while the restart runs, while A is
/// not idle (in a turn the restart waits for its end), and while his last ask (ASKING) still waits for coordd's reply.
fn retry(a: &AgentView, asking: bool) -> Option<Request> {
    let name = a.name.clone();
    (due_words(a).is_some() && !a.restarting && a.state == "idle" && !asking).then(|| match (asked_yolo(a), yolo(a)) {
        (asked, runs) if asked != runs => Request::Mode { name, mode: (if asked { "yolo" } else { "safe" }).into() },
        _ => Request::Restart { name },
    })
}

/// The agent's ⋯ item's words: `Session`, and `Session · Guarded` or `Session · Yolo` while A's session runs without
/// asking, so it is seen without opening the panel (line 25); what he asked for and it does not run yet does not change it.
pub(crate) fn button_words(a: &AgentView) -> &'static str {
    match runs(a) {
        Permission::Safe => "Session",
        Permission::Guarded => "Session · Guarded",
        Permission::Yolo => "Session · Yolo",
    }
}

/// Whether an agent of KIND has a mode on a coordd of PROTOCOL: a Claude Code agent's from `MODE_PROTOCOL`, an agy
/// agent's from `AGY_MODE_PROTOCOL` (N-1956); no other harness has it.
pub(crate) fn takes_mode(kind: &str, protocol: u32) -> bool {
    match kind {
        "claude" => protocol >= coord_types::MODE_PROTOCOL,
        "agy" => protocol >= coord_types::AGY_MODE_PROTOCOL,
        _ => false,
    }
}

/// Whether the panel offers its Permissions for A in project PI ([`takes_mode`]): Safe, Guarded and Yolo for a Claude
/// Code agent; Safe and Yolo for an agy agent, whose harness has no check of its own ([`choices`]).
fn offers_mode(s: &App, pi: usize, a: &AgentView) -> bool {
    s.projects.get(pi).is_some_and(|p| takes_mode(&a.kind, p.protocol))
}

/// What his message box to A says in its place while A's session restarts (`AgentView.restarting`: a mode, model,
/// effort or account change), naming Guarded or Yolo when that is what it comes back in; none while it does not, a restart that
/// failed included. Line 25, the owner: "the input for the user is blocked claude code is changed then the wrapper
/// releases again for edits".
pub(crate) fn restarting(a: &AgentView) -> Option<&'static str> {
    a.restarting.then(|| match permission(a, asked_yolo(a)) {
        Permission::Safe => "restarting…",
        Permission::Guarded => "restarting with Guarded…",
        Permission::Yolo => "restarting with Yolo…",
    })
}

/// Open the settings of agent NAME in project PI, as its ⋯ Session does (a press on what he can do about its
/// session taking no messages, agents.rs `deaf_act`); open already, they stay so.
pub fn open(s: &mut App, pi: usize, name: &str) {
    s.streams.open.insert(key(pi, name));
}

/// Whether the settings of agent NAME in project PI are open; [`toggle`] opens or folds them.
pub(crate) fn shown(s: &App, pi: usize, name: &str) -> bool {
    s.streams.open.contains(&key(pi, name))
}

pub(crate) fn toggle(s: &mut App, pi: usize, name: &str) {
    if !s.streams.open.remove(&key(pi, name)) {
        open(s, pi, name);
    }
}

/// The model and effort of A's record (`MODEL:EFFORT`).
fn split(model: &str) -> (&str, Option<&str>) {
    match model.split_once(':') {
        Some((m, e)) => (m, Some(e)),
        None => (model, None),
    }
}

/// The settings of agent A in project PI, while opened.
pub fn panel(s: &App, pi: usize, a: &AgentView, k: &Kit) -> Option<El> {
    if !s.streams.open.contains(&key(pi, &a.name)) {
        return None;
    }
    Some(body(s, pi, a, false, k))
}

/// The model and effort agent NAME of project PI has staged ([`panel`]'s picks): how many.
pub(crate) fn staged(s: &App, pi: usize, name: &str) -> usize {
    s.streams.staged.get(&(pi, name.to_string())).map_or(0, |(m, e)| usize::from(m.is_some()) + usize::from(e.is_some()))
}

/// The model and effort staged for agent NAME of project PI sent, as Apply sends them.
pub(crate) fn apply(s: &mut App, pi: usize, name: &str) {
    let Some((now, claude)) = s.projects.get(pi).and_then(|p| p.live.agents.iter().find(|a| a.name == name)).map(|a| (a.model.clone(), a.kind == "claude")) else { return };
    let Some((model, effort)) = s.streams.staged.remove(&(pi, name.to_string())) else { return };
    s.streams.said.remove(&(pi, name.to_string()));
    let one = claude && s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::MODEL_EFFORT_PROTOCOL);
    for req in applied(name, &now, model, effort, one) {
        s.request_in(pi, req, Pending::Effort { agent: name.to_string() });
    }
}

/// What the settings of agent A in project PI hold: its model and effort, Restart, its account, Safe, Guarded or Yolo, coordd's
/// word on a change, and what it is. FOOTER: in the Inspect drawer (inspect.rs), whose footer applies the picks, so no
/// Apply here.
pub(crate) fn body(s: &App, pi: usize, a: &AgentView, footer: bool, k: &Kit) -> El {
    let (model, effort) = split(&a.model);
    let (staged_model, staged_effort) = s.streams.staged.get(&(pi, a.name.clone())).map_or((None, None), |(m, e)| (m.as_deref(), e.as_deref()));
    let label = |t: &str| text(t.to_string()).font(k.ty.small).color(k.c.text_muted).w(96.0);
    let value = |t: String| text(t).font(k.ty.small).color(k.c.text_base);
    let mut c = col().w_full().px(24.0).pb(10.0).gap(8.0);
    let mut m = row().gap(8.0).center_y().child(label("Model"));
    let at = MODELS.iter().position(|x| x.0 == model);
    if a.kind == "claude" {
        if at.is_none() {
            m = m.child(value(model.to_string()));
        }
        let names: Vec<&str> = MODELS.iter().map(|x| x.1).collect();
        let picked = staged_model.and_then(|x| MODELS.iter().position(|m| m.0 == x)).or(at);
        let (name, now) = (a.name.clone(), model.to_string());
        m = m.child(
            SegmentedControl::new(&names, picked)
                .label("Model")
                .fit()
                .on_pick(move |s: &mut App, i, _| {
                    let pick = MODELS[i].0.to_string();
                    let st = s.streams.staged.entry((pi, name.clone())).or_default();
                    st.0 = (pick != now).then_some(pick);
                })
                .build(k),
        );
    } else {
        m = m.child(value(model.to_string()));
    }
    c = c.child(m);
    let mut e = row().gap(8.0).center_y().child(label("Effort"));
    if let Some(levels) = coord_types::harness::efforts(&a.kind) {
        let at = staged_effort.or(effort).and_then(|x| levels.iter().position(|l| *l == x));
        let (name, now) = (a.name.clone(), effort.map(str::to_string));
        e = e.child(
            SegmentedControl::new(levels, at)
                .label("Effort")
                .fit()
                .on_pick(move |s: &mut App, i, _| {
                    let pick = levels[i].to_string();
                    let st = s.streams.staged.entry((pi, name.clone())).or_default();
                    st.1 = (Some(&pick) != now.as_ref()).then_some(pick);
                })
                .build(k),
        );
    } else {
        e = e.child(value(effort.unwrap_or("its model's default").to_string()));
    }
    c = c.child(e);
    // what is picked goes in one Apply, which says beforehand when it takes effect; Discard puts the picks back. With
    // nothing picked, Restart: his own act, which roster refuses an agent below him (N-804.p9: a new compaction point
    // holds from a start)
    if staged_model.is_none() && staged_effort.is_none() && a.kind == "claude" && a.state != "stopped" {
        if s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::MODEL_EFFORT_PROTOCOL) {
            let name = a.name.clone();
            let restart = rune_kit::Button::new("Restart")
                .variant(rune_kit::ButtonVariant::Neutral)
                .on_press(move |s: &mut App| {
                    s.streams.said.remove(&(pi, name.clone()));
                    s.request_in(pi, Request::Restart { name: name.clone() }, Pending::Effort { agent: name.clone() });
                })
                .build(k);
            c = c.child(row().gap(8.0).center_y().child(label("")).child(restart).child(text(takes_effect(&a.kind, &a.state)).font(k.ty.small).color(k.c.text_muted)));
        }
    }
    if footer && (staged_model.is_some() || staged_effort.is_some()) {
        c = c.child(row().gap(8.0).center_y().child(label("")).child(text(format!("Staged: applied with the footer's Apply, {}", takes_effect(&a.kind, &a.state))).font(k.ty.small).color(k.c.text_muted).wrap()));
    } else if staged_model.is_some() || staged_effort.is_some() {
        let name = a.name.clone();
        let apply = rune_kit::Button::new("Apply").on_press(move |s: &mut App| apply(s, pi, &name)).build(k);
        let name = a.name.clone();
        let discard = rune_kit::Button::new("Discard")
            .variant(rune_kit::ButtonVariant::Ghost)
            .on_press(move |s: &mut App| drop(s.streams.staged.remove(&(pi, name.clone()))))
            .build(k);
        c = c.child(row().gap(8.0).center_y().child(label("")).child(apply).child(discard).child(text(takes_effect(&a.kind, &a.state)).font(k.ty.small).color(k.c.text_muted)));
    }
    // the account it runs on (N-588.p3: an agent's line names its account; N-591.p4: on or off his accounts here, not
    // `coord native` in a terminal); a gateway agent's says it goes through the model route
    if a.kind == "claude" && s.projects.get(pi).is_some_and(|p| p.protocol >= crate::accounts::ACCOUNTS) {
        let mut r = row().gap(8.0).center_y().child(label("Account"));
        let name = a.name.clone();
        r = r.child(
            // what he chose: `native` from ACCOUNTS_ALL (an agent on the gateway at a limit runs on no account, yet he
            // put it on his accounts, accounts-lead N-596.p13); before it, an account being there said so
            SegmentedControl::new(&["My accounts", "Gateway"], Some(if chosen_native(s, pi, a) { 0 } else { 1 }))
                .label("Account")
                .fit()
                .on_pick(move |s: &mut App, i, _| {
                    s.streams.said.remove(&(pi, name.clone()));
                    s.request_in(pi, Request::Native { name: name.clone(), on: i == 0 }, Pending::Effort { agent: name.clone() });
                })
                .build(k),
        );
        c = c.child(r);
        if let Some(n) = &a.account_note {
            c = c.child(row().gap(8.0).child(label("")).child(text(n.clone()).font(k.ty.small).color(k.c.text_muted).wrap()));
        }
    }
    // Safe, Guarded or Yolo (line 25, N-1400; the owner: "add a button say to the session connect we have now to allow yolo or
    // safety … if the setting is changed the underlying claude code is restarted with the new permission"): sent on
    // the press, as Account is; coordd restarts the session for it, and its reply below says when
    // one choice of three; a harness with no switch for the check offers Safe and the one its check gives
    if offers_mode(s, pi, a) {
        let takes_check = a.capabilities.safety_toggle && s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CONTROL_PROTOCOL);
        let (yolo, check) = (asked_yolo(a), a.switches.safety_check && has_check(&a.kind));
        let can = choices(s, pi, a);
        let now = permission(a, yolo);
        let labels: Vec<&str> = can.iter().map(|c| c.word()).collect();
        let name = a.name.clone();
        c = c.child(
            row().gap(8.0).center_y().child(label("Permissions")).child(
                SegmentedControl::new(&labels, can.iter().position(|c| *c == now))
                    .label("Permissions")
                    .fit()
                    .on_pick(move |s: &mut App, i, _| {
                        if can[i] != now {
                            crate::permission::send(s, pi, &name, can[i], (yolo, check), takes_check);
                        }
                    })
                    .build(k)
                    .help(crate::permission::HELP),
            ),
        );
        if let Some(w) = due_words(a) {
            let mut due = row().gap(8.0).center_y().child(label("")).child(text(w).font(k.ty.small).color(k.c.text_muted));
            let asking = s.projects.get(pi).is_some_and(|p| p.pending.values().any(|q| matches!(q, Pending::Effort { agent } if *agent == a.name)));
            if let Some(ask) = retry(a, asking) {
                let name = a.name.clone();
                let again = rune_kit::Button::new("Try again")
                    .variant(rune_kit::ButtonVariant::Ghost)
                    .on_press(move |s: &mut App| {
                        s.streams.said.remove(&(pi, name.clone()));
                        s.request_in(pi, ask.clone(), Pending::Effort { agent: name.clone() });
                    })
                    .build(k);
                due = due.child(again);
            }
            c = c.child(due);
        }
    }
    // coordd's word on the change: when it takes effect
    if let Some(said) = s.streams.said.get(&(pi, a.name.clone())) {
        c = c.child(row().gap(8.0).child(label("")).child(text(crate::accounts::plain(said)).font(k.ty.small).color(k.c.text_faint).wrap()));
    }
    // what it is (state, id, harness, model, since, items, memory, where it stands): out of first sight since the chat
    // page's rules (N-873), one click away here until the circle's panel carries them (consult-3, N-412 post 1158)
    let facts = crate::chat::facts_lines(s, &s.projects[pi], a);
    let mut f = col().gap(2.0);
    for l in facts.iter().filter(|l| !l.starts_with("on ")) {
        f = f.child(text(l.clone()).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    c = c.child(row().key(format!("session-facts-{}", a.name)).gap(8.0).child(label("It is")).child(f.grow(1.0).shrink().w(0.0)));
    c
}

/// The requests one Apply sends for agent NAME running NOW (`MODEL:EFFORT`) with MODEL and EFFORT picked: one `model`
/// carrying both where coordd takes it (ONE: MODEL_EFFORT_PROTOCOL), so it restarts once; else `model`, then `effort`.
fn applied(name: &str, now: &str, model: Option<String>, effort: Option<String>, one: bool) -> Vec<Request> {
    let name = name.to_string();
    let (running, _) = split(now);
    match (model, effort) {
        (m, Some(e)) if one => vec![Request::Model { name, model: format!("{}:{e}", m.as_deref().unwrap_or(running)) }],
        (m, e) => {
            let mut out: Vec<Request> = m.into_iter().map(|model| Request::Model { name: name.clone(), model }).collect();
            out.extend(e.map(|level| Request::Effort { name, level }));
            out
        }
    }
}

/// When a model or effort applied now takes effect, for a KIND of agent in STATE, as coordd's `model` and `effort` decide
/// it: only Claude Code restarts; the others take an effort with their next turn.
fn takes_effect(kind: &str, state: &str) -> &'static str {
    match state {
        "stopped" => "from its next start",
        _ if kind != "claude" => "from its next turn",
        "working" => "after its turn: it restarts",
        _ => "now: it restarts, tasks end",
    }
}

/// Whether the owner put A on his accounts: its `native` from the protocol that sends it, else whether it runs on one.
fn chosen_native(s: &App, pi: usize, a: &AgentView) -> bool {
    if s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::ACCOUNTS_ALL) { a.native } else { a.account.is_some() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row reads what he chose: from ACCOUNTS_ALL an agent he put on his accounts that runs through the gateway at
    /// a limit (no account now) still reads My accounts; before it, an account being there says so.
    #[test]
    fn the_account_row_reads_what_he_chose() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let pi = s.projects.len() - 1;
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "", "state": "", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        a.native = true;
        s.projects[pi].protocol = coord_types::ACCOUNTS_ALL;
        assert!(chosen_native(&s, pi, &a), "on the gateway at a limit, still his choice");
        s.projects[pi].protocol = coord_types::ACCOUNTS_ALL - 1;
        assert!(!chosen_native(&s, pi, &a), "before the protocol, no account reads Gateway");
        a.account = Some("work".into());
        assert!(chosen_native(&s, pi, &a));
    }

    /// Line 25: Safe and Yolo are offered for a Claude Code agent on a coordd that takes `Mode`; the header reads
    /// `Session · Yolo` while the agent runs in Yolo; his box gives way to `restarting…` while its session restarts,
    /// naming Yolo when it comes back in it. While the mode he asked for is not the one it runs in, the line under the
    /// row says so, with `Try again` once a restart is over and did not take. An older coordd sends neither field:
    /// nothing changes. The press sends `{"op": "mode", "name": …, "mode": "yolo"}`.
    #[test]
    fn yolo_is_offered_seen_on_the_header_and_a_restart_blocks_the_box() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let pi = s.projects.len() - 1;
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "", "state": "idle", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
        assert_eq!((a.mode.as_deref(), a.restarting), (None, false), "an older coordd's agent");
        s.projects[pi].protocol = coord_types::MODE_PROTOCOL - 1;
        assert!(!offers_mode(&s, pi, &a));
        s.projects[pi].protocol = coord_types::MODE_PROTOCOL;
        assert!(offers_mode(&s, pi, &a));
        a.kind = "agy".into();
        s.projects[pi].protocol = coord_types::AGY_MODE_PROTOCOL - 1;
        assert!(!offers_mode(&s, pi, &a), "an agy agent's only from a coordd that takes it");
        s.projects[pi].protocol = coord_types::AGY_MODE_PROTOCOL;
        assert!(offers_mode(&s, pi, &a));
        // agy has no check of its own: its Yolo is Yolo, whatever the check reads, and it offers Safe and Yolo
        (a.mode, a.switches.safety_check, a.safety_check_runs) = (Some("yolo".into()), true, Some(true));
        assert_eq!((button_words(&a), due_words(&a), choices(&s, pi, &a)), ("Session · Yolo", None, vec![Permission::Safe, Permission::Yolo]));
        a.kind = "claude".into();
        assert_eq!(button_words(&a), "Session · Guarded", "Claude Code's check kept");
        (a.mode, a.switches.safety_check, a.safety_check_runs) = (None, false, None);
        a.kind = "codex".into();
        assert!(!offers_mode(&s, pi, &a), "Claude Code's and agy's only");
        assert_eq!((button_words(&a), restarting(&a)), ("Session", None));
        a.mode = Some("yolo".into());
        assert_eq!((button_words(&a), restarting(&a)), ("Session · Yolo", None));
        a.restarting = true;
        assert_eq!(restarting(&a), Some("restarting with Yolo…"));
        a.mode = None;
        assert_eq!((button_words(&a), restarting(&a)), ("Session", Some("restarting…")));
        // Safe asked of a session in Yolo: until the restart lands the header reads Yolo, and the row says when
        a.mode = Some("yolo".into());
        a.mode_due = Some("safe".into());
        assert_eq!((button_words(&a), restarting(&a), due_words(&a)), ("Session · Yolo", Some("restarting…"), Some("Safe from its next start")));
        // the restart failed (coordd's `restarted` all the same): the box is free, the session still runs in Yolo and
        // reads so (consult-3, N-1400 p5)
        a.restarting = false;
        assert_eq!((button_words(&a), restarting(&a), due_words(&a)), ("Session · Yolo", None, Some("Safe from its next start")));
        // the picked segment cannot be pressed again: `Try again` asks once more for Safe, only while nothing runs
        // or waits (docket-core N-1400 p8: asking again tries again)
        assert_eq!((again(&a, false), again(&a, true)), (Some(mode("safe")), None), "his ask waiting for its reply: none");
        a.state = "working".into();
        assert_eq!(again(&a, false), None, "in a turn the restart waits for its end");
        a.state = "idle".into();
        // Yolo asked of a session in Safe, restarting into it
        (a.mode, a.mode_due, a.restarting) = (None, Some("yolo".into()), true);
        assert_eq!((button_words(&a), restarting(&a), due_words(&a)), ("Session", Some("restarting with Yolo…"), Some("Yolo from its next start")));
        assert_eq!(again(&a, false), None, "while the restart runs");
        a.restarting = false;
        assert_eq!(again(&a, false), Some(mode("yolo")));
        // it landed: one mode, nothing due
        (a.mode, a.mode_due, a.restarting) = (Some("yolo".into()), None, false);
        assert_eq!((button_words(&a), restarting(&a), due_words(&a), again(&a, false)), ("Session · Yolo", None, None, None));
        let sent = serde_json::to_value(Request::Mode { name: "a".into(), mode: "yolo".into() }).unwrap();
        assert_eq!(sent, serde_json::json!({"op": "mode", "name": "a", "mode": "yolo"}));
    }

    /// What `Try again` would send for A, as its line.
    fn again(a: &AgentView, asking: bool) -> Option<serde_json::Value> {
        retry(a, asking).map(|r| serde_json::to_value(r).unwrap())
    }

    fn mode(m: &str) -> serde_json::Value {
        serde_json::json!({"op": "mode", "name": "a", "mode": m})
    }

    /// Guarded and Yolo differ in the check alone: the header reads what the session was started with
    /// (`safety_check_runs`), not what it is set to, and the row says the other is due until a new session runs it;
    /// `Try again` restarts it, as no mode is due. An older coordd sends no `safety_check_runs`: the check as set.
    #[test]
    fn a_change_between_guarded_and_yolo_shows_once_a_session_runs_it() {
        let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "", "name": "a", "parent": "", "kind": "claude", "model": "", "state": "idle", "since": "", "last_tool": null, "tools": [], "items": [], "transcript": null, "mode": "yolo"})).unwrap();
        (a.switches.safety_check, a.safety_check_runs) = (true, Some(false));
        assert_eq!((button_words(&a), due_words(&a)), ("Session · Yolo", Some("Guarded from its next start")), "Guarded asked of a session in Yolo");
        assert_eq!(again(&a, false), Some(serde_json::json!({"op": "restart", "name": "a"})));
        a.restarting = true;
        assert_eq!((restarting(&a), again(&a, false)), (Some("restarting with Guarded…"), None));
        (a.restarting, a.safety_check_runs) = (false, Some(true));
        assert_eq!((button_words(&a), due_words(&a)), ("Session · Guarded", None), "the new session runs it");
        a.switches.safety_check = false;
        assert_eq!((button_words(&a), due_words(&a)), ("Session · Guarded", Some("Yolo from its next start")), "and back");
        a.safety_check_runs = None;
        assert_eq!((button_words(&a), due_words(&a)), ("Session · Yolo", None), "an older coordd");
    }

    /// The stills of line 25 (N-1400): `STILL_OUT=DIR cargo test yolo_stills -- --ignored`, each with its element tree.
    /// A made-up agent in Yolo: the Session panel opened (the Permissions row, the header's `Session · Yolo`); then its
    /// session restarting into Yolo, the message box given way to `restarting with Yolo…`; then Safe asked and its
    /// restart over without taking, the line under the row with `Try again`.
    #[test]
    #[ignore]
    fn yolo_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn init() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::MODE_PROTOCOL;
            let mut a: AgentView = serde_json::from_value(serde_json::json!({"id": "AGT-007", "name": "dev", "parent": "owner", "kind": "claude", "model": "claude-opus-5-5:high",
                "state": "idle", "since": "2026-11-20T07:30:00Z", "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap();
            a.mode = Some("yolo".into());
            p.live.agents = vec![a];
            s.projects = vec![p];
            s.cur = 0;
            s.sel = crate::Sel::Agent("dev".into());
            s.streams.open.insert("session|0|dev".into());
            s
        }
        fn back_in_yolo() -> App {
            let mut s = init();
            s.streams.open.clear();
            s.projects[0].live.agents[0].restarting = true;
            s
        }
        fn safe_not_taken() -> App {
            let mut s = init();
            s.projects[0].live.agents[0].mode_due = Some("safe".into());
            s
        }
        let shots: [(&str, fn() -> App); 3] = [("yolo-session", init), ("yolo-restarting", back_in_yolo), ("yolo-try-again", safe_not_taken)];
        for (name, start) in shots {
            let (png, tree) = (out.join(format!("{name}.png")), out.join(format!("{name}.json")));
            let args = vec!["--snap".to_string(), png.display().to_string(), "--tree".into(), tree.display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// Apply says beforehand when the picks take effect, as coordd will (N-541 post 32).
    #[test]
    fn apply_says_when_the_picks_take_effect() {
        assert_eq!(takes_effect("claude", "working"), "after its turn: it restarts");
        assert_eq!(takes_effect("agy", "stopped"), "from its next start");
        assert!(takes_effect("claude", "idle").starts_with("now"));
        assert_eq!(takes_effect("agy", "working"), "from its next turn");
    }

    /// One Apply is one `model` request carrying both where coordd takes it, so the agent restarts once (N-804.p7).
    #[test]
    fn apply_sends_model_and_effort_as_one_where_coordd_takes_it() {
        let one = applied("a", "m1:high", Some("m2".into()), Some("low".into()), true);
        assert!(matches!(&one[..], [Request::Model { model, .. }] if model == "m2:low"));
        let effort_only = applied("a", "m1:high", None, Some("low".into()), true);
        assert!(matches!(&effort_only[..], [Request::Model { model, .. }] if model == "m1:low"));
        let model_only = applied("a", "m1:high", Some("m2".into()), None, true);
        assert!(matches!(&model_only[..], [Request::Model { model, .. }] if model == "m2"));
        let agy = applied("a", "agy/g:low", None, Some("xhigh".into()), false);
        assert!(matches!(&agy[..], [Request::Effort { level, .. }] if level == "xhigh"), "an agy agent's effort is coord effort");
        let two = applied("a", "m1:high", Some("m2".into()), Some("low".into()), false);
        assert!(matches!(&two[..], [Request::Model { model, .. }, Request::Effort { level, .. }] if model == "m2" && level == "low"));
    }

    #[test]
    fn a_records_model_holds_its_effort_after_a_colon() {
        assert_eq!(split("claude-opus-5-5:xhigh"), ("claude-opus-5-5", Some("xhigh")));
        assert_eq!(split("claude-opus-5-5"), ("claude-opus-5-5", None));
    }
}
