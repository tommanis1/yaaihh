//! A reference's two parts (the owner N-286.p1, paper-research N-101 p764): its id, exact (`P-2`, `orch5`), and a
//! label that says what it is ("part about-note", "In repository a, add notes…"). Wherever a reference is shown both
//! are, the label to read and the id small and grey after it; an id never stands alone.
//!
//! This is the app's one place for labels, so that every page reads them alike. The labels are coordd's own (`coord
//! label ID…`, docket-core2's `docket_machine::label`, paper-research N-101 p815, p843): one request asks for every
//! ticket and agent the app holds, again when what it holds changes, and the answers are kept on the project. The app
//! composes none itself: a reference coordd has not answered (or a coordd older than the request) reads as its kind,
//! an agent as its role.

use crate::{App, Pending, Proj};
use coord_types::{Label, Request};
use docket_machine::TicketView;
use rune::{El, Tx, row, text};
use rune_kit::Kit;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{Duration, Instant};

/// The labels coordd gave a project's references, by id; what they were asked for (a digest of the tickets and agents
/// held then), and whether an ask is out, or coordd does not know the request.
#[derive(Default)]
pub struct Held {
    by: HashMap<String, String>,
    /// Counts the changes to `by`: what is drawn from the labels is drawn again when it moves (post.rs `as_drawn`).
    rev: u64,
    sig: u64,
    out: bool,
    pub off: bool,
    at: Option<Instant>,
}

/// The least time between two asks while what the app holds keeps changing.
const ASK_EVERY: Duration = Duration::from_secs(2);

impl Held {
    /// coordd's label of ID, if it gave one.
    pub fn get(&self, id: &str) -> Option<&str> {
        self.by.get(id).map(String::as_str).filter(|l| !l.is_empty())
    }

    /// The answer to an ask: one `Label` a line. A refusal means a coordd without the request; it is not asked again
    /// until the pipe comes up anew.
    pub fn took(&mut self, ok: bool, out: &str) {
        self.out = false;
        if !ok {
            self.off = true;
            return;
        }
        self.rev += 1;
        for l in out.lines().filter_map(|l| serde_json::from_str::<Label>(l).ok()) {
            if l.is.is_empty() {
                self.by.remove(&l.id);
            } else {
                self.by.insert(l.id, l.label);
            }
        }
    }

    /// ID's label as he just gave it (threads.rs `retitle`), until coordd's next answer.
    pub fn set(&mut self, id: &str, label: &str) {
        self.rev += 1;
        self.by.insert(id.to_string(), label.to_string());
    }

    /// The count of changes to the labels held.
    pub fn rev(&self) -> u64 {
        self.rev
    }

    /// The pipe went down: what was out is not coming, and the coordd that answers next may know the request.
    pub fn down(&mut self) {
        (self.out, self.off, self.sig) = (false, false, 0);
    }
}

/// What a project's labels depend on, as a digest: each ticket's place in its run and each agent's state.
fn sig(p: &Proj) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut ts: Vec<&TicketView> = p.live.tickets.values().collect();
    ts.sort_by_key(|t| t.n);
    for t in ts {
        (&t.id, t.posts.len(), t.children.len(), t.closed_at.is_some(), t.spent, t.waits_on.len(), std::mem::discriminant(&t.status)).hash(&mut h);
    }
    for a in &p.live.agents {
        (&a.name, &a.state, &a.role, &a.on).hash(&mut h);
    }
    h.finish()
}

/// Each update: every connected project whose tickets or agents changed since its labels were asked is asked again
/// (at most every ASK_EVERY; the window wakes for the next one).
/// The ids P's labels are asked for: its tickets, its agents, the tickets its agents are on, and the commits its
/// tickets ended with (`REPO:SHA`, from a result's `commit` field, one per repository), which coordd labels by their
/// subjects.
pub fn ids_of(p: &Proj) -> Vec<String> {
    let mut ids: Vec<String> = p.live.tickets.values().map(|t| t.id.clone()).collect();
    ids.extend(p.live.agents.iter().map(|a| a.name.clone()));
    // the tickets the agents are on (AgentView.on), which the window may not hold: their lines show those labels
    ids.extend(p.live.agents.iter().filter_map(|a| a.on.clone()).filter(|id| !p.live.tickets.values().any(|t| &t.id == id)));
    ids.sort();
    ids.dedup();
    for t in p.live.tickets.values() {
        if let docket_machine::ViewStatus::Done { result } = &t.status
            && let Some(c) = result.get("commit").and_then(|c| c.as_object())
        {
            ids.extend(c.iter().filter_map(|(r, sha)| sha.as_str().map(|sha| format!("{r}:{sha}"))));
        }
    }
    ids
}

pub fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let mut any = false;
    for p in &mut s.projects {
        if p.down.is_some() || p.labels.off || p.labels.out || p.link.is_none() {
            continue;
        }
        let now = sig(p);
        if now == p.labels.sig {
            continue;
        }
        if let Some(d) = p.labels.at.map(|t| t.elapsed()).filter(|d| *d < ASK_EVERY) {
            tx.wake_after(ASK_EVERY - d);
            continue;
        }
        let ids = ids_of(p);
        if ids.is_empty() {
            p.labels.sig = now;
            continue;
        }
        if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&Request::Label { ids })) {
            p.pending.insert(id, Pending::Labels);
            (p.labels.sig, p.labels.out, p.labels.at) = (now, true, Some(Instant::now()));
            any = true;
        }
    }
    any
}

/// A reference: what it is, in words, and its id.
#[derive(Clone, Debug, PartialEq)]
pub struct Ref {
    pub label: String,
    pub id: String,
}

/// Ticket V's reference: coordd's label of it; until coordd has answered (or one that does not know the request),
/// [`unlabelled`].
pub fn ticket(p: Option<&Proj>, v: &TicketView) -> Ref {
    let label = p.and_then(|p| p.labels.get(&v.id)).map_or_else(|| unlabelled(v), str::to_string);
    Ref { label, id: v.id.clone() }
}

/// V's words before coordd has labelled it. One word for one thing (0.2's C16, the owner: no "note" beside
/// "thread"): a thread, of either kind, reads as its title's first line, else "a thread"; never its kind's name. Any
/// other kind as its kind.
pub fn unlabelled(v: &TicketView) -> String {
    if !matches!(v.kind.as_str(), "thread" | "note") {
        return v.kind.clone();
    }
    let first = crate::sidebar::first_line(&v.title);
    if first.is_empty() || first == v.kind { THREAD.to_string() } else { first }
}

/// What a thread is called where nothing names it: one word for threads and notes alike.
pub const THREAD: &str = "a thread";

/// The reference of the ticket called ID in project P; one the app does not hold reads as its id alone, labelled
/// "a thread" when its id is a thread's (`N-`), else "a ticket".
pub fn ticket_id(p: &Proj, id: &str) -> Ref {
    match p.live.tickets.values().find(|t| t.id == id) {
        Some(v) => ticket(Some(p), v),
        None => Ref { label: p.labels.get(id).unwrap_or(if id.starts_with("N-") { THREAD } else { "a ticket" }).to_string(), id: id.to_string() },
    }
}

/// NAME as he reads it: "you" for the owner (beside agents' names, which are lowercase), else the name itself.
pub fn person(name: &str) -> String {
    if name == "owner" { "you".into() } else { name.to_string() }
}

/// Who did or does a step, as the run tree says it (`role orchestrator`, an agent's name, `owner`): the agent that
/// holds the role, by name, with its role after it.
pub fn who(p: Option<&Proj>, w: &str) -> Ref {
    if let Some(role) = w.strip_prefix("role ") {
        let holder = p.and_then(|p| p.live.agents.iter().find(|a| a.role == role && a.state != "stopped"));
        return match holder {
            Some(a) => Ref { label: a.name.clone(), id: role.to_string() },
            None => Ref { label: format!("the {role}"), id: String::new() },
        };
    }
    if w == "owner" {
        return Ref { label: "you".into(), id: String::new() };
    }
    let role = p.and_then(|p| p.live.agents.iter().find(|a| a.name == w)).map(|a| a.role.clone()).filter(|r| !r.is_empty() && r != "member");
    Ref { label: w.to_string(), id: role.unwrap_or_default() }
}

/// What agent NAME is for: coordd's label of it (its role, else where it works now); until coordd has answered, its
/// role. Empty when neither says more than "member".
pub fn agent_label(p: &Proj, name: &str) -> String {
    let role = || p.live.agents.iter().find(|a| a.name == name).map(|a| a.role.as_str()).unwrap_or("");
    match p.labels.get(name).unwrap_or_else(role) {
        "member" => String::new(),
        l => l.to_string(),
    }
}

/// R drawn: its label in FONT and COLOR, its id small and grey after it.
pub fn el(r: &Ref, font: rune::TextStyle, color: rune::Color, k: &Kit) -> El {
    let mut c = row().gap(6.0).center_y().child(text(r.label.clone()).font(font).color(color).truncate().shrink());
    if !r.id.is_empty() {
        c = c.child(text(r.id.clone()).font(k.ty.small).color(k.c.text_faint));
    }
    c
}

/// A count beside its word, the one way (docs/specs/window-rules.md §4): "Waits on you · 4".
pub fn count(word: &str, n: usize) -> String {
    format!("{word} · {n}")
}

/// T's first N characters, with "…" when cut.
pub fn cut(t: &str, n: usize) -> String {
    if t.chars().count() <= n { t.to_string() } else { format!("{}…", t.chars().take(n).collect::<String>().trim_end()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordds_labels_are_kept_by_id_and_a_refusal_stops_the_asking() {
        let mut h = Held::default();
        h.took(true, "{\"id\":\"P-2\",\"is\":\"ticket\",\"label\":\"part about-note\"}\n{\"id\":\"orch5\",\"is\":\"agent\",\"label\":\"orchestrator\"}\n{\"id\":\"X-9\",\"is\":\"\",\"label\":\"\"}");
        assert_eq!((h.get("P-2"), h.get("orch5"), h.get("X-9")), (Some("part about-note"), Some("orchestrator"), None));
        assert!(!h.off);
        h.took(false, "coord bridge: not a request: unknown variant `Label`");
        assert!(h.off);
        assert_eq!(h.get("P-2"), Some("part about-note"));
        h.down();
        assert!(!h.off);
    }

    /// 0.2's C16 (the owner: no "note" beside "thread"): before coordd's label, a thread of either kind reads as its
    /// title's first line, else "a thread", never as its kind's name; an id the window does not hold reads "a thread"
    /// when it is a thread's.
    #[test]
    fn a_thread_without_a_label_is_never_called_by_its_kind() {
        let v = |kind: &str, title: &str| -> coord_types::TicketView {
            serde_json::from_value(serde_json::json!({"id": "N-7", "n": 7, "kind": kind, "opener": "owner", "ctx": {}, "form": {}, "title": title,
                "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-20T08:00:00Z", "given": [], "posts": []}))
            .unwrap()
        };
        assert_eq!(super::ticket(None, &v("note", "")).label, "a thread");
        assert_eq!(super::ticket(None, &v("note", "note")).label, "a thread");
        assert_eq!(super::ticket(None, &v("thread", "thread")).label, "a thread");
        assert_eq!(super::ticket(None, &v("thread", "\n  fix the box\nand the rest")).label, "fix the box");
        assert_eq!(super::ticket(None, &v("item", "")).label, "item", "another kind keeps its word");
        let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        assert_eq!(super::ticket_id(&p, "N-404").label, "a thread");
        assert_eq!(super::ticket_id(&p, "Q-9").label, "a ticket");
    }
}
