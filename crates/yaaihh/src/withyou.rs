//! With you: the list of threads narrowed to one agent, beside the thread picked (paper-research N-800, N-873.p14; the
//! owner, N-944 post 1: "threads as in the general threads and threads for a pseici agent with you look too different
//! of an experience"). It is the Threads page's pane, narrowed to the agent (item 15, paper-research N-1446 p2603):
//! its list column (`threads::list`: the outline or the list by time, "+ New thread", the threads archived with the
//! agent at its foot), its reading pane (`threads::reading`: the centred column, opened where he last read, else at the
//! foot), its one pick (`threads::picked`) and its way back. A list of threads is one thing wherever it stands
//! (window-rules §8). With nothing picked, the first row he would look at is shown (`outline::first`). In a narrow pane
//! the list alone; a press replaces it with the thread and `‹ With you`.
//!
//! His talk with the agent is the page: the agent's errands with other agents alone, threads he takes no part in,
//! stand behind one row at the list's foot, "Between agents", folded, never beside his, and With you never opens on
//! one unasked (outline.rs `apart`; his walk items 6 and 15, yaaihh/fresh N-1724 p22, paper-research N-1751).
//!
//! The one box says where his words go: on a thread ("On: label · N-541", × makes it a new thread), or a new thread
//! with the agent. `Reply` on a post, or picking a thread, binds it.

use crate::App;
use coord_types::TicketView;
use rune::{El, col, row};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct WithYou {
    /// Where his words to the agent go, as he chose it: a thread's id, or None for a new thread. Not chosen: the
    /// thread his last message continues (`streams::follow_up`), else a new thread.
    pub on: HashMap<(usize, String), Option<String>>,
    /// The thread shown beside the list, as he picked it, by (project, agent).
    pub inside: HashMap<(usize, String), u64>,
    /// The projects whose picks above came back from ui.json (state.rs `restore`) and wait for its first snapshot to
    /// be checked ([`check_restored`]).
    pub restored: HashSet<usize>,
}

/// Project PI's first snapshot is in: the picks ui.json gave back for it that no longer stand are dropped, each a line
/// in yaaihh.log and nothing shown to him (consult-3, N-1396 p331). That covers an agent no longer there (retired
/// since: its picks and its view, as a missing project's are), a thread beside the list that is gone or no longer in
/// its list (the list's first takes its place, [`view`]), and his words' thread when it has closed or left the
/// agent's list (they go as with nothing chosen, [`bound`]). His own picks after the start are his and are not checked.
pub fn check_restored(s: &mut App, pi: usize) {
    if !s.withyou.restored.remove(&pi) {
        return;
    }
    let Some(p) = s.projects.get(pi) else { return };
    let here = |a: &str| p.live.agents.iter().any(|x| x.name == a);
    let id_of = |n: u64| p.live.tickets.get(&n).map_or_else(|| format!("ticket {n}"), |v| v.id.clone());
    let mut said: Vec<String> = Vec::new();
    s.withyou.inside.retain(|(i, a), n| {
        let keep = *i != pi || stands(p, a, *n);
        if !keep {
            said.push(format!("{} beside {a}'s list in With you is no longer there; the list's first is shown", id_of(*n)));
        }
        keep
    });
    s.withyou.on.retain(|(i, a), t| {
        let open = |t: &str| p.live.tickets.values().find(|v| v.id == t).is_some_and(|v| crate::outline::in_list(p, a, v.n, true));
        let keep = *i != pi || (here(a) && t.as_deref().is_none_or(open));
        if !keep {
            said.push(format!("{} is gone; {a}'s words go unthreaded", t.as_deref().unwrap_or("a new thread")));
        }
        keep
    });
    s.streams.which.retain(|(i, a), _| {
        let keep = *i != pi || here(a);
        if !keep {
            said.push(format!("{a} left the project"));
        }
        keep
    });
    for line in said {
        crate::log::line(&format!("ui.json: {} {line}", p.id));
    }
}

/// Whether thread N may stand beside agent A's list in project P: A is still among its agents and N is in A's list,
/// an ended-today thread included. What [`check_restored`] keeps of ui.json, and what a With you lens's kept thread
/// must be to open beside its list again (lens.rs `show_sel`).
pub(crate) fn stands(p: &crate::Proj, a: &str, n: u64) -> bool {
    p.live.agents.iter().any(|x| x.name == a) && crate::outline::in_list(p, a, n, false)
}

/// The thread his next words to agent A go on: the one he chose, else the one they continue; None: a new thread.
pub fn bound(s: &App, pi: usize, a: &str) -> Option<String> {
    match s.withyou.on.get(&(pi, a.to_string())) {
        Some(on) => on.clone(),
        None => s.projects.get(pi).and_then(|p| crate::streams::follow_up(p, a)),
    }
}

/// The box's line for agent A: where the words go, and whether × is there to undo it.
pub fn line(s: &App, pi: usize, a: &str) -> (String, bool) {
    match bound(s, pi, a).and_then(|t| s.projects.get(pi)?.live.tickets.values().find(|v| v.id == t)) {
        Some(v) => (format!("On: {} · {}", crate::streams::clip(&label(s.projects.get(pi), v), 60), v.id), true),
        None => (crate::look::say(s, "new_thread_with", &[("agent", a)]), false),
    }
}

/// × on the box's line: his words to A start a new thread.
pub fn unbind(s: &mut App, pi: usize, a: &str) {
    s.withyou.on.insert((pi, a.to_string()), None);
}

pub fn bind(s: &mut App, pi: usize, a: &str, t: &str) {
    s.withyou.on.insert((pi, a.to_string()), Some(t.to_string()));
}

/// A thread as he reads it: its title's first line, else its label.
fn label(p: Option<&crate::Proj>, v: &TicketView) -> String {
    let first = v.title.lines().next().unwrap_or("").trim();
    if first.is_empty() || first == v.kind { crate::refs::ticket(p, v).label } else { first.to_string() }
}

/// With you for agent A of project PI: the list, and the thread shown beside it; narrow, one of the two. FOCUSED: in the
/// focused pane (the search field is its list's, [`list`]).
pub fn view(s: &App, pi: usize, a: &coord_types::AgentView, focused: bool, k: &Kit) -> El {
    let name = a.name.as_str();
    let Some(p) = s.projects.get(pi) else { return col() };
    let narrow = s.narrow.get() || !s.panes.is_empty();
    let (shown, new) = beside(s, p, pi, name);
    // the Threads page's list column, narrowed to the agent (threads.rs `list`; item 15)
    let list = crate::threads::list(s, p, Some(name), shown.map(|v| v.n), focused, k);
    let right = match shown {
        Some(v) => whole(s, p, v, k),
        None => col().w_full().grow(1.0).min_h(0.0).child(rune::el().grow(1.0)).child(crate::chat::composer(s, pi, a, k)),
    };
    match (narrow, shown.is_some() || new) {
        (true, true) => col().w_full().grow(1.0).min_h(0.0).child(crate::threads::back(pi, Some(name), k)).child(right),
        (true, false) => col().w_full().grow(1.0).min_h(0.0).child(row().w_full().grow(1.0).min_h(0.0).child(list.max_w(f32::INFINITY))),
        // beside the list as on Threads (threads.rs `page`)
        (false, _) => row().w_full().grow(1.0).min_h(0.0).child(list).child(row().w(0.0).grow(3.0).min_w(0.0).h_full().child(right)),
    }
}

/// The thread With you for agent NAME of project P (PI) draws whole beside its list, every agent's posts in it: the one
/// he picked, else, not narrow and "+ New thread" not chosen, the first he would look at (`outline::first`); and
/// whether "+ New thread" is chosen (the box alone, saying so). texts.rs `page_wanted` reads its words by it.
pub(crate) fn beside<'a>(s: &App, p: &'a crate::Proj, pi: usize, name: &str) -> (Option<&'a TicketView>, bool) {
    let key = (pi, name.to_string());
    let narrow = s.narrow.get() || !s.panes.is_empty();
    let inside = crate::threads::picked(s, pi, Some(name)).and_then(|n| p.live.tickets.get(&n));
    let new = inside.is_none() && s.withyou.on.get(&key) == Some(&None);
    (inside.or_else(|| if narrow || new { None } else { crate::outline::first(p, name).and_then(|n| p.live.tickets.get(&n)) }), new)
}

/// Whether agent NAME of project PI shows its list of threads now: its chat on With you, not its terminal, and not
/// narrowed to one thread open (find.rs puts the field's focus there, else opens the box over the page); not while its
/// session shows in With you's place ([`session_instead`]).
pub fn lists(s: &App, pi: usize, name: &str) -> bool {
    let key = (pi, name.to_string());
    s.streams.which.get(&key).copied().unwrap_or_default() == crate::streams::Which::You && !s.term.pages.contains(&key) && list_shown(s, pi, name) && !s.projects.get(pi).is_some_and(|p| session_instead(p, name))
}

/// Whether With you for agent NAME of project P shows its session as the chat in its place (chat.rs `pane`, as its
/// › on a running turn's line opens it): no ticket with NAME at all yet, archived or not (outline.rs `is_with`), and
/// its session holds an exchange, the words it was started with and its answer. Its first exchange is on its page,
/// not "Nothing between you and NAME yet" (paper-research N-1446 p3041, 2; proto-window's walk p3038, 2). His first
/// message from its box opens a thread, and With you is his threads from then on.
pub(crate) fn session_instead(p: &crate::Proj, name: &str) -> bool {
    p.convs.get(name).is_some_and(|c| !c.entries.is_empty()) && !p.live.tickets.values().any(|v| crate::outline::is_with(v, name))
}

/// Whether With you for agent NAME of project PI draws its list: not narrowed to one thread open (or to the box of a
/// new one). On the agent's page [`lists`] asks it once its chat shows With you; a With you lens always does (find.rs
/// `has_list`).
pub(crate) fn list_shown(s: &App, pi: usize, name: &str) -> bool {
    let key = (pi, name.to_string());
    let narrow = s.narrow.get() || !s.panes.is_empty();
    let open = crate::threads::picked(s, pi, Some(name)).is_some() || s.withyou.on.get(&key) == Some(&None);
    !(narrow && open)
}

/// Thread V in the Threads page's reading pane (threads.rs `reading`; item 15, paper-research N-1446 p2603): its
/// column as wide, centred, the said line above it, opened where he last read it, else at its foot; its rows in view
/// only, so a key in its box builds the box and those rows, not the whole thread (N-1272). Its scroll keeps its own
/// key, `with-you-thread-ID`. Also a lens's thread in a pane that is not focused (lens.rs `apart`).
pub(crate) fn whole(s: &App, p: &crate::Proj, v: &TicketView, k: &Kit) -> El {
    row().w_full().grow(1.0).min_h(0.0).child(crate::threads::reading(s, p, v, format!("with-you-thread-{}", v.id), k))
}

/// A still's state (lib.rs `read_once`): YAAIHH_WITH_YOU_OPEN=ID shows that thread beside the list,
/// YAAIHH_WITH_YOU_ON=ID binds the box to it, YAAIHH_RETITLE=ID opens its label as a field, YAAIHH_WITH_YOU_SEEN=AT
/// marks what he had seen by AT; YAAIHH_OUTLINE_OPEN unfolds topics, as on Threads.
pub fn scene(s: &mut App, pi: usize, name: &str) {
    let find = |s: &App, id: &str| s.projects.get(pi)?.live.tickets.values().find(|v| v.id == id).map(|v| v.n);
    if let Some(n) = std::env::var("YAAIHH_WITH_YOU_OPEN").ok().and_then(|id| find(s, &id)) {
        s.withyou.inside.insert((pi, name.to_string()), n);
    }
    crate::threads::retitle_scene(s, pi);
    if let Ok(id) = std::env::var("YAAIHH_WITH_YOU_ON") {
        bind(s, pi, name, &id);
    }
    // YAAIHH_WITH_YOU_SEEN=AT: he last looked at AT (a still from a record: what he has seen is the window's memory,
    // not in the record), so each thread is seen up to its last post before AT
    if let (Ok(at), Some(p)) = (std::env::var("YAAIHH_WITH_YOU_SEEN"), s.projects.get(pi)) {
        let upto: Vec<(String, u32)> = p.live.tickets.values().map(|v| (v.id.clone(), v.posts.iter().filter(|x| x.at.to_string() < at).map(|x| x.n).max().unwrap_or(0))).collect();
        for (id, n) in upto {
            s.tickets.seen.upto.insert((pi, id), n);
        }
    }
    for id in std::env::var("YAAIHH_OUTLINE_OPEN").unwrap_or_default().split(',').filter(|x| !x.is_empty()) {
        s.streams.open.insert(format!("outline|{pi}|{id}"));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::{App, Sel};
    use serde_json::json;

    /// pr's page in a window of project [`research`].
    fn app() -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research()];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        (held, rt)
    }

    /// A project with agent pr and his threads with it: N-7 asks him something (an open ask told him), N-8 is
    /// answered and has subthread N-9 split off it; N-10 is with qa; N-11 ended this morning.
    pub(crate) fn research() -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "pr", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let thread = |n: u64, posts: serde_json::Value, extra: serde_json::Value| -> coord_types::TicketView {
            let mut v = json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "pr"},
                "title": format!("thread {n}"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-19T08:00:00Z", "given": [], "posts": posts});
            v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            serde_json::from_value(v).unwrap()
        };
        let o = |n: u32, at: &str| json!({"n": n, "at": at, "by": "owner", "text": "is it in?", "names": ["pr"], "told": ["pr"]});
        let a = |n: u32, at: &str, asks: bool| json!({"n": n, "at": at, "by": "pr", "text": "Yes. Shall I go on?", "names": [], "told": ["owner"],
            "asks": if asks { json!([{"at": [5, 19]}]) } else { json!([]) }, "asks_open": asks});
        p.live.tickets.insert(7, thread(7, json!([o(1, "2026-11-19T08:00:00Z"), a(2, "2026-11-19T08:01:00Z", true)]), json!({})));
        p.live.tickets.insert(8, thread(8, json!([o(1, "2026-11-19T07:00:00Z"), a(2, "2026-11-19T07:01:00Z", false)]), json!({})));
        p.live.tickets.insert(9, thread(9, json!([]), json!({"under": "N-8", "opener": "pr", "form": {}})));
        p.live.tickets.insert(10, thread(10, json!([]), json!({"form": {"to": "qa"}})));
        p.live.tickets.insert(11, thread(11, json!([o(1, "2026-11-19T06:00:00Z")]), json!({"closed_at": "2026-11-19T06:30:00Z"})));
        p
    }

    fn click(rt: &mut rune::runtime::Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is drawn"));
        click_at(rt, x, y);
    }

    /// A press on the element whose words are T (a menu's item), as [`click`] presses a key.
    fn click_text(rt: &mut rune::runtime::Runtime<App>, t: &str) {
        let r = rt.elements().into_iter().find(|e| e.text.as_deref() == Some(t)).map(|e| e.rect).unwrap_or_else(|| panic!("{t} is drawn"));
        click_at(rt, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    }

    fn click_at(rt: &mut rune::runtime::Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        rt.frame(0.016);
    }

    /// p3041, 2 (proto-window's walk p3038, 2): pr, started by New agent, was given its words and answered them, and
    /// there is no thread between him and it yet. Its With you is its session as the chat (the words it was started
    /// with, then its answer) with its box under them, not "Nothing between you and pr yet". Once a thread with it is
    /// in, With you is his threads again.
    #[test]
    fn with_no_thread_yet_an_agents_with_you_is_its_session_first_exchange_included() {
        const STARTED: &str = "You are a standing agent: wait for messages.";
        const ANSWER: &str = "Up and waiting for a task.";
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = research();
        let threads = std::mem::take(&mut p.live.tickets);
        let entry = |offset: u64, kind: &str, t: &str| -> coord_types::Entry { serde_json::from_value(json!({"offset": offset, "at": "2026-11-19T08:00:00Z", "kind": kind, "text": t})).unwrap() };
        // merged as a page from coordd is (live.rs `Conv::merge`, which keys its rows)
        p.convs.entry("pr".into()).or_default().merge(coord_types::Transcript { entries: vec![entry(0, "user", STARTED), entry(10, "assistant", ANSWER)], start: 0, next: 20, session: "s-1".into(), ..Default::default() }, false);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        rt.frame(0.016);
        let says = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().any(|e| !e.exiting && e.text.as_deref().is_some_and(|x| x.contains(t)));
        assert!(says(&rt, STARTED) && says(&rt, ANSWER), "the words it was started with and its answer");
        assert!(!says(&rt, "Nothing between you"), "no thread yet is not nothing between them");
        assert!(rt.find("conv-0-pr").is_some(), "its session's chat");
        assert!(!crate::withyou::lists(&rt.state, 0, "pr"), "no list for the search field to go to");
        assert!(rt.elements().iter().any(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("composer-0-"))), "its box under it");
        rt.update(|s, _| s.projects[0].live.tickets = threads);
        rt.frame(0.016);
        rt.frame(0.016);
        assert!(rt.find("conv-0-pr").is_none() && !says(&rt, STARTED), "a thread with it in: his threads again");
        assert!(crate::withyou::lists(&rt.state, 0, "pr"));
    }

    /// A link pressed inside a thread opens where it was pressed (his N-977 p37: a link took him off his agent's page
    /// onto Threads; item 15, paper-research N-1446 p2603): on pr's With you, N-12's "continues N-8 ›" opens N-8 beside
    /// the list, pr's page kept; N-13's "continues N-10 ›", a thread not in pr's list (qa's), opens on Threads.
    #[test]
    fn a_link_in_a_thread_opens_where_it_was_pressed() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = research();
        for (n, follows) in [(12, "N-8"), (13, "N-10")] {
            let mut v = p.live.tickets[&8].clone();
            (v.id, v.n, v.title) = (format!("N-{n}"), n, format!("thread {n}"));
            v.form = json!({"to": "pr", "follows": follows});
            p.live.tickets.insert(n, v);
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        s.withyou.inside.insert((0, "pr".into()), 12);
        let mut rt = crate::view_cost::window(s);
        for _ in 0..4 {
            rt.frame(0.016);
        }
        click_text(&mut rt, "continues N-8 ›");
        let pr = (0, "pr".to_string());
        assert_eq!((rt.state.sel.clone(), rt.state.withyou.inside.get(&pr).copied()), (Sel::Agent("pr".into()), Some(8)), "beside pr's list, on pr's page");
        rt.update(|s, _| {
            s.withyou.inside.insert((0, "pr".into()), 13);
        });
        for _ in 0..4 {
            rt.frame(0.016);
        }
        click_text(&mut rt, "continues N-10 ›");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (Sel::Tickets, Some(10)), "not in pr's list: on Threads");
    }

    /// One reading pane on both pages (item 15, his N-944 p1; paper-research N-1446 p2603 (a)): N-20, 40 posts with pr,
    /// reads in a column of one width on Threads and beside pr's list, centred in the pane it is given, and opens at its
    /// foot, never read. Read up to its middle beside pr's list, it opens there on Threads, and beside the list again
    /// there too; left at its foot, it opens at its foot.
    #[test]
    fn a_thread_reads_in_one_pane_on_threads_and_with_you() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = research();
        let mut v = p.live.tickets[&8].clone();
        (v.id, v.n, v.title) = ("N-20".into(), 20, "thread 20".into());
        let posts: Vec<serde_json::Value> = (1..=40u32)
            .map(|n| {
                json!({"n": n, "at": format!("2026-11-19T09:{n:02}:00Z"), "by": if n % 2 == 1 { "owner" } else { "pr" }, "names": [], "told": ["pr"],
                    "text": format!("p{n}: a post of the made-up thread, long enough to take two lines in the reading column, so that forty of them run well past one screen.")})
            })
            .collect();
        v.posts = serde_json::from_value(json!(posts)).unwrap();
        p.live.tickets.insert(20, v);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Tickets;
        s.tickets.shown = Some(20);
        s.withyou.inside.insert((0, "pr".into()), 20);
        let mut rt = crate::view_cost::window(s);
        // the places and the scroll back, as the window's poll runs them (lib.rs `poll`)
        fn poll(s: &mut App, _: f64, tx: &mut rune::Tx) -> bool {
            let made = crate::history::poll(s);
            crate::read_place::poll(s, tx);
            made
        }
        rt.poll = Some(poll);
        let frames = |rt: &mut rune::runtime::Runtime<App>| {
            for _ in 0..30 {
                rt.invalidate();
                rt.frame(0.016);
            }
        };
        let rect = |rt: &rune::runtime::Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
        // post N's column: its width, and how far its centre is from the pane's
        let column = |rt: &rune::runtime::Runtime<App>, n: u32| {
            let post = rect(rt, &format!("hover-N-20-{n}")).unwrap_or_else(|| panic!("post {n} drawn"));
            let pane = rect(rt, "reading-N-20").expect("the reading pane");
            (post[2], (post[0] + post[2] / 2.0 - (pane[0] + pane[2] / 2.0)).abs())
        };
        // row KEY's top, from the top of the scroll SCROLL; None when it is not in sight
        let from_top = |rt: &rune::runtime::Runtime<App>, scroll: &str, key: &str| match (rect(rt, key), rect(rt, scroll)) {
            (Some(r), Some(v)) if r[1] < v[1] + v[3] && r[1] + r[3] > v[1] => Some(r[1] - v[1]),
            _ => None,
        };
        let (threads, beside) = ("ticket-detail", "with-you-thread-N-20");
        let (first, last) = ("post-N-20-1", "post-N-20-40");
        let to = |rt: &mut rune::runtime::Runtime<App>, sel: Sel| rt.update(move |s, _| s.sel = sel);
        frames(&mut rt);
        assert!(from_top(&rt, threads, last).is_some() && from_top(&rt, threads, first).is_none(), "Threads: at its foot, never read");
        let on_threads = column(&rt, 40);
        to(&mut rt, Sel::Agent("pr".into()));
        frames(&mut rt);
        assert!(from_top(&rt, beside, last).is_some() && from_top(&rt, beside, first).is_none(), "With you: at its foot");
        let with_you = column(&rt, 40);
        assert!(on_threads.0 <= crate::threads::READING_W - 56.0 + 0.5 && (on_threads.0 - with_you.0).abs() < 0.5, "one width: Threads {on_threads:?}, With you {with_you:?}");
        assert!(on_threads.1 < 1.0 && with_you.1 < 1.0, "centred in its pane: Threads {on_threads:?}, With you {with_you:?}");
        // read up to its middle beside pr's list
        let r = rect(&rt, beside).expect("With you's scroll");
        rt.wheel(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, 2400.0);
        frames(&mut rt);
        let view = crate::read_place::thread_view("research", "N-20");
        let read = rt.state.read_places.kept.get(&view).cloned().unwrap_or_default();
        assert!(read.starts_with("post-N-20-") && from_top(&rt, beside, last).is_none(), "read up to {read:?}");
        let at_top = |rt: &rune::runtime::Runtime<App>, scroll: &str| from_top(rt, scroll, &read).is_some_and(|y| y.abs() < 4.0);
        to(&mut rt, Sel::Tickets);
        frames(&mut rt);
        assert!(at_top(&rt, threads), "on Threads where he read it: {read} at {:?}", from_top(&rt, threads, &read));
        to(&mut rt, Sel::Agent("pr".into()));
        frames(&mut rt);
        assert!(at_top(&rt, beside), "beside the list again where he read it: {read} at {:?}", from_top(&rt, beside, &read));
        // read to its foot: it opens there
        let r = rect(&rt, beside).expect("With you's scroll");
        rt.wheel(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, -100_000.0);
        frames(&mut rt);
        assert_eq!(rt.state.read_places.kept.get(&view).map(String::as_str), Some(""), "kept at its foot");
        to(&mut rt, Sel::Tickets);
        frames(&mut rt);
        assert!(from_top(&rt, threads, last).is_some(), "on Threads at its foot");
    }

    /// One list column on both pages (item 15; paper-research N-1446 p2603 (b), (c)): the same frame beside the
    /// reading pane, Outline | By time and "+ New thread" on its top; by time, pr's threads only, a press showing one
    /// beside the list; at the foot of pr's list the threads archived with pr only, and the Archive press on Threads
    /// only.
    #[test]
    fn the_list_is_one_column_on_threads_and_with_you() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = research();
        // archived: N-12 with pr, N-13 with qa
        for (n, like) in [(12, 8), (13, 10)] {
            let mut v = p.live.tickets[&like].clone();
            (v.id, v.n, v.title, v.archived) = (format!("N-{n}"), n, format!("thread {n}"), true);
            p.live.tickets.insert(n, v);
        }
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        let frames = |rt: &mut rune::runtime::Runtime<App>| {
            for _ in 0..6 {
                rt.frame(0.016);
            }
        };
        let rect = |rt: &rune::runtime::Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect);
        let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect::<Vec<String>>();
        let head = |t: &[String]| ["Outline", "By time", "+ New thread"].iter().all(|w| t.iter().any(|x| x == w));
        frames(&mut rt);
        let on_threads = rect(&rt, "list-column").expect("Threads' list");
        let t = texts(&rt);
        assert!(head(&t), "Threads: Outline | By time, + New thread");
        assert!(rt.find("archive-all").is_some() && t.iter().any(|x| x == "▸ ARCHIVED · 2"), "Threads: both archived, and the Archive press");
        rt.update(|s, _| s.sel = Sel::Agent("pr".into()));
        frames(&mut rt);
        let beside_pr = rect(&rt, "list-column").expect("pr's list");
        assert!((on_threads[0] - beside_pr[0]).abs() < 0.5 && (on_threads[2] - beside_pr[2]).abs() < 0.5 && (250.0..=340.0).contains(&beside_pr[2]), "one frame: Threads {on_threads:?}, pr {beside_pr:?}");
        let t = texts(&rt);
        assert!(head(&t), "pr: Outline | By time, + New thread");
        assert!(rt.find("archive-all").is_none() && t.iter().any(|x| x == "▸ ARCHIVED · 1"), "pr: the one archived with pr, no Archive press");
        click_text(&mut rt, "By time");
        assert!(rt.find("ticket-N-7").is_some() && rt.find("ticket-N-8").is_some() && rt.find("ticket-N-10").is_none(), "by time, pr's threads only");
        click(&mut rt, "ticket-N-8");
        assert_eq!((rt.state.sel.clone(), rt.state.withyou.inside.get(&(0, "pr".to_string())).copied()), (Sel::Agent("pr".into()), Some(8)), "shown beside pr's list");
    }

    /// "+ New thread" and the way back in a narrow window, one for both pages (item 15; p2603 (b)): on Threads it shows
    /// today's box to the project in the list's place, and "‹ Threads" brings the list back; beside pr's list the box
    /// to pr, and "‹ With you" brings the list back (before, the box stayed, "+ New thread" still chosen).
    #[test]
    fn new_thread_and_the_way_back_in_a_narrow_window() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research()];
        s.cur = 0;
        s.sel = Sel::Tickets;
        let mut rt = rune::runtime::Runtime::new(s, crate::view, (900.0, 900.0), rune::Env::new(false, 1.0));
        for _ in 0..4 {
            rt.frame(0.016);
        }
        let shows = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().any(|e| !e.exiting && e.text.as_deref() == Some(t));
        assert!(rt.state.narrow.get() && rt.find("ticket-list").is_some(), "narrow: the list alone");
        click(&mut rt, "new-thread");
        assert!(rt.find("ticket-list").is_none() && shows(&rt, "‹ Threads") && rt.state.tickets.shown.is_none(), "the box to the project in its place");
        click_text(&mut rt, "‹ Threads");
        assert!(rt.find("ticket-list").is_some() && !rt.state.tickets.new_box, "the list again");
        rt.update(|s, _| s.sel = Sel::Agent("pr".into()));
        for _ in 0..4 {
            rt.frame(0.016);
        }
        assert!(rt.find("with-you-list-0-pr").is_some(), "pr's list alone");
        click(&mut rt, "new-thread");
        assert!(rt.find("with-you-list-0-pr").is_none() && shows(&rt, "‹ With you"), "the box to pr in its place");
        click_text(&mut rt, "‹ With you");
        assert!(rt.find("with-you-list-0-pr").is_some(), "pr's list again");
    }

    /// The count of what waits on him on pr's page is the list its press opens (docket-edge8's walk of 73, N-1446
    /// p2353: "Waits on you · 7" over a list of 9 on paper-research's page): N-7's ask is pr's, N-12's is qa's in pr's
    /// topic; both are in With you's list, so the line under pr's head says 2, as the list's heading does (73: 1, what
    /// pr asked alone).
    #[test]
    fn the_count_on_an_agents_page_is_the_list_it_opens() {
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = research();
        let mut n12 = p.live.tickets[&10].clone();
        (n12.id, n12.n, n12.title) = ("N-12".into(), 12, "thread 12".into());
        n12.form = json!({"to": "pr"});
        n12.posts = serde_json::from_value(json!([{"n": 1, "at": "2026-11-19T08:05:00Z", "by": "qa", "text": "Which one goes first?", "names": [], "told": ["owner"],
            "asks": [{"at": [0, 21]}], "asks_open": true}])).unwrap();
        p.live.tickets.insert(12, n12);
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        let mut rt = crate::view_cost::window(s);
        for _ in 0..4 {
            rt.frame(0.016);
        }
        assert!(rt.find("need-N-7.p2").is_some() && rt.find("need-N-12.p1").is_some(), "both in With you's list");
        let counts: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).filter(|t| t.starts_with("Waits on you")).collect();
        assert_eq!(counts, ["Waits on you · 2", "Waits on you · 2"], "the line under pr's head and the list's heading");
    }

    /// With you is the Threads page's list narrowed to pr: what waits on him first, pr's topics, not qa's; the one
    /// that waits on him shown whole beside it; no second way to draw it.
    #[test]
    fn with_you_is_the_threads_list_narrowed_to_the_agent() {
        let (_held, mut rt) = app();
        assert!(rt.find("need-N-7.p2").is_some(), "what waits on him, as on Threads");
        assert!(rt.find("goal-N-7").is_some() && rt.find("goal-N-8").is_some(), "pr's topics");
        assert!(rt.find("goal-N-10").is_none(), "qa's thread is not in pr's list");
        assert!(rt.find("post-N-7-2").is_some(), "the one that waits on him shown whole");
        assert!(rt.find("with-you-how-Blocks").is_none() && rt.find("with-you-block-N-7").is_none(), "one drawing");
        click(&mut rt, "goal-N-8");
        assert_eq!(rt.state.withyou.inside.get(&(0, "pr".to_string())), Some(&8), "a press shows it beside the list");
        assert!(rt.find("post-N-8-2").is_some());
    }

    /// His walk items 6 and 15 (yaaihh/fresh N-1724 p22; paper-research N-1751): pr's errands with other agents alone
    /// stand behind one row, Between agents, folded, and his talk with pr is the page. N-12 is between pr and qa (pr
    /// opened it, they wrote to each other, nothing there names him) and at work, waiting on qa: before, it stood under
    /// All active beside his N-7 and N-8, and with nothing waiting on him With you opened on it. Now With you opens on
    /// his newest, N-8; N-12 is behind "Between agents · 1" in the outline and in the list by time, and a press on the
    /// row shows it; picked (as a search's hit opens it), its row shows by itself. N-9, split off his N-8 by pr, goes with
    /// N-8 in both lists.
    #[test]
    fn an_agents_errands_with_other_agents_stand_behind_one_row() {
        let (_held, mut s) = crate::view_cost::test_app();
        let p = errand();
        assert_eq!(crate::outline::first(&p, "pr"), Some(8), "his newest, not pr's errand with qa");
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        let mut rt = crate::view_cost::window(s);
        for _ in 0..4 {
            rt.frame(0.016);
        }
        let shows = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().any(|e| !e.exiting && e.text.as_deref().is_some_and(|x| x.starts_with(t)));
        assert!(rt.find("goal-N-7").is_some() && rt.find("goal-N-8").is_some(), "his threads with pr are the list");
        assert!(rt.find("post-N-8-2").is_some(), "N-8 beside it");
        assert!(rt.find("to-N-8-2").is_none() && opacity(&rt, "said-N-8-2") == Some(1.0), "pr's answer to him: no addressee, at full strength");
        assert!(rt.find("goal-N-12").is_none() && shows(&rt, "Between agents · 1"), "N-12 behind one row");
        click(&mut rt, "group-between-agents");
        assert!(rt.find("goal-N-12").is_some(), "a press on the row shows it");
        click(&mut rt, "group-between-agents");
        assert!(rt.find("goal-N-12").is_none(), "and folds it again");
        click_text(&mut rt, "By time");
        assert!(rt.find("ticket-N-7").is_some() && rt.find("ticket-N-8").is_some() && rt.find("ticket-N-12").is_none(), "by time, his threads; N-12 behind the row");
        let list: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("with-you-list-0-pr")).filter_map(|e| e.text).collect();
        assert!(shows(&rt, "▸ BETWEEN AGENTS · 1 thread,") && list.iter().any(|t| t == "thread 8"), "the row by time, N-9 split off his N-8 not behind it: {list:?}");
        click_text(&mut rt, "Outline");
        rt.update(|s, _| crate::threads::set_picked(s, 0, Some("pr"), Some(12)));
        for _ in 0..2 {
            rt.frame(0.016);
        }
        assert!(rt.find("goal-N-12").is_some() && rt.find("post-N-12-1").is_some(), "picked, its row shows and it is beside the list");
        // on its page, each post to an agent says whom beside its sender, and is drawn quieter than his talk
        assert_eq!(text_of(&rt, "to-N-12-1").as_deref(), Some("to qa"));
        assert_eq!(text_of(&rt, "to-N-12-2").as_deref(), Some("to pr"));
        assert!(opacity(&rt, "said-N-12-1").is_some_and(|o| o < 0.9) && opacity(&rt, "said-N-12-2").is_some_and(|o| o < 0.9), "quieter");
    }

    /// The research project with N-12 between pr and qa alone (pr opened it, they wrote to each other, nothing there
    /// names him, though the norms told him) and at work, waiting on qa, and nothing waiting on him (pr's ask on N-7
    /// answered).
    fn errand() -> crate::Proj {
        let mut p = research();
        if let Some(x) = p.live.tickets.get_mut(&7).and_then(|v| v.posts.get_mut(1)) {
            x.asks_open = false;
        }
        let mut n12 = p.live.tickets[&10].clone();
        (n12.id, n12.n, n12.title, n12.opener) = ("N-12".into(), 12, "pr and qa: the log".into(), Some("pr".into()));
        n12.form = json!({"to": "qa"});
        n12.posts = serde_json::from_value(json!([
            {"n": 1, "at": "2026-11-19T09:00:00Z", "by": "pr", "text": "qa, the log of last night's run?", "names": ["qa"], "told": ["qa", "owner"]},
            {"n": 2, "at": "2026-11-19T09:05:00Z", "by": "qa", "text": "Here, the tail of it.", "names": ["pr"], "told": ["pr", "owner"]}
        ]))
        .unwrap();
        n12.waits_on = serde_json::from_value(json!([{"ticket": "N-12", "agent": "qa", "call": "answer"}])).unwrap();
        p.live.tickets.insert(12, n12);
        p
    }

    /// The opacity of the element keyed KEY, as drawn.
    fn opacity(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<f32> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.opacity)
    }

    /// The words of the element keyed KEY.
    fn text_of(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<String> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).and_then(|e| e.text)
    }

    /// On Threads too his threads are the list (yaaihh/fresh's Go, N-1751 p2): a thread is his if he opened it, wrote on
    /// it, or a post there names him; whom the norms told decides nothing. N-12, pr's errand with qa, stands behind
    /// "Between agents · 1" in the outline and by time, not under All active nor YOURS, and the left bar's Threads
    /// counts his three topics, N-7, N-8 and N-10, not four; its Structure lists those three, N-12 behind one row,
    /// folded (fresh N-1751 p6).
    #[test]
    fn on_threads_the_talk_between_agents_stands_behind_one_row_and_is_not_counted() {
        let (_held, mut s) = crate::view_cost::test_app();
        let p = errand();
        assert_eq!(crate::outline::active(&s, &p), 3, "N-7, N-8, N-10: his");
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        for _ in 0..4 {
            rt.frame(0.016);
        }
        let shows = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().any(|e| !e.exiting && e.text.as_deref().is_some_and(|x| x.starts_with(t)));
        assert!(rt.find("goal-N-7").is_some() && rt.find("goal-N-8").is_some() && rt.find("goal-N-10").is_some(), "his threads are the list");
        assert!(rt.find("goal-N-12").is_none() && shows(&rt, "Between agents · 1"), "N-12 behind one row");
        let tree = ["tree-N-7", "tree-N-8", "tree-N-10", "tree-between-agents"];
        assert!(tree.iter().all(|t| rt.find(t).is_some()) && rt.find("tree-N-12").is_none(), "the left bar's Structure: his threads, N-12 behind one row");
        click(&mut rt, "tree-fold-between-agents");
        assert!(rt.find("tree-N-12").is_some(), "unfolded, N-12 under it");
        click(&mut rt, "tree-fold-between-agents");
        assert!(rt.find("tree-N-12").is_none(), "folded again");
        click(&mut rt, "group-between-agents");
        assert!(rt.find("goal-N-12").is_some(), "a press on the row shows it");
        click(&mut rt, "group-between-agents");
        click_text(&mut rt, "By time");
        assert!(rt.find("ticket-N-8").is_some() && rt.find("ticket-N-12").is_none() && shows(&rt, "▸ BETWEEN AGENTS · 1 thread,"), "by time, N-12 behind the row, not under YOURS");
    }

    /// A window ended with no exit (release.sh's `taskkill /F`) starts again on the thread he had beside the list, not
    /// the list's first. ui.json kept only the page, so With you showed its first thread in place of his (his "i keep
    /// going into some other chat"; his frames.jsonl: N-1104 beside the list before the 19:50 restart, N-1103 after;
    /// paper-research, N-1396 p322/p326). Only the poll's own save (state.rs `keep`) writes it, and frames with no
    /// press keep it, from before the project's snapshot is in to after.
    #[test]
    fn a_window_ended_with_no_exit_starts_on_the_thread_he_had_beside_the_list() {
        let (_held, mut rt) = app();
        // ui.json in a folder of this test's own, and YAAIHH_CONFIG given back after
        let dir = std::env::temp_dir().join(format!("yaaihh-with-you-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let was = std::env::var_os("YAAIHH_CONFIG");
        // SAFETY: under the tests' lock (`app` holds it), which every test that reads or sets it holds
        unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
        // the window's own save, as its poll runs it (lib.rs `poll`)
        fn keep(s: &mut App, _: f64, tx: &mut rune::Tx) -> bool {
            crate::state::keep(s, tx);
            false
        }
        rt.poll = Some(keep);
        assert!(rt.find("post-N-7-2").is_some(), "the list's first thread, before he picks one");
        // two changes inside the save's half second: N-7 picked, then N-8 (consult-3, N-1396 p323, 2)
        click(&mut rt, "goal-N-7");
        click(&mut rt, "goal-N-8");
        assert!(rt.find("post-N-8-2").is_some(), "N-8 beside the list");
        // half a second with nothing changed, and the poll writes what is current then
        let file = dir.join("ui.json");
        for _ in 0..40 {
            if file.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            rt.frame(0.016);
        }
        assert!(file.exists(), "written by the poll, with no exit");
        // ended here: nothing of the window runs again, nothing more is written
        drop(rt);
        // the next window: its project connects with no tickets yet, and its snapshot comes after the first frames
        let mut n = crate::init();
        let mut p = research();
        let tickets: Vec<coord_types::TicketView> = std::mem::take(&mut p.live.tickets).into_values().collect();
        n.projects = vec![p];
        crate::state::restore(&mut n);
        assert_eq!((n.cur, &n.sel), (0, &Sel::Agent("pr".into())), "his page");
        assert_eq!(n.withyou.inside.get(&(0, "pr".to_string())), Some(&8), "the thread he had beside the list");
        let mut rt = crate::view_cost::window(n);
        for _ in 0..10 {
            rt.frame(0.016);
        }
        snapshot(&mut rt.state, tickets);
        rt.invalidate();
        for _ in 0..20 {
            rt.frame(0.016);
        }
        assert!(rt.find("post-N-8-2").is_some() && rt.find("post-N-7-2").is_none(), "N-8 beside the list, not the list's first");
        assert_eq!(rt.state.withyou.inside.get(&(0, "pr".to_string())), Some(&8), "kept with no press");
        // SAFETY: as above
        match was {
            Some(v) => unsafe { std::env::set_var("YAAIHH_CONFIG", v) },
            None => unsafe { std::env::remove_var("YAAIHH_CONFIG") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Project 0's snapshot, its agents as held and TICKETS, taken as the window takes one from coordd (lib.rs `take`).
    fn snapshot(s: &mut App, tickets: Vec<coord_types::TicketView>) {
        let agents = s.projects[0].live.agents.clone();
        let snap = coord_types::Snapshot { agents, tickets, ..Default::default() };
        s.take(0, crate::link::Incoming::Event(coord_types::Event { seq: 1, body: coord_types::Body::Snapshot(snap) }));
    }

    /// What ui.json gave back and no longer stands is dropped at the project's first snapshot, with nothing shown to
    /// him (consult-3, N-1396 p331). His words to pr were to go on N-8, which has closed since. The thread beside
    /// pr's list was N-10, which is qa's now. Agent old has been retired since, with a thread beside its list and its
    /// Work view. pr's page starts plain: the list's first beside it, and his words go as with nothing chosen.
    #[test]
    fn picks_that_no_longer_stand_start_plain_at_the_first_snapshot() {
        let (_held, mut s) = crate::view_cost::test_app();
        // ui.json in a folder of this test's own, and YAAIHH_CONFIG given back after
        let dir = std::env::temp_dir().join(format!("yaaihh-with-you-checked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let was = std::env::var_os("YAAIHH_CONFIG");
        // SAFETY: under the tests' lock (`test_app` holds it), which every test that reads or sets it holds
        unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
        s.projects = vec![research()];
        (s.cur, s.sel) = (0, Sel::Agent("pr".into()));
        let key = |a: &str| (0, a.to_string());
        s.withyou.inside.insert(key("pr"), 10);
        s.withyou.on.insert(key("pr"), Some("N-8".into()));
        s.withyou.inside.insert(key("old"), 7);
        s.streams.which.insert(key("old"), crate::streams::Which::Work);
        assert!(crate::state::write_now(&mut s));
        // the next window, before its project's snapshot: all of it as written
        let mut n = crate::init();
        let mut p = research();
        let mut tickets: Vec<coord_types::TicketView> = std::mem::take(&mut p.live.tickets).into_values().collect();
        n.projects = vec![p];
        crate::state::restore(&mut n);
        assert_eq!((n.withyou.inside.get(&key("pr")), n.withyou.on.get(&key("pr"))), (Some(&10), Some(&Some("N-8".to_string()))));
        // the snapshot: N-8 has closed, and old is not among the agents
        let n8 = tickets.iter_mut().find(|v| v.n == 8).unwrap();
        let mut j = serde_json::to_value(&*n8).unwrap();
        j["closed_at"] = json!("2026-11-19T09:00:00Z");
        *n8 = serde_json::from_value(j).unwrap();
        snapshot(&mut n, tickets);
        assert_eq!(n.withyou.inside.get(&key("pr")), None, "N-10 is not in pr's list: the list's first takes its place");
        assert_eq!(n.withyou.on.get(&key("pr")), None, "N-8 has closed: his words go as with nothing chosen");
        assert!(n.withyou.inside.get(&key("old")).is_none() && n.streams.which.get(&key("old")).is_none(), "a retired agent's picks are left out");
        assert!(n.withyou.restored.is_empty(), "checked once");
        let mut rt = crate::view_cost::window(n);
        rt.frame(0.016);
        assert!(rt.find("post-N-7-2").is_some(), "pr's page starts plain, on the list's first");
        // SAFETY: as above
        match was {
            Some(v) => unsafe { std::env::set_var("YAAIHH_CONFIG", v) },
            None => unsafe { std::env::remove_var("YAAIHH_CONFIG") },
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A thread that ended today stays in the list, at its foot under `Ended today`; and what the agent is (its id,
    /// model, since) is one click away in Session (consult-3, N-412 post 1158).
    #[test]
    fn an_ended_thread_stays_at_the_foot_and_the_agents_facts_are_in_session() {
        let (_held, mut rt) = app();
        assert!(rt.find("group-Ended today").is_some() && rt.find("goal-N-11").is_some(), "the ended thread at the foot");
        // Session opened as its button opens it; a change from outside a handler is drawn once the view is invalidated
        rt.state.streams.open.insert("session|0|pr".into());
        rt.invalidate();
        rt.frame(0.016);
        rt.frame(0.016);
        assert!(rt.find("session-facts-pr").is_some(), "the facts behind Session: {:?}", rt.elements().iter().filter_map(|e| e.text.clone()).filter(|t| t == "It is" || t == "Model" || t == "Session").collect::<Vec<_>>());
    }

    /// A topic unfolds to its subthreads as on Threads; a press there shows it beside the list.
    #[test]
    fn a_topic_unfolds_to_its_subthreads_and_a_press_shows_one_beside() {
        let (_held, mut rt) = app();
        assert!(rt.find("piece-N-9").is_none());
        click(&mut rt, "fold-N-8");
        assert!(rt.find("piece-N-9").is_some(), "the subthread under N-8");
        click(&mut rt, "piece-N-9");
        assert_eq!(rt.state.withyou.inside.get(&(0, "pr".to_string())), Some(&9));
    }

    /// A click on the shown thread's label makes it a field; Esc leaves it as it was.
    #[test]
    fn a_threads_label_is_renamed_in_its_head_and_esc_leaves_it() {
        let (_held, mut rt) = app();
        click(&mut rt, "title-N-7");
        assert!(rt.state.tickets.retitle.contains_key(&7) && rt.find("retitle-N-7").is_some(), "the label is a field");
        rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
        rt.frame(0.016);
        assert!(!rt.state.tickets.retitle.contains_key(&7) && rt.find("title-N-7").is_some(), "Esc leaves it");
    }

    fn lens(filter: crate::lens::Filter, at: Option<&str>) -> Sel {
        Sel::Lens(crate::lens::Lens { filter, depth: crate::lens::Depth::Structure, at: at.map(str::to_string) })
    }

    /// Whether an element drawn holds WORDS.
    fn shows(rt: &rune::runtime::Runtime<App>, words: &str) -> bool {
        rt.elements().into_iter().any(|e| !e.exiting && e.text.is_some_and(|t| t.contains(words)))
    }

    fn with_pr(at: Option<&str>) -> Sel {
        lens(crate::lens::Filter::WithYou { agent: "pr".into() }, at)
    }

    /// A With you lens's thread is `withyou.inside` while it is shown and its `at` while it is not (lens.rs; proto-lead
    /// N-1446 p72): two panes on With you · pr hold two threads. A kept thread that no longer stands beside pr's list
    /// (N-10 is qa's) opens the lens at its list, the list's first beside it, as edge6-view's check does for ui.json.
    #[test]
    fn a_with_you_lens_keeps_its_thread_apart_and_one_that_no_longer_stands_opens_at_its_list() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research()];
        s.cur = 0;
        let pr = (0, "pr".to_string());
        s.show_sel(with_pr(Some("N-8")));
        assert_eq!((s.sel.clone(), s.withyou.inside.get(&pr)), (with_pr(None), Some(&8)), "N-8 beside the list; the lens on screen keeps no thread");
        assert_eq!(s.sel_kept(), with_pr(Some("N-8")), "what a pane keeps of it carries its thread");
        s.drop_sel(0, with_pr(Some("N-7")), 0, crate::panes::Zone::Right);
        assert_eq!((s.focus, s.withyou.inside.get(&pr)), (1, Some(&7)), "the same lens beside, at N-7");
        s.focus_pane(0);
        assert_eq!(s.withyou.inside.get(&pr), Some(&8), "the first pane back at N-8");
        assert_eq!(s.panes[1], (0, with_pr(Some("N-7"))), "the second keeps N-7");
        let mut rt = crate::view_cost::window(s);
        for _ in 0..4 {
            rt.frame(0.016);
        }
        assert!(rt.find("with-you-thread-N-7").is_some(), "the pane beside draws the thread it keeps");
        rt.update(|s, _| s.show_sel(with_pr(Some("N-10"))));
        assert_eq!(rt.state.withyou.inside.get(&pr), None, "qa's N-10 is not opened beside pr's list: the list's first is shown");
        rt.update(|s, _| s.show_sel(with_pr(Some("N-11"))));
        assert_eq!(rt.state.withyou.inside.get(&pr), Some(&11), "one ended today still stands in the list");
    }

    /// With you, a2o and a2a are drawn from pr's page, never a copy: With you's list and the thread beside it, pr's
    /// stream with him (his N-7 among it), pr's stream with the other agents (its post to qa on N-10). A search's words
    /// go into the field, over Threads' list or over pr's With you list.
    #[test]
    fn the_lenses_are_drawn_from_the_agents_page() {
        use crate::lens::Filter;
        let (_held, mut rt) = app();
        rt.update(|s, _| {
            let v = s.projects[0].live.tickets.get_mut(&10).unwrap();
            v.posts = serde_json::from_value(json!([{"n": 1, "at": "2026-11-19T09:00:00Z", "by": "pr", "text": "qa, the log", "names": ["qa"], "told": ["qa"]}])).unwrap();
            s.show_sel(with_pr(Some("N-8")));
        });
        rt.invalidate();
        for _ in 0..4 {
            rt.frame(0.016);
        }
        assert!(rt.find("with-you-list-0-pr").is_some() && rt.find("with-you-thread-N-8").is_some(), "With you: the list, N-8 beside it");
        assert!(rt.find("find-bar-sidebar").is_some() && rt.find("find-bar-pr").is_none(), "its search field: the sidebar's, over the list (N-1446 p2121)");
        for (f, drawn, not) in [(Filter::A2o { agent: "pr".into() }, "stream-0-pr-You", "stream-0-pr-Agents"), (Filter::A2a { agent: "pr".into() }, "stream-0-pr-Agents", "stream-0-pr-You")] {
            rt.update(|s, _| s.show_sel(lens(f.clone(), None)));
            rt.invalidate();
            for _ in 0..4 {
                rt.frame(0.016);
            }
            assert!(rt.find(drawn).is_some() && rt.find(not).is_none(), "{f:?}: {drawn}");
            assert!(rt.find("with-you-list-0-pr").is_none(), "{f:?}: no list");
        }
        assert!(shows(&rt, "qa, the log"), "a2a: pr's post to qa");
        rt.update(|s, _| s.show_sel(lens(Filter::A2o { agent: "pr".into() }, None)));
        rt.invalidate();
        rt.frame(0.016);
        assert!(!shows(&rt, "qa, the log"), "not among a2o");
        rt.update(|s, _| s.show_sel(lens(Filter::Find { words: "Shall I".into(), titles: false, agent: Some("pr".into()) }, None)));
        assert_eq!(rt.state.tickets.find.q.text, "Shall I", "the search's words in the field");
        assert!(crate::find::has_list(&rt.state), "over pr's With you list");
        rt.update(|s, _| s.show_sel(lens(Filter::Find { words: "brain".into(), titles: true, agent: None }, Some("N-7"))));
        assert!(rt.state.sel.shows_threads() && rt.state.tickets.shown == Some(7) && rt.state.tickets.find.titles, "over Threads' list, at its thread, titles only");
        rt.update(|s, _| s.show_sel(lens(Filter::A2a { agent: "gone".into() }, None)));
        rt.invalidate();
        rt.frame(0.016);
        assert!(shows(&rt, "gone is not an agent of this box"), "an agent no longer there says so");
    }

    /// The bar's menu offers pr's three lenses on pr's page, and the search in the field over a list; the lens shown is
    /// pinned as itself, at its root.
    #[test]
    fn the_bar_offers_the_lenses_of_the_page_shown_and_pins_them_at_their_root() {
        use crate::lens::{Filter, Lens};
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![research()];
        s.cur = 0;
        s.sel = Sel::Agent("pr".into());
        let pr = |f: fn(String) -> Filter| Lens::of(f("pr".into()));
        assert_eq!(crate::lens::offers(&s), vec![pr(|agent| Filter::WithYou { agent }), pr(|agent| Filter::A2o { agent }), pr(|agent| Filter::A2a { agent })]);
        s.tickets.find.q = rune::TextEdit::new("brain");
        assert_eq!(crate::lens::offers(&s).last(), Some(&Lens::of(Filter::Find { words: "brain".into(), titles: false, agent: Some("pr".into()) })), "a search within pr's list");
        s.sel = Sel::Tickets;
        assert_eq!(crate::lens::offers(&s), vec![Lens::of(Filter::Find { words: "brain".into(), titles: false, agent: None })], "a search over Threads' list");
        s.sel = Sel::Files;
        assert!(crate::lens::offers(&s).is_empty(), "no list: no search to pin");
        s.show_sel(with_pr(Some("N-8")));
        let pin = s.shown_pin().unwrap();
        assert_eq!((pin.agent.as_str(), pin.lens.clone()), ("With you · pr", Some(Lens::of(Filter::WithYou { agent: "pr".into() }))), "pinned at its root");
        s.pins.list.clear();
        s.pins.toggle(pin.clone());
        assert!(s.pins.has(&s.lens_pin(&Lens::of(Filter::WithYou { agent: "pr".into() })).unwrap()), "pinned");
        s.pins.toggle(pin);
        assert!(s.pins.list.is_empty(), "and unpinned");
    }

    /// His press opens what it names (paper-research N-1446 p198, item 12 of his visual asks): on pr's page the bar's ⋯
    /// offers pr's lenses; a press on "Pin With you · pr" puts its row in Pinned and closes the menu, and a press on that
    /// row from another page shows With you · pr, drawn from pr's page. The same for pr ↔ you and pr ↔ agents.
    #[test]
    fn a_lens_pinned_from_the_bars_menu_opens_from_its_row() {
        use crate::lens::{Filter, Lens};
        let (_held, mut rt) = app();
        rt.update(|s, _| {
            s.pins.list.clear();
            let v = s.projects[0].live.tickets.get_mut(&10).unwrap();
            v.posts = serde_json::from_value(json!([{"n": 1, "at": "2026-11-19T09:00:00Z", "by": "pr", "text": "qa, the log", "names": ["qa"], "told": ["qa"]}])).unwrap();
        });
        let pr = "pr".to_string();
        for (name, f, drawn) in [
            ("With you · pr", Filter::WithYou { agent: pr.clone() }, "with-you-list-0-pr"),
            ("pr ↔ you", Filter::A2o { agent: pr.clone() }, "stream-0-pr-You"),
            ("pr ↔ agents", Filter::A2a { agent: pr.clone() }, "stream-0-pr-Agents"),
        ] {
            rt.update(|s, _| (s.sel, s.bar_menu) = (Sel::Agent("pr".into()), true));
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
            click_text(&mut rt, &format!("Pin {name}"));
            assert!(!rt.state.bar_menu, "{name}: the menu closes");
            rt.update(|s, _| s.sel = Sel::Files);
            rt.invalidate();
            for _ in 0..3 {
                rt.frame(0.016);
            }
            click(&mut rt, &format!("pin-research-lens-{}", f.key()));
            assert_eq!(rt.state.sel, Sel::Lens(Lens::of(f.clone())), "{name}: shown");
            // the page's rows are laid out in the frame after the one the press built it in
            rt.frame(0.016);
            assert!(rt.find(drawn).is_some(), "{name}: drawn from pr's page ({drawn})");
        }
        assert_eq!(rt.state.pins.list.len(), 3, "three rows in Pinned");
    }

    /// The lens head's stills (proto-reader, N-1446 p157): `STILL_OUT=DIR … withyou::tests::lens_stills --ignored`, on
    /// the made-up project [`research`] of the two tests above. pr's page with the bar's ⋯ open, offering pr's With you,
    /// a2o and a2a to pin (lens-offers.png); then two panes on With you · pr, N-8 in the focused one and N-7 kept in the
    /// one beside, With you · pr and pr ↔ you pinned (lens-two-panes.png); the Structure section unfolded, Insights and
    /// thread 8 open, thread 9 under it (structure.png; paper-research N-1446 p848).
    #[test]
    #[ignore]
    fn lens_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn base() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut p = research();
            p.protocol = coord_types::PROTOCOL;
            s.projects = vec![p];
            s.cur = 0;
            s
        }
        fn offers() -> App {
            let mut s = base();
            s.sel = Sel::Agent("pr".into());
            s.bar_menu = true;
            s
        }
        fn two_panes() -> App {
            let mut s = base();
            for f in [crate::lens::Filter::WithYou { agent: "pr".into() }, crate::lens::Filter::A2o { agent: "pr".into() }] {
                let pin = s.lens_pin(&crate::lens::Lens::of(f)).unwrap();
                s.pins.list.push(pin);
            }
            s.show_sel(with_pr(Some("N-8")));
            s.drop_sel(0, with_pr(Some("N-7")), 0, crate::panes::Zone::Right);
            s.focus_pane(0);
            s
        }
        fn structure() -> App {
            let mut s = base();
            s.streams.open.extend(["tree|0|insights", "tree|0|N-8"].map(String::from));
            s.show_sel(with_pr(Some("N-8")));
            s
        }
        let shots: [(&str, fn() -> App); 3] = [("lens-offers.png", offers), ("lens-two-panes.png", two_panes), ("structure.png", structure)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1600x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }

    /// The stills of who still owes his message (proto-lead N-1479 p138, consult-3 p141; I-1455 (B)), on a made-up
    /// project: his N-200.p1 asks w2 and w3, and w2 answers it at p2; w3 has not. The Threads page, where "Asked, no
    /// answer yet" names who has it (asked-threads.png); w2's page, whose list no longer has it (asked-w2.png); w3's,
    /// whose list has it (asked-w3.png); and the box under each one's session (the work behind
    /// its replies), whose line says where his next words go: a new thread with w2 (asked-w2-box.png), N-200 with w3
    /// (asked-w3-box.png). On N-201, a lead's thread, w2 answers him apart with a `coord post`: his stream with w2 has
    /// no "no answer yet" under it, with w3 it has (asked-w2-stream.png, asked-w3-stream.png; proto-reader p164).
    /// `STILL_OUT=DIR … withyou::tests::asked_stills --ignored --exact`.
    #[test]
    #[ignore]
    fn asked_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn base() -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.down = None;
            p.protocol = coord_types::PROTOCOL;
            let agent = |id: &str, name: &str| -> coord_types::AgentView {
                serde_json::from_value(json!({"id": id, "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": "idle", "since": "2026-11-21T15:00:00Z", "tools": [], "items": []})).unwrap()
            };
            p.live.agents = vec![agent("AGT-2", "w2"), agent("AGT-3", "w3")];
            let mut v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-200", "n": 200, "kind": "thread", "opener": "owner", "ctx": {}, "form": {},
                "title": "the stills of the Map", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-21T16:00:00Z", "given": [{"to": "w2", "at": "2026-11-21T16:00:01Z", "delivery": "N-200.p1", "upto": 1}, {"to": "w3", "at": "2026-11-21T16:00:01Z", "delivery": "N-200.p1", "upto": 1}],
                "posts": [
                    {"n": 1, "at": "2026-11-21T16:00:00Z", "by": "owner", "text": "@w2 @w3 can you both check the stills?", "names": ["w2", "w3"], "told": ["w2", "w3"]},
                    {"n": 2, "at": "2026-11-21T16:05:00Z", "by": "w2", "text": "Mine are right: 4 of 4.", "names": ["owner"], "answering": ["owner"]}]})).unwrap();
            v.unanswered_owner = vec![1];
            v.asked = serde_json::from_value(json!([{"post": 1, "agent": "w2", "answer": "N-200.p2"}, {"post": 1, "agent": "w3"}])).unwrap();
            p.live.tickets.insert(200, v);
            // a lead's thread: w2 answers his message with a `coord post "@owner …"`, which stands apart in w2's stream
            // (proto-reader N-1479 p164)
            let mut lead: coord_types::TicketView = serde_json::from_value(json!({"id": "N-201", "n": 201, "kind": "thread", "opener": "lead", "ctx": {}, "form": {},
                "title": "the Map's islands", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-21T16:10:00Z", "given": [], "posts": [
                    {"n": 1, "at": "2026-11-21T16:10:00Z", "by": "owner", "text": "@w2 @w3 are the islands laid?", "names": ["w2", "w3"], "told": ["w2", "w3"]},
                    {"n": 2, "at": "2026-11-21T16:12:00Z", "by": "w2", "text": "@owner mine are: 9 of 9.", "names": ["owner"]}]})).unwrap();
            lead.unanswered_owner = vec![1];
            lead.asked = serde_json::from_value(json!([{"post": 1, "agent": "w2", "answer": "N-201.p2"}, {"post": 1, "agent": "w3"}])).unwrap();
            p.live.tickets.insert(201, lead);
            s.projects = vec![p];
            s.cur = 0;
            s
        }
        fn threads() -> App {
            let mut s = base();
            s.sel = Sel::Tickets;
            s
        }
        fn w2() -> App {
            let mut s = base();
            s.sel = Sel::Agent("w2".into());
            s
        }
        fn w3() -> App {
            let mut s = base();
            s.sel = Sel::Agent("w3".into());
            s
        }
        // the work behind the replies (chat.rs): its box says where his next words to the agent go (withyou.rs `line`);
        // With agents has no box
        fn w2_box() -> App {
            let mut s = w2();
            s.streams.which.insert((0, "w2".into()), crate::streams::Which::Work);
            s
        }
        fn w3_box() -> App {
            let mut s = w3();
            s.streams.which.insert((0, "w3".into()), crate::streams::Which::Work);
            s
        }
        // his stream with each (the `↔ you` lens): "no answer yet" under N-201.p1 only in w3's
        fn a2o(name: &str) -> App {
            let mut s = base();
            s.show_sel(crate::lens::Lens::of(crate::lens::Filter::A2o { agent: name.into() }).as_sel());
            s
        }
        fn w2_stream() -> App {
            a2o("w2")
        }
        fn w3_stream() -> App {
            a2o("w3")
        }
        let shots: [(&str, fn() -> App); 7] = [("asked-threads.png", threads), ("asked-w2.png", w2), ("asked-w3.png", w3), ("asked-w2-box.png", w2_box), ("asked-w3-box.png", w3_box), ("asked-w2-stream.png", w2_stream), ("asked-w3-stream.png", w3_stream)];
        for (name, start) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1600x900".into(), "--at".into(), "1.0".into()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), start, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
