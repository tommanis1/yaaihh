//! What waits on the owner, said once (paper-research N-797.p1; the owner N-541: "i expect all the clones taken out
//! and software be made right"). Threads' "Needs you", the sidebar's count, an agent's "waits on you", Home's line and
//! the list by time all read [`on_owner`]; whether one ticket waits on him is [`owes`].

use crate::Proj;
use coord_types::TicketView;
use docket_machine::ViewStatus;

/// What kind of thing of his it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// A post of an agent's that asks him something still open: post POST of the thread.
    Question { post: u32 },
    /// An agent's question asked with the ask box (an `ask` ticket), its answer his.
    Ask,
    /// A step of his: a step owed, a halt (slow mode), a stop passed to him, his clarify.
    Step,
}

/// One thing that waits on him.
pub struct Wait<'a> {
    /// The thread (or ticket) it is on.
    pub t: &'a TicketView,
    /// The topic it stands in: the top of T's parents.
    pub goal: &'a TicketView,
    /// The agent that waits for his answer: who asked, else who has the thread.
    pub agent: Option<String>,
    pub kind: Kind,
}

/// V owes him something: coordd's word (`Machine::owes_owner`: a step of his down its tree, a halt, a stop, asks left
/// with him; closed too), on the outline it gives a thread or on the view. A coordd from before `owed` gives neither:
/// then it is read from the view's waits, halts and asks.
pub fn owes(v: &TicketView) -> bool {
    if let Some(o) = &v.outline {
        return o.owed || v.owed;
    }
    v.owed
        || halted(v)
        || owes_owner(v)
        || v.waits_on.iter().any(|w| w.agent == "owner")
        || asks_owner(v)
}

/// A post of V whose questions wait on the owner: the one test of his card and Needs (threads.rs `open_to_owner`), a
/// post typed an ask to him too (I-1480).
pub(crate) fn asks_owner(v: &TicketView) -> bool {
    v.posts.iter().any(|x| crate::threads::open_to_owner(v, x))
}

/// It halts for the owner (slow mode): a card waits on it or on a ticket under it.
pub(crate) fn halted(v: &TicketView) -> bool {
    !v.halts.is_empty() || !v.halted.is_empty()
}

/// It awaits a step of his, or rests with one of his owed.
pub fn owes_owner(v: &TicketView) -> bool {
    matches!(&v.status, ViewStatus::Awaits { agent, .. } if agent == "owner") || (matches!(v.status, ViewStatus::Resting) && v.waits_on.iter().any(|w| w.agent == "owner"))
}

/// A line of the outline, or a ticket of its own at the top.
fn listed(v: &TicketView) -> bool {
    !v.archived && v.archived_under.is_none() && (v.kind == "thread" || v.kind == "item" || (v.parent.is_none() && v.kind != "words"))
}

/// Everything that waits on him in project P: each open question of a post told to him, each ask box's question, and
/// each thread or ticket that owes him a step; the topics with most first, then the newest.
pub fn on_owner(p: &Proj) -> Vec<Wait<'_>> {
    on_owner_now(p)
}

/// When P's tickets last changed, as what is built from all of them reads it (streams.rs, threads.rs): its count of
/// ticket events, the last event's number, how many tickets it holds.
pub(crate) type TicketsAt = (u64, u64, usize);
pub(crate) fn tickets_at(p: &Proj) -> TicketsAt {
    (p.live.tickets_rev, p.live.seq, p.live.tickets.len())
}

thread_local! {
    /// [`on_owner`] of each project, as ticket numbers, with when its tickets last changed ([`tickets_at`]).
    static DRAWN: std::cell::RefCell<std::collections::HashMap<String, (TicketsAt, std::rc::Rc<Vec<(u64, u64, Option<String>, Kind)>>)>> = Default::default();
}

/// [`on_owner`] for what a view builds: read again when the project's tickets change, not on each view (the owner's
/// N-1793: it walked every post of every ticket, twice on every key typed in With you, for the agent's line and for its
/// outline). A handler, which may have just changed the tickets, asks [`on_owner`].
pub fn on_owner_drawn(p: &Proj) -> Vec<Wait<'_>> {
    let at = tickets_at(p);
    let back = |kept: &[(u64, u64, Option<String>, Kind)]| -> Option<Vec<Wait<'_>>> {
        kept.iter().map(|(t, g, agent, kind)| Some(Wait { t: p.live.tickets.get(t)?, goal: p.live.tickets.get(g)?, agent: agent.clone(), kind: *kind })).collect()
    };
    let held = DRAWN.with_borrow(|m| m.get(&p.id).filter(|(when, _)| *when == at).map(|(_, kept)| kept.clone()));
    if let Some(w) = held.and_then(|kept| back(&kept)) {
        return w;
    }
    let now = on_owner_now(p);
    let kept: Vec<(u64, u64, Option<String>, Kind)> = now.iter().map(|w| (w.t.n, w.goal.n, w.agent.clone(), w.kind)).collect();
    // kept only when each comes back by its number, its key in `tickets`
    if back(&kept).is_some_and(|b| b.len() == now.len()) {
        DRAWN.with_borrow_mut(|m| m.insert(p.id.clone(), (at, std::rc::Rc::new(kept))));
    }
    now
}

fn on_owner_now(p: &Proj) -> Vec<Wait<'_>> {
    let mut them: Vec<&TicketView> = p.live.tickets.values().filter(|v| listed(v) && owes(v)).collect();
    them.sort_by(|a, b| a.opened_at.cmp(&b.opened_at));
    let has = |id: &str| them.iter().any(|t| t.id == id);
    let mut out: Vec<Wait> = Vec::new();
    for &t in &them {
        let goal = crate::outline::goal(p, t);
        if t.kind == "ask" {
            let agent = t.opener.clone().or_else(|| t.form.get("by").and_then(|b| b.as_str()).map(str::to_string));
            out.push(Wait { t, goal, agent, kind: Kind::Ask });
            continue;
        }
        let asking: Vec<&docket_machine::Post> = t.posts.iter().filter(|q| crate::threads::open_to_owner(t, q)).collect();
        for q in &asking {
            out.push(Wait { t, goal, agent: Some(q.by.clone()), kind: Kind::Question { post: q.n } });
        }
        // a step of his: unless every step it waits on is another line's (a subthread's, an ask box's)
        let elsewhere = !t.waits_on.is_empty() && t.halts.is_empty() && t.waits_on.iter().filter(|w| w.agent == "owner").all(|w| w.ticket != t.id && has(&w.ticket));
        if asking.is_empty() && !elsewhere {
            out.push(Wait { t, goal, agent: crate::outline::holder(t), kind: Kind::Step });
        }
    }
    // the topics with most first, then the newest; within one, as opened
    let count = |g: &str| out.iter().filter(|w| w.goal.id == g).count();
    let mut order: Vec<(&str, usize, String)> = Vec::new();
    for w in &out {
        if !order.iter().any(|o| o.0 == w.goal.id) {
            order.push((w.goal.id.as_str(), count(&w.goal.id), w.goal.opened_at.to_string()));
        }
    }
    order.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
    let rank = |g: &str| order.iter().position(|o| o.0 == g).unwrap_or(usize::MAX);
    let mut ranked: Vec<(usize, Wait)> = out.into_iter().map(|w| (rank(&w.goal.id), w)).collect();
    ranked.sort_by_key(|(r, _)| *r);
    ranked.into_iter().map(|(_, w)| w).collect()
}

/// Agent NAME waits for his answer on something in P: a ticket's, or a read on its card.
pub fn agent_waits(p: &Proj, name: &str) -> bool {
    on_owner(p).iter().any(|w| w.agent.as_deref() == Some(name)) || p.live.agents.iter().any(|a| a.name == name && !a.read_asks.is_empty())
}

/// How many things wait on him in P: the tickets' and the agents' reads.
pub fn count(p: &Proj) -> usize {
    on_owner(p).len() + crate::read_ask::asks(p).len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(v: serde_json::Value) -> TicketView {
        let mut base = json!({"n": 0, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [], "posts": []});
        for (k, x) in v.as_object().unwrap() {
            base[k] = x.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    fn proj(vs: Vec<TicketView>) -> Proj {
        let mut p = Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        for v in vs {
            p.live.tickets.insert(v.n, v);
        }
        p
    }

    #[test]
    fn a_one_off_closed_then_asked_by_an_agent_is_listed_while_the_asks_are_open() {
        // N-16: the owner closed it as OneOff; paper-research's post 2 after it told him and asked 2 things. Since 3h
        // (64b79a9) the view keeps those asks open (asks_open) until answered, nothing to answer, a later post by the
        // asker, or a thread following it: the page reads that
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let v: TicketView = serde_json::from_value(json!({"id": "N-16", "n": 16, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "t", "status": {"is": "done", "result": {"head": "OneOff"}}, "children": [], "program": "", "bound": 1, "spent": 1, "given": [],
            "opened_at": "2026-11-12T17:50:00Z", "closed_at": "2026-11-12T17:53:27Z",
            "waits_on": [{"ticket": "N-16", "agent": "owner", "call": "asks", "post": 2}],
            "posts": [{"n": 1, "at": "2026-11-12T17:50:00Z", "by": "owner", "text": "q", "names": ["paper-research"]},
                      {"n": 2, "at": "2026-11-12T17:54:11Z", "by": "paper-research", "text": "A? B?", "names": ["owner"], "asks": [{"at": [0, 2]}, {"at": [3, 5]}], "asks_open": true}]}))
        .unwrap();
        p.live.tickets.insert(16, v.clone());
        assert!(crate::threads::listed(&v));
        assert_eq!(crate::waits::on_owner(&p).len(), 1, "the sidebar's count has it");
        // closed by any of the four: off the list again, as a one-off is
        let mut closed = v.clone();
        closed.posts[1].asks_open = false;
        closed.waits_on.clear();
        assert!(!crate::threads::listed(&closed));
        // an open ask not told to the owner is not his
        let mut other = v.clone();
        other.posts[1].names = vec!["lead".into()];
        assert!(!crate::threads::listed(&other));
    }

    /// What a view reads of all the tickets (`on_owner_drawn`, outline.rs `pieces_drawn`) is kept between keys and read
    /// again when a ticket event comes (N-1793): the wait an event brings shows on the next view, and the one an event
    /// ends goes.
    #[test]
    fn what_a_view_keeps_of_the_waits_follows_each_ticket_event() {
        use coord_types::{Body, Event};
        let waits = |id: &str, n: u64, owed: bool| {
            let w = if owed { json!([{"ticket": id, "agent": "owner", "call": "clarify"}]) } else { json!([]) };
            view(json!({"id": id, "n": n, "form": {"to": "lead"}, "waits_on": w}))
        };
        let mut p = Proj::new(&crate::projects::ProjectConfig { name: "kept".into(), id: "kept".into(), boxed: false }, Vec::new());
        p.live.apply(Event { seq: 1, body: Body::Ticket(waits("N-1", 1, true)) });
        p.live.apply(Event { seq: 2, body: Body::Ticket(waits("N-2", 2, false)) });
        let ids = |p: &Proj| on_owner_drawn(p).iter().map(|w| w.t.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&p), ["N-1"]);
        assert_eq!(crate::outline::pieces_drawn(&p, Some("lead")).len(), 2, "both its topics");
        p.live.apply(Event { seq: 3, body: Body::Ticket(waits("N-2", 2, true)) });
        assert_eq!(ids(&p), ["N-1", "N-2"], "the wait the event brought");
        p.live.apply(Event { seq: 4, body: Body::Ticket(waits("N-1", 1, false)) });
        assert_eq!(ids(&p), ["N-2"], "the wait the event ended");
        assert_eq!(ids(&p).len(), on_owner(&p).len(), "as read afresh");
    }

    #[test]
    fn a_thread_whose_owners_post_waits_on_clarify_waits_on_the_owner() {
        // design §13, paper-research m1314: the handler ticket is hidden, so the thread carries the wait
        let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-12T10:00:00Z", "given": [], "posts": [],
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "clarify"}]}))
        .unwrap();
        assert!(crate::waits::owes_owner(&v));
        assert_eq!(crate::ticket::waits_line(&v).unwrap(), "Waits on owner (clarify, N-1)");
        assert!(!crate::waits::owes_owner(&TicketView { waits_on: vec![], ..v }), "a thread at rest owes nothing");
    }

    #[test]
    fn one_list_says_what_waits_on_him_and_who_waits() {
        // N-1: lead asks him two things in post 2; N-2: a step of his under N-1 (a subthread fixer has); N-3: at work
        let n1 = view(json!({"id": "N-1", "n": 1, "posts": [{"n": 1, "at": "2026-11-12T10:00:00Z", "by": "owner", "text": "hi", "names": ["lead"]},
            {"n": 2, "at": "2026-11-12T10:01:00Z", "by": "lead", "text": "A? B?", "told": ["owner"], "asks": [{"at": [0, 2]}, {"at": [3, 5]}], "asks_open": true}],
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "asks", "post": 2}, {"ticket": "N-2", "agent": "owner", "call": "decide"}]}));
        let n2 = view(json!({"id": "N-2", "n": 2, "parent": "N-1", "form": {"to": "fixer"}, "opened_at": "2026-11-12T11:00:00Z",
            "waits_on": [{"ticket": "N-2", "agent": "owner", "call": "decide"}]}));
        let n3 = view(json!({"id": "N-3", "n": 3, "form": {"to": "quiet"}, "status": {"is": "waits", "children": []}, "waits_on": [{"ticket": "N-3", "agent": "quiet", "call": "work"}]}));
        let p = proj(vec![n1, n2, n3]);
        let got: Vec<(String, Option<String>, Kind, String)> = on_owner(&p).iter().map(|w| (w.t.id.clone(), w.agent.clone(), w.kind, w.goal.id.clone())).collect();
        assert_eq!(
            got,
            vec![
                ("N-1".to_string(), Some("lead".to_string()), Kind::Question { post: 2 }, "N-1".to_string()),
                ("N-2".to_string(), Some("fixer".to_string()), Kind::Step, "N-1".to_string()),
            ]
        );
        // an agent "waits on you" exactly when the list has a thing of its
        assert!(agent_waits(&p, "lead") && agent_waits(&p, "fixer") && !agent_waits(&p, "quiet"));
        // answered: nothing of lead's is left, the step stays
        let mut p = p;
        let v = p.live.tickets.get_mut(&1).unwrap();
        v.posts[1].asks_open = false;
        v.waits_on.retain(|w| w.ticket != "N-1");
        assert_eq!(on_owner(&p).iter().map(|w| w.t.id.as_str()).collect::<Vec<_>>(), ["N-2"]);
        // lead's post 2 typed an ask to him (I-1480), marking no passage: lead's question again, from the one gate
        let v = p.live.tickets.get_mut(&1).unwrap();
        v.posts[1].asks.clear();
        v.posts[1].told.clear();
        v.posts[1].names = vec!["owner".into()];
        v.posts[1].kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
        v.posts[1].asks_open = true;
        let waits = on_owner(&p);
        let got: Vec<(&str, Option<&str>, Kind)> = waits.iter().map(|w| (w.t.id.as_str(), w.agent.as_deref(), w.kind)).collect();
        assert_eq!(got, [("N-1", Some("lead"), Kind::Question { post: 2 }), ("N-2", Some("fixer"), Kind::Step)]);
        assert!(asks_owner(&p.live.tickets[&1]) && agent_waits(&p, "lead"));
    }

    /// A made-up project for the stills: N-1 asks him two things (lead), N-2 under it owes him a step (fixer), N-3 at
    /// work (quiet), an ask box of designer's under N-3; times today, two days ago and ten days ago.
    fn standin() -> Proj {
        let ago = |h: i64| (chrono::Utc::now() - chrono::Duration::hours(h)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let n1 = view(json!({"id": "N-1", "n": 1, "title": "the usage page by the hour", "opened_at": ago(240), "posts": [
            {"n": 1, "at": ago(240), "by": "owner", "text": "show me the month by the hour", "names": ["lead"]},
            {"n": 2, "at": ago(50), "by": "lead", "text": "Which month first? And the gateway's rows too?", "told": ["owner"], "asks": [{"at": [0, 17]}, {"at": [18, 46]}], "asks_open": true},
            {"n": 3, "at": ago(1), "by": "fixer", "text": "the chart is in", "told": ["owner"]}],
            "waits_on": [{"ticket": "N-1", "agent": "owner", "call": "asks", "post": 2}, {"ticket": "N-2", "agent": "owner", "call": "decide"}]}));
        let n2 = view(json!({"id": "N-2", "n": 2, "parent": "N-1", "title": "the span's totals", "form": {"to": "fixer"}, "opened_at": ago(26),
            "waits_on": [{"ticket": "N-2", "agent": "owner", "call": "decide"}]}));
        let n3 = view(json!({"id": "N-3", "n": 3, "title": "the terminal's theme", "form": {"to": "quiet"}, "status": {"is": "waits", "children": []}, "opened_at": ago(2),
            "waits_on": [{"ticket": "N-3", "agent": "quiet", "call": "work"}]}));
        let q5 = view(json!({"id": "Q-5", "n": 5, "kind": "ask", "under": "N-3", "opener": "designer", "title": "Dark or light for the terminal?", "opened_at": ago(3),
            "waits_on": [{"ticket": "Q-5", "agent": "owner", "call": "question"}], "owed": true}));
        let mut p = proj(vec![n1, n2, n3, q5]);
        let agent = |name: &str, state: &str, since: String| -> coord_types::AgentView {
            serde_json::from_value(json!({"id": "", "name": name, "parent": "", "kind": "claude", "model": "", "state": state, "since": since,
                "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null})).unwrap()
        };
        p.live.agents = vec![agent("lead", "idle", ago(50)), agent("fixer", "idle", ago(26)), agent("quiet", "working", ago(1)), agent("designer", "idle", ago(3)), agent("old", "idle", ago(300))];
        p.live.project = json!({"view": [{"id": "home", "kind": "overview", "items": "", "runner": ""}]});
        p
    }

    /// The pages "what waits on you" and the times are drawn on, from the made-up project (never a coordd). The
    /// `typed-*` ones: lead's N-1.p2 typed an ask to him (`coord post --ask owner`, I-1480), marking no passage: open
    /// on its thread, its first line under "Waits on you" beside it, then closed by his "nothing to answer", and
    /// answered by his post 4.
    /// `STILL_OUT=DIR cargo test --profile quick -p yaaihh --lib waits_stills -- --ignored`.
    #[test]
    #[ignore]
    fn waits_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn on(sel: crate::Sel, shown: Option<u64>) -> crate::App {
            let (_held, mut s) = crate::view_cost::test_app();
            s.projects = vec![standin()];
            s.cur = 0;
            s.sel = sel;
            s.tickets.shown = shown;
            s
        }
        // lead's N-1.p2 an ask typed to him, its how closed (None: open)
        fn typed(sel: crate::Sel, shown: Option<u64>, closed: Option<docket_machine::line::AsksClosed>) -> crate::App {
            let mut s = on(sel, shown);
            let v = s.projects[0].live.tickets.get_mut(&1).unwrap();
            let q = &mut v.posts[1];
            q.asks.clear();
            q.told.clear();
            q.names = vec!["owner".into()];
            q.kind = Some(docket_machine::PostKind::Ask { to: vec!["owner".into()] });
            q.text = serde_json::from_value(json!("Which month first, November or October? The gateway's rows are in both.")).unwrap();
            v.waits_on.retain(|w| w.ticket != "N-1");
            if let Some(c) = closed {
                let answer = matches!(c, docket_machine::line::AsksClosed::Answered { .. });
                let q = &mut v.posts[1];
                q.asks_open = false;
                q.asks_closed = Some(c);
                if answer {
                    let at = v.posts[2].at.clone();
                    v.posts.push(serde_json::from_value(json!({"n": 4, "at": at, "by": "owner", "text": "November first.", "names": ["lead"]})).unwrap());
                }
            }
            s
        }
        let shots: [(&str, fn() -> crate::App, &str); 9] = [
            ("threads.png", || on(crate::Sel::Tickets, None), "1400x900"),
            ("threads-narrow.png", || on(crate::Sel::Tickets, None), "760x900"),
            ("thread.png", || on(crate::Sel::Tickets, Some(1)), "1400x900"),
            ("agents.png", || on(crate::Sel::Agents, None), "1400x900"),
            ("home.png", || on(crate::Sel::View("home".into()), None), "1400x900"),
            ("by-time.png", || {
                let mut s = on(crate::Sel::Tickets, None);
                s.tickets.by_time = true;
                s
            }, "1400x900"),
            ("typed-thread.png", || typed(crate::Sel::Tickets, Some(1), None), "1400x900"),
            ("typed-nothing.png", || typed(crate::Sel::Tickets, Some(1), Some(docket_machine::line::AsksClosed::NothingToAnswer)), "1400x900"),
            ("typed-answered.png", || typed(crate::Sel::Tickets, Some(1), Some(docket_machine::line::AsksClosed::Answered { post: 4, by: "owner".into() })), "1400x900"),
        ];
        for (name, init, size) in shots {
            let (w, h) = size.split_once('x').map(|(w, h)| (w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap())).unwrap();
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), size.into(), "--at".into(), "1.0".into(), "--tree".into(), out.join(name.replace(".png", ".json")).display().to_string()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (w, h), init, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
