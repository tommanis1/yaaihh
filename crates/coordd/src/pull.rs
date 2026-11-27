//! An agy session that pulls (N-1998; std/bridges/agy.docket's Pull-Brain, on coordd): `coord read-brain` hands it
//! what is due and answers its briefing, `coord report-work` answers what its pulls handed. From its first pull on,
//! nothing is pushed into the session but the owner's words while no turn runs (`Core::hand_job`), so a session kicked
//! off by its first message works on its own timer. A pull missed past its wake and the grace is told once, softly: a
//! journal line and `missed` in its pull, which its view shows; the session is never woken, stopped or written into.

use crate::core::{Agent, Core, Who};
use coord_types::Request;
use docket_machine::{Delivery, Machine, What};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The minutes past a pull's wake before it counts as missed.
pub const GRACE_MINS: i64 = 5;
/// The wake told: while work waits, while none does, and while a check of its own runs (std.bridges.agy's defaults).
pub const WORK_MINS: i64 = 10;
pub const SLEEP_MINS: i64 = 20;
pub const WATCH_MINS: i64 = 3;

/// An agy session's last pull: when (unix seconds), the wake it was told (minutes), its own note, the messages its
/// pulls handed that no report answered yet, the owner's last word handed, and the due time of a missed pull told.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pull {
    pub at: i64,
    pub wake: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handed: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub directive: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missed: Option<i64>,
}

impl Pull {
    /// When the next pull is due: its wake and the grace after it; none when it was told to set no timer.
    pub fn due(&self) -> Option<i64> {
        (self.wake > 0).then(|| self.at + (self.wake + GRACE_MINS) * 60)
    }
}

/// The open items each agent started and the open checks it opened: what a pull reads of the tickets, kept from each
/// ticket's change (`Effect::Changed`), so that a pull touches only its agent's and never a closed ticket (consult-lead-c
/// N-1895 p331). Read whole once, when the tickets load.
#[derive(Debug, Default)]
pub struct Holds {
    by: BTreeMap<String, BTreeSet<u64>>,
    of: BTreeMap<u64, Vec<String>>,
}

impl Holds {
    pub fn read_all(&mut self, m: &Machine) {
        *self = Holds::default();
        for &t in m.tickets.keys() {
            self.see(m, t);
        }
    }

    /// Ticket T as it is now: held by its starters while an open item, by its opener while an open check.
    pub fn see(&mut self, m: &Machine, t: u64) {
        for a in self.of.remove(&t).unwrap_or_default() {
            if let Some(s) = self.by.get_mut(&a) {
                s.remove(&t);
                if s.is_empty() {
                    self.by.remove(&a);
                }
            }
        }
        let Some(x) = m.tickets.get(&t).filter(|x| x.is_open()) else { return };
        let who: Vec<String> = match x.kind.as_str() {
            "item" => x.started.clone(),
            "check" => x.opener.iter().cloned().collect(),
            _ => return,
        };
        for a in &who {
            self.by.entry(a.clone()).or_default().insert(t);
        }
        if !who.is_empty() {
            self.of.insert(t, who);
        }
    }

    /// The tickets NAME holds.
    pub fn of_agent<'a>(&'a self, name: &str) -> impl Iterator<Item = u64> + 'a {
        self.by.get(name).into_iter().flatten().copied()
    }
}

/// Whether D holds a post of the owner's: what is still pushed to a session that pulls.
pub fn from_owner(d: &Delivery) -> bool {
    match &d.what {
        What::Posts { unread } | What::Step { unread, .. } | What::Closed { unread, .. } => unread.iter().any(|p| p.by == "owner"),
        _ => false,
    }
}

impl Core {
    /// Whether A is an agy session that has pulled: pushed only the owner's words, while no turn runs.
    pub(crate) fn pulls(&self, a: &Agent) -> bool {
        a.kind == "agy" && a.live.pull.is_some()
    }

    /// Who may pull: an agy agent, for itself, while its session runs.
    pub(crate) fn may_pull(&self, who: &Who) -> Result<String, String> {
        let Who::Agent(me) = who else { return Err("coord read-brain runs inside an agent session".into()) };
        match self.agents.get(me) {
            Some(a) if a.kind != "agy" => Err(format!("coord read-brain: {me} is a {} agent, which is handed its messages; only an agy agent pulls", a.kind)),
            Some(a) if !a.running() => Err(format!("coord read-brain: {me} runs no session")),
            Some(_) => Ok(me.clone()),
            None => Err(format!("coord read-brain: no agent {me}")),
        }
    }

    /// The open items NAME started (`coord item start`), each `I-12: its title`.
    fn started_items(&self, name: &str) -> Vec<String> {
        let m = &self.tickets.m;
        let open_item = |t: &u64| m.tickets.get(t).is_some_and(|x| x.kind == "item" && x.is_open() && x.started.iter().any(|s| s == name));
        self.tickets.holds.of_agent(name).filter(open_item).map(|t| (m.id(t), m.view(t).map(|v| v.title).unwrap_or_default())).map(|(id, title)| if title.is_empty() { id } else { format!("{id}: {title}") }).collect()
    }

    /// ID's thread as a directive, `N-12: its title`, when ID is a post of the owner's.
    fn owners_post(&self, id: &str) -> Option<String> {
        let (t, n) = docket_machine::post_id(id)?;
        let x = self.tickets.m.tickets.get(&t)?;
        x.posts.iter().find(|p| p.n == n && p.by == "owner")?;
        let title = self.tickets.m.view(t).map(|v| v.title).unwrap_or_default();
        Some(if title.is_empty() { self.tickets.m.id(t) } else { format!("{}: {title}", self.tickets.m.id(t)) })
    }

    /// Whether NAME waits on a check of its own: an open check ticket it opened (`coord ready`).
    fn watching(&self, name: &str) -> bool {
        self.tickets.holds.of_agent(name).any(|t| self.tickets.m.tickets.get(&t).is_some_and(|x| x.kind == "check" && x.is_open() && x.opener.as_deref() == Some(name)))
    }

    /// NAME's briefing (std.bridges.agy's `Briefing`), its pull noted: LINES the messages handed now (HANDED their
    /// ids), DIRECTIVES the owner's words among them. The mode is "stop" while it is held or stopping, "work" while
    /// anything was handed or an item of its own is open, else "sleep".
    pub(crate) fn briefing(&mut self, name: &str, note: &str, lines: Vec<String>, handed: Vec<String>, directives: Vec<String>) -> Value {
        let items = self.started_items(name);
        let stop = self.holding || self.tickets.stopping.contains(name) || !self.agents[name].running();
        let unanswered = !self.agents[name].live.pushed.is_empty() || self.agents[name].live.pull.as_ref().is_some_and(|p| !p.handed.is_empty());
        let mode = if stop {
            "stop"
        } else if !lines.is_empty() || !items.is_empty() || unanswered {
            "work"
        } else {
            "sleep"
        };
        let wake = match mode {
            "stop" => 0,
            _ if self.watching(name) => WATCH_MINS,
            "work" => WORK_MINS,
            _ => SLEEP_MINS,
        };
        let old = self.agents[name].live.pull.clone().unwrap_or_default();
        // what was pushed to it since: named, to be answered with the rest, his words among them its directive
        let pushed = std::mem::take(&mut self.agents.get_mut(name).unwrap().live.pushed);
        let (mut lines, mut directives) = (lines, directives);
        for id in &pushed {
            lines.push(format!("{id}: given to you when it started your turn; answer it with report_work"));
            directives.extend(self.owners_post(id));
        }
        let directive = directives.last().cloned().unwrap_or(old.directive);
        let mut waiting = old.handed;
        waiting.extend(handed.into_iter().chain(pushed).filter(|id| !waiting.contains(id)).collect::<Vec<_>>());
        let pull = Pull { at: crate::accounts::mover::now_s(), wake, note: note.trim().to_string(), handed: waiting, directive: directive.clone(), missed: None };
        self.agents.get_mut(name).unwrap().live.pull = Some(pull);
        self.changed(name);
        let hint = if mode == "stop" {
            "set no timer: the session is held or stopping; it is started again by him".to_string()
        } else {
            format!("pull again in {wake} min (read_brain); a pull later than {} min from now is told to the owner", wake + GRACE_MINS)
        };
        json!({"mode": mode, "priority_directive": directive, "new_directives": directives, "unread_messages": lines,
               "assigned_items": items, "suggested_wake_mins": wake, "wake_hint": hint})
    }

    /// `coord report-work`: SUMMARY (and REFS) posted as WHO's answer to the messages its pulls handed, the first where
    /// it goes, as the reply tool posts; with ITEM, the item started first if it is not yet (claimed), and the report
    /// posted on it when no message waits. The post request to make, the handed ones cleared.
    pub(crate) fn report_work(&mut self, who: &Who, summary: &str, refs: &[String], item: Option<&str>) -> Result<Request, String> {
        let Who::Agent(me) = who else { return Err("coord report-work runs inside an agent session".into()) };
        let me = me.clone();
        if summary.trim().is_empty() {
            return Err("coord report-work: say what was done".into());
        }
        if let Some(i) = item.filter(|i| !self.started_items(&me).iter().any(|s| s.split(':').next() == Some(*i))) {
            let r = self.item_start(who, i);
            if !r.ok {
                return Err(r.out);
            }
        }
        let text = if refs.is_empty() { summary.trim().to_string() } else { format!("{}\n\nShown by: {}", summary.trim(), refs.join(", ")) };
        let mut handed = self.agents.get(&me).and_then(|a| a.live.pull.as_ref()).map(|p| p.handed.clone()).unwrap_or_default();
        handed.extend(self.agents.get(&me).map(|a| a.live.pushed.clone()).unwrap_or_default().into_iter().filter(|id| !handed.contains(id)).collect::<Vec<_>>());
        let mut ids: Vec<(String, u32)> = handed.iter().filter_map(|id| docket_machine::post_or_ticket(id).map(|(t, n)| (t.to_string(), n.unwrap_or(0)))).filter(|(_, n)| *n > 0).collect();
        let req = if let Some((t, n)) = (!ids.is_empty()).then(|| ids.remove(0)) {
            Request::Post { kind: None, t, text, to: None, answers: Some(n), wait: false, also: ids.iter().map(|(t, n)| format!("{t}.p{n}")).collect(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() }
        } else if let Some(i) = item {
            Request::Post { kind: None, t: i.to_string(), text, to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() }
        } else {
            return Err("coord report-work: no message your pulls handed waits for an answer, and no item was named (--item ITEM)".into());
        };
        if let Some(a) = self.agents.get_mut(&me) {
            a.live.pushed.clear();
            if let Some(p) = a.live.pull.as_mut() {
                p.handed.clear();
                p.directive.clear();
            }
        }
        self.changed(&me);
        Ok(req)
    }

    /// Each session that pulls and is past its next pull's due time, told once for that due time: a journal line, and
    /// `missed` in its pull. Never while deliveries are held or it is stopping, and nothing is handed or stopped.
    pub(crate) fn pulls_missed(&mut self) -> Vec<String> {
        let now = crate::accounts::mover::now_s();
        let late: Vec<(String, i64)> = self
            .agents
            .values()
            .filter(|a| self.pulls(a) && a.running() && !self.holding && !self.tickets.stopping.contains(&a.name))
            .filter_map(|a| a.live.pull.as_ref().and_then(|p| p.due().filter(|d| now > *d && p.missed != Some(*d))).map(|d| (a.name.clone(), d)))
            .collect();
        for (name, due) in &late {
            let p = self.agents.get_mut(name).unwrap().live.pull.as_mut().unwrap();
            p.missed = Some(*due);
            eprintln!("coordd: {name}: agy_missed_checkin: its pull was due by {} (wake {} min and {GRACE_MINS} min grace); nothing is handed or stopped", crate::core::at_secs(*due), p.wake);
            self.changed(name);
        }
        late.into_iter().map(|(n, _)| n).collect()
    }
}

#[cfg(test)]
#[path = "tests/pull.rs"]
mod tests;
