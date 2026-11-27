//! The menu at a thing, drawn from the interpreter's answer (one-interpreter.md section 5; N-568 part 6). The owner is
//! bound by no act of the norms, so he drives in an agent's name: a secondary press on an agent's tile asks the coordd
//! for the acts its lead has that name it and for the agent's own (`Request::Enabled` with ACTOR), on a ticket's row
//! for the acts the agent who has the ticket has there. The menu lists each under "As NAME": enabled ones as they are, the others greyed with the clause that
//! failed. A greyed one is still his to do: it asks once, "Not allowed: WHY. Do it anyway?". Only from
//! ENABLED_PROTOCOL; below it no menu opens and the window is as before.
//!
//! The menu offers only what it can carry out (consult-4 and paper-research, N-587.p8): an act the window has a
//! request or a page for today (esc, set-slow, lift-slow, read-session). Every other act waits for coordd to take
//! acts as phrases (part 5), and is left out until then; nothing in it ends in "not sent". Only an act on the thing
//! pressed is offered: the coordd answers every instance the actor has, so the rest is filtered out here (N-587.p10).

use crate::App;
use coord_types::ActAnswer;
use rune::{El, PointerPress, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, MenuItem, context_menu};

/// One group of an open menu: ACTOR's acts at THING (none: the team); the acts once the coordd answered (or why not).
pub struct Group {
    pub actor: String,
    pub thing: Option<String>,
    pub got: Option<Result<Vec<ActAnswer>, Failed>>,
}

/// A failed ask: one plain sentence for the menu, and the coordd's own words behind a click.
#[derive(Clone, Debug, PartialEq)]
pub struct Failed {
    pub said: String,
    pub detail: String,
}

/// An open menu in project P, where the press was: a group per agent he may act as there.
pub struct Menu {
    pub p: usize,
    pub at: PointerPress,
    pub groups: Vec<Group>,
}

/// The owner's state for acts: the open menu, and a greyed act he chose, waiting for his "Do it anyway".
#[derive(Default)]
pub struct Acts {
    pub menu: Option<Menu>,
    pub anyway: Option<(usize, ActAnswer)>,
    /// A failed ask's detail he clicked, in a dialog.
    pub detail: Option<Failed>,
}

/// The asks at agent NAME's tile, whose lead is LEAD: as its lead, the acts that name it (when the lead is an agent);
/// as itself, its own (ee7bb20, N-568.p36).
pub fn at_agent(name: &str, lead: &str) -> Vec<(String, Option<String>)> {
    let mut asks = Vec::new();
    if !lead.is_empty() && lead != "owner" {
        asks.push((lead.to_string(), Some(name.to_string())));
    }
    asks.push((name.to_string(), None));
    asks
}

/// Open the menu in project P with a group per ask (ACTOR, THING), if its coordd answers what is enabled; each is asked
/// for now.
pub fn open(s: &mut App, p: usize, asks: Vec<(String, Option<String>)>, at: PointerPress) {
    if asks.is_empty() || !s.projects.get(p).is_some_and(|q| q.protocol >= coord_types::ENABLED_PROTOCOL) {
        return;
    }
    s.acts.menu = Some(Menu { p, at, groups: asks.iter().map(|(actor, thing)| Group { actor: actor.clone(), thing: thing.clone(), got: None }).collect() });
    for (actor, thing) in asks {
        s.request_in(p, coord_types::Request::Enabled { thing: thing.clone(), actor: Some(actor.clone()) }, crate::Pending::Acts { thing, actor });
    }
}

/// The coordd's answer for ACTOR at THING in project P: one act per line. A menu since closed or moved on takes nothing.
pub fn took(s: &mut App, p: usize, thing: Option<String>, actor: String, ok: bool, out: String) {
    let Some(m) = s.acts.menu.as_mut().filter(|m| m.p == p) else { return };
    let Some(g) = m.groups.iter_mut().find(|g| g.thing == thing && g.actor == actor) else { return };
    g.got = Some(if ok { Ok(lines(&out)) } else { Err(failed(&out)) });
}

/// The acts in a reply's lines; a line this build cannot read is left out.
fn lines(out: &str) -> Vec<ActAnswer> {
    out.lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// What the window does for an act it can carry out today: its request, or its page.
#[derive(Clone, Debug, PartialEq)]
pub enum Do {
    /// Stop agent NAME's turn (`Esc`, as the chat's Esc button).
    Esc(String),
    /// Slow over SCOPE (`project`, or a ticket's id) set or lifted (`Slow`, as the project's menu and a ticket's switch).
    Slow { t: String, on: bool },
    /// Agent NAME's session, on its page.
    Read(String),
}

/// What the window does for act A, when it can carry it out today; None for every other act (they wait for part 5).
pub fn carry(a: &ActAnswer) -> Option<Do> {
    let one = || a.objects.first().filter(|o| !o.is_empty() && a.objects.len() == 1).cloned();
    match a.act.as_str() {
        "esc" => one().map(Do::Esc),
        "set-slow" => one().map(|t| Do::Slow { t, on: true }),
        "lift-slow" => one().map(|t| Do::Slow { t, on: false }),
        "read-session" => one().map(Do::Read),
        _ => None,
    }
}

/// A thing as he reads it: a ticket by its id and its label, the project as itself, an agent by its name.
fn thing(p: Option<&crate::Proj>, x: &str) -> String {
    match p {
        Some(p) if x.starts_with(|c: char| c.is_ascii_uppercase()) && x.contains('-') => {
            let r = crate::refs::ticket_id(p, x);
            format!("{} · {}", r.id, r.label)
        }
        _ if x == "project" => "the whole project".to_string(),
        _ => x.to_string(),
    }
}

/// What D does, as the menu says it.
pub fn words(p: Option<&crate::Proj>, d: &Do) -> String {
    match d {
        Do::Esc(n) => format!("Esc {n}"),
        Do::Slow { t, on: true } => format!("Slow: {}", thing(p, t)),
        Do::Slow { t, on: false } => format!("Lift slow: {}", thing(p, t)),
        Do::Read(n) => format!("Read {n}'s session"),
    }
}

/// A greyed act's line under its entry. The clause that failed and where it is written are engine detail, shown
/// behind the click, in the "Do it anyway" dialog (consult-3 N-754, paper-research N-587.p8); the rule's own words
/// take this place when coordd sends them (`says`, N-747).
pub fn reason(_a: &ActAnswer) -> String {
    "not allowed now".to_string()
}

/// A failed ask as the window says it: one plain sentence, with no engine words and no `coord` command; the coordd's
/// line (without its `coord can:`) is the detail behind the click (paper-research, N-587.p12).
pub fn failed(out: &str) -> Failed {
    let out = out.trim();
    let rest = out.strip_prefix("coord can: ").unwrap_or(out);
    let said = if let Some(n) = rest.strip_prefix("no agent named ") {
        format!("{n} is not an agent here any more")
    } else if let Some(t) = rest.strip_prefix("no ticket, agent or team ").or_else(|| rest.strip_prefix("no ticket ")) {
        format!("{t} is not here any more")
    } else {
        "Actions unreadable".to_string()
    };
    Failed { said, detail: rest.to_string() }
}

/// Whether act A belongs in group G's menu: only an act whose object is the thing pressed, the ticket or the agent
/// (as itself: the agent), never every instance the actor has there; a thread's row lists no agents to stop, a lead's
/// tile not one Esc per agent below it (consult-4, N-587.p10).
fn names(g: &Group, a: &ActAnswer) -> bool {
    let at = g.thing.as_deref().unwrap_or(&g.actor);
    a.objects.iter().any(|o| o == at)
}

/// What group G offers of ACTS: the ones on the thing pressed that the window carries out, enabled ones first.
fn kept<'a>(g: &Group, acts: &'a [ActAnswer]) -> Vec<(&'a ActAnswer, Do)> {
    acts.iter().filter(|a| a.enabled).chain(acts.iter().filter(|a| !a.enabled)).filter(|a| names(g, a)).filter_map(|a| Some((a, carry(a)?))).collect()
}

/// The menu, a group per agent he acts as, enabled acts first in each, each kind in the coordd's order; only the acts
/// it can carry out, and a group with none left out.
pub fn menu(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(m) = &s.acts.menu else { return };
    let close = |s: &mut App| s.acts.menu = None;
    let key = format!("acts-{}-{}", m.p, m.groups.iter().map(|g| format!("{}:{}", g.actor, g.thing.as_deref().unwrap_or(""))).collect::<Vec<_>>().join(","));
    let mut menu = context_menu(key, rune::el(), Some(m.at)).on_close(close);
    let (p, proj) = (m.p, s.projects.get(m.p));
    let mut shown = 0;
    for g in &m.groups {
        let items: Vec<MenuItem> = match &g.got {
            None => vec![MenuItem::new("…").disabled(true)],
            Some(Err(f)) => {
                let f = f.clone();
                vec![MenuItem::new(f.said.clone()).on_select(move |s: &mut App| {
                    s.acts.menu = None;
                    s.acts.detail = Some(f.clone());
                })]
            }
            Some(Ok(acts)) => kept(g, acts)
                .into_iter()
                .map(|(a, d)| {
                    let mut item = MenuItem::new(words(proj, &d));
                    if !a.enabled {
                        // greyed, its reason under it; still his to choose
                        item = item.description(reason(a));
                    }
                    let a = a.clone();
                    item.on_select(move |s: &mut App| {
                        s.acts.menu = None;
                        if a.enabled {
                            send(s, p, &d);
                        } else {
                            s.acts.anyway = Some((p, a.clone()));
                        }
                    })
                })
                .collect(),
        };
        if items.is_empty() {
            continue;
        }
        if shown > 0 {
            menu = menu.separator();
        }
        shown += 1;
        menu = menu.group_label(format!("As {}", g.actor));
        for i in items {
            menu = menu.item(i);
        }
    }
    if shown == 0 {
        menu = menu.item(MenuItem::new("Nothing you can do here yet").disabled(true));
    }
    let menu = menu.build(k, layer);
    layer.push(menu);
}

/// A failed ask's detail, the coordd's own words, behind his click on its plain sentence.
pub fn detail(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some(f) = &s.acts.detail else { return };
    let close = Button::new("Close").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.acts.detail = None);
    let body = col().gap(12.0).child(text(f.detail.clone()).font(k.ty.small).color(k.c.text_muted).wrap()).child(row().justify_end().child(close.build(k)));
    Dialog::new("act-detail", f.said.clone()).open(true).fit().width(420.0).child(body).on_close(|s: &mut App| s.acts.detail = None).build(k, layer);
}

/// His "Do it anyway" for a greyed act: once, with the clause that failed and where it is written.
pub fn anyway(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let Some((p, a)) = &s.acts.anyway else { return };
    let Some(d) = carry(a) else { return };
    let p = *p;
    let why = a.why.clone().unwrap_or_else(|| "not allowed now".into());
    let title = words(s.projects.get(p), &d);
    let go = {
        let d = d.clone();
        Button::new("Do it anyway").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| {
            s.acts.anyway = None;
            send(s, p, &d);
        })
    };
    let no = Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.acts.anyway = None);
    let body = col().gap(12.0).child(text(format!("Not allowed: {why}")).font(k.ty.small).color(k.c.text_muted).wrap()).child(row().gap(8.0).justify_end().child(no.build(k)).child(go.build(k)));
    Dialog::new("act-anyway", title).open(true).fit().width(420.0).child(body).on_close(|s: &mut App| s.acts.anyway = None).build(k, layer);
}

/// Carry out D in project P: its request (its answer where that request's answer goes), or its page.
fn send(s: &mut App, p: usize, d: &Do) {
    match d {
        Do::Esc(n) => _ = s.request_in(p, coord_types::Request::Esc { to: vec![n.clone()], step: false }, crate::Pending::Sent),
        Do::Slow { t, on } => _ = s.request_in(p, coord_types::Request::Slow { t: t.clone(), on: *on }, crate::Pending::TicketDone { clear: crate::threads::Cleared::Quiet }),
        Do::Read(n) => {
            s.cur = p;
            s.select_agent(n);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only what the window carries out is offered, each in his words; a greyed one's reason is its clause without
    /// where it is written; a failed ask names no command (consult-4, N-587.p8).
    #[test]
    fn the_menu_offers_what_it_carries_out_in_his_words() {
        let act = |act: &str, objects: &[&str], why: Option<&str>| ActAnswer { act: act.into(), objects: objects.iter().map(|o| o.to_string()).collect(), enabled: why.is_none(), why: why.map(str::to_string), says: None };
        assert_eq!(carry(&act("esc", &["dev"], None)), Some(Do::Esc("dev".into())));
        assert_eq!(carry(&act("set-slow", &["project"], None)), Some(Do::Slow { t: "project".into(), on: true }));
        assert_eq!(carry(&act("lift-slow", &["N-12"], None)), Some(Do::Slow { t: "N-12".into(), on: false }));
        assert_eq!(carry(&act("read-session", &["sub"], None)), Some(Do::Read("sub".into())));
        for other in ["add-agent", "run-model", "notify", "edit", "post"] {
            assert_eq!(carry(&act(other, &["x"], None)), None, "{other} waits for part 5");
        }
        assert_eq!(words(None, &Do::Esc("dev".into())), "Esc dev");
        assert_eq!(words(None, &Do::Slow { t: "project".into(), on: true }), "Slow: the whole project");
        assert_eq!(words(None, &Do::Read("sub".into())), "Read sub's session");
        assert_eq!(reason(&act("esc", &["orch"], Some("`below(orch, dev)` fails (agent-base/policy/esc.eflint:12)"))), "not allowed now");
        assert_eq!(failed("coord can: no agent named old").said, "old is not an agent here any more");
        let f = failed("coord can: the norms could not be asked: clingo: x");
        assert_eq!(f.said, "Actions unreadable", "no engine words in the menu");
        assert_eq!(f.detail, "the norms could not be asked: clingo: x", "the detail behind the click");
        assert!(!failed("coord can: no ticket N-9").said.contains("coord"));
        assert!(!failed("unknown variant `enabled`").said.contains("variant"));
        let out = "{\"act\":\"a\",\"enabled\":true}\nnot json\n{\"act\":\"b\",\"enabled\":false,\"why\":\"w\"}\n";
        let got = lines(out);
        assert_eq!(got.iter().map(|a| (a.act.as_str(), a.enabled, a.why.as_deref())).collect::<Vec<_>>(), [("a", true, None), ("b", false, Some("w"))]);
        // an agent's tile: as its lead at it, then as itself; a lead's (under the owner) only as itself
        assert_eq!(at_agent("dev", "orch"), [("orch".to_string(), Some("dev".to_string())), ("dev".to_string(), None)]);
        assert_eq!(at_agent("orch", "owner"), [("orch".to_string(), None)]);
    }

    fn act(act: &str, objects: &[&str]) -> ActAnswer {
        ActAnswer { act: act.into(), objects: objects.iter().map(|o| o.to_string()).collect(), enabled: true, why: None, says: None }
    }

    /// The coordd answers every instance the actor has: of a lead's, one Esc per agent below it and a slow per thread.
    fn a_leads_acts() -> Vec<ActAnswer> {
        vec![act("esc", &["dev"]), act("esc", &["dev2"]), act("set-slow", &["N-12"]), act("set-slow", &["N-13"]), act("lift-slow", &["N-12"]), act("read-session", &["dev"])]
    }

    /// On a thread's row only that thread's own acts: no Esc of agents (consult-4 N-587.p10, paper-research p12).
    #[test]
    fn a_threads_row_shows_no_esc() {
        let g = Group { actor: "lead".into(), thing: Some("N-12".into()), got: None };
        let acts = a_leads_acts();
        let got: Vec<Do> = kept(&g, &acts).into_iter().map(|(_, d)| d).collect();
        assert_eq!(got, [Do::Slow { t: "N-12".into(), on: true }, Do::Slow { t: "N-12".into(), on: false }]);
    }

    /// At an agent's tile only the entries that name that agent: as its lead "Esc dev", "Read dev's session", and as
    /// itself nothing of the agents below it.
    #[test]
    fn a_leads_tile_shows_only_the_entries_naming_its_agent() {
        let acts = a_leads_acts();
        let groups: Vec<Group> = at_agent("dev", "lead").into_iter().map(|(actor, thing)| Group { actor, thing, got: None }).collect();
        let at_dev: Vec<Do> = kept(&groups[0], &acts).into_iter().map(|(_, d)| d).collect();
        assert_eq!(at_dev, [Do::Esc("dev".into()), Do::Read("dev".into())]);
        let lead_tile = Group { actor: "lead".into(), thing: None, got: None };
        assert!(kept(&lead_tile, &acts).is_empty(), "a lead's own tile: not one Esc per agent below it");
    }
}
