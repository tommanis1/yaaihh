//! What one frame of the agents' pages costs to build, on a project the size of research's (the owner, N-412: "fixing
//! htat angets view is slwo as shit"). Ignored: a timing, not a check; run it alone, release:
//! `cargo test --release -p yaaihh view_cost -- --ignored --nocapture --test-threads 1`.
//!
//! The project is made up to research's counts on 17 Nov (its record: about 250 threads and notes open or closed
//! within the day, 2,300 posts, N-101 alone 1,880 of them, 12,000 given lines), with six agents working, each holding a
//! chat of 3,000 entries (a day's follow of a busy agent). HOME, XDG_STATE_HOME and YAAIHH_CONFIG point into an empty
//! folder of its own, so nothing of the owner's is read or written.

use crate::{App, Proj, Sel};
use coord_types::{Entry, Transcript};
use docket_machine::TicketView;
use serde_json::json;
use std::time::{Duration, Instant};

const AGENTS: [&str; 6] = ["paper-research", "docket-core", "docket-edge", "yaaihh", "docket-core2", "a2h-speed"];

fn at(min: u64) -> String {
    let t = chrono::DateTime::parse_from_rfc3339("2026-11-12T08:00:00Z").unwrap() + chrono::Duration::seconds(min as i64 * 37);
    t.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn thread(n: u64, opener: &str, posts: usize, clock: &mut u64) -> TicketView {
    let mut ps = Vec::new();
    let mut given = Vec::new();
    for i in 1..=posts {
        *clock += 1;
        let k = (n as usize * 7 + i) % 25;
        // about one post in eight is the owner's; paper-research answers him and leads; the others report to it
        let (by, names, auto) = match k {
            0..=2 => ("owner", vec!["paper-research"], false),
            3..=11 => ("paper-research", vec![AGENTS[1 + k % 5]], k % 2 == 0),
            _ => (AGENTS[1 + k % 5], vec!["paper-research"], k % 3 == 0),
        };
        let text = format!("post {i} of N-{n}: {}", "words of a report, with a table and a few lines. ".repeat(1 + k % 6));
        ps.push(json!({"n": i, "at": at(*clock), "by": by, "text": text, "names": names, "told": ["owner"], "auto": auto}));
        for a in AGENTS.iter().take(1 + i % 5) {
            given.push(json!({"to": a, "delivery": format!("N-{n}.p{i}"), "upto": i, "at": at(*clock)}));
        }
    }
    serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": opener, "ctx": {}, "form": {"to": "paper-research"},
        "title": format!("thread {n}"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
        "opened_at": at(n * 3), "given": given, "posts": ps}))
    .unwrap()
}

fn chat(name: &str) -> Transcript {
    let mut entries = Vec::new();
    for i in 0..3000u64 {
        let (kind, tool, text) = match i % 6 {
            0 => ("user", None, format!("<channel source=\"coord\" from=\"x\">message {i} for {name}</channel>")),
            1 | 4 => ("assistant", None, format!("Working on step {i}: {}", "reading the code and the record. ".repeat(3))),
            2 => ("tool", Some("Bash".to_string()), format!("{{\"command\": \"grep -n thing src/file{i}.rs\"}}")),
            3 => ("result", None, format!("src/file{i}.rs:12: a line\nsrc/file{i}.rs:40: another line")),
            _ => ("tool", Some("Read".to_string()), format!("{{\"file_path\": \"/src/f{i}.rs\"}}")),
        };
        entries.push(Entry { offset: i * 900, at: at(i), kind: kind.into(), tool, text, patch: None, images: Vec::new() });
    }
    Transcript { entries, start: 0, next: 3000 * 900, session: format!("{name}.jsonl"), ..Default::default() }
}

/// The made-up project of these scenes, for other modules' tests.
pub(crate) fn project_for_tests() -> Proj {
    project()
}

fn project() -> Proj {
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.live.agents = AGENTS
        .iter()
        .enumerate()
        .map(|(i, a)| {
            serde_json::from_value(json!({"id": format!("AGT-{i}"), "name": a, "parent": if i == 0 { "owner" } else { "paper-research" },
                "kind": "claude", "model": "claude-opus-5-5", "state": "working", "since": at(1), "tools": [], "items": []}))
            .unwrap()
        })
        .collect();
    // VIEW_COST_TICKETS=FILE: the tickets of a record as a snapshot sends them (coordd's views_of_a_coords_record with
    // VIEWS_ALL), in place of the made-up ones
    if let Ok(f) = std::env::var("VIEW_COST_TICKETS") {
        let views: Vec<TicketView> = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        p.live.tickets = views.into_iter().map(|v| (v.n, v)).collect();
        for a in AGENTS {
            p.convs.entry(a.to_string()).or_default().merge(chat(a), false);
        }
        return p;
    }
    let mut clock = 0;
    let mut n = 1;
    let mut big = thread(101, "paper-research", 1880, &mut clock);
    big.n = 101;
    p.live.tickets.insert(101, big);
    while p.live.tickets.len() < 250 {
        n += 1;
        if n == 101 {
            continue;
        }
        let opener = if n % 4 == 0 { "owner" } else { AGENTS[(n as usize) % 6] };
        p.live.tickets.insert(n, thread(n, opener, 2, &mut clock));
    }
    for a in AGENTS {
        p.convs.entry(a.to_string()).or_default().merge(chat(a), false);
    }
    p
}

/// The tests that make a whole App, one at a time: it reads and keeps its config and state beside YAAIHH_CONFIG, which
/// points into a folder of their own.
static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The lock of the tests that make a whole App or point YAAIHH_CONFIG elsewhere (state.rs's), held while one runs.
pub(crate) fn one() -> std::sync::MutexGuard<'static, ()> {
    ONE.lock().unwrap_or_else(|e| e.into_inner())
}

/// An App of its own, nothing of the owner's read or written, nor of another test's: the lock held while it lives, and
/// YAAIHH_CONFIG pointed at this process's folder again with no ui.json in it. A test that writes ui.json (a Send, Add
/// comment's press, a swap's `begin`) left it there for whichever test came next, whose window then started on that
/// test's view (build 37's check: find's With you test opened on ask.rs's N-687); state.rs's tests point YAAIHH_CONFIG
/// at a folder of their own and leave it there.
pub(crate) fn test_app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let held = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock, which every test that reads or sets them holds; the crate's other tests read none
    unsafe {
        std::env::set_var("HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml"));
    }
    // and his window's states and his pins (state.rs `deltas_path`, sidebar.rs), which a test of them writes there
    for f in ["ui.json", "ui-dev.json", "ui-deltas.jsonl", "ui-dev-deltas.jsonl", "pins.json", "pins-lenses.json"] {
        let _ = std::fs::remove_file(dir.join(f));
    }
    let mut s = crate::init();
    // the Map's tests from before its time view draw the cloud as it was then; the time view's own set it (spacetime.rs)
    s.cloud.space = crate::map::spacetime::Space::BEFORE;
    (held, s)
}

/// An App with one project, its agents NAMES (most lately active last), its pipe up and a link whose requests go into
/// a `cat` that drops them: what is asked is in the project's `pending`, and replies are a test's to give (`App::take`).
pub(crate) fn linked_app(names: &[&str]) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
    let (held, mut s) = test_app();
    let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.live.agents = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            serde_json::from_value(serde_json::json!({"id": format!("AGT-{i}"), "name": n, "parent": "owner", "kind": "claude", "model": "m", "state": "idle",
                "since": "2026-11-18T08:00:00Z", "last_active": format!("2026-11-18T10:{i:02}:00Z"), "tools": [], "items": []}))
            .unwrap()
        })
        .collect();
    let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    p.link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    s.projects = vec![p];
    s.cur = 0;
    (held, s, child)
}

/// The id of the page of AGENT's session asked and not answered in S's first project.
pub(crate) fn page_asked(s: &App, agent: &str) -> Option<u64> {
    s.projects[0].pending.iter().find(|(_, p)| matches!(p, crate::Pending::Page { agent: a, older: false } if a == agent)).map(|(id, _)| *id)
}

/// coordd's answer to page ID: entries at OFFSETS (an image on each one in IMAGED), from START to NEXT of session `s`.
pub(crate) fn page_reply(id: u64, start: u64, next: u64, offsets: &[u64], imaged: &[u64]) -> crate::link::Incoming {
    let entries = offsets
        .iter()
        .map(|o| coord_types::Entry { offset: *o, at: "2026-11-18T10:00:00Z".into(), kind: "assistant".into(), tool: None, text: format!("at {o}"), patch: None, images: if imaged.contains(o) { vec![1] } else { vec![] } })
        .collect();
    let out = serde_json::to_string(&coord_types::Transcript { entries, start, next, session: "s".into(), ..Default::default() }).unwrap();
    crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None })
}

fn app() -> (std::sync::MutexGuard<'static, ()>, App) {
    let (held, mut s) = test_app();
    s.projects = vec![project()];
    s.cur = 0;
    (held, s)
}

/// The window's runtime on S, its first frame drawn.
pub(crate) fn window(s: App) -> rune::runtime::Runtime<App> {
    let mut rt = rune::runtime::Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
    rt.frame(0.0);
    rt
}

/// Whole frames of the page RT shows: the first after it opens (its rows built), then the mean of 20 more, each
/// running the view again (a spinner's, a counter's frame).
fn frames(rt: &mut rune::runtime::Runtime<App>) -> (f64, f64) {
    let t = Instant::now();
    rt.frame(0.016);
    let first = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    for _ in 0..20 {
        rt.invalidate();
        rt.frame(0.016);
    }
    (first, t.elapsed().as_secs_f64() * 1e3 / 20.0)
}

/// The mean and the most of building the focused page N times.
fn time(s: &App, k: &rune_kit::Kit, n: usize) -> (Duration, Duration) {
    let mut most = Duration::ZERO;
    let all = Instant::now();
    for _ in 0..n {
        let t = Instant::now();
        std::hint::black_box(crate::panes::page(s, k));
        most = most.max(t.elapsed());
    }
    (all.elapsed() / n as u32, most)
}

#[test]
#[ignore]
fn view_cost() {
    let (_held, mut s) = app();
    let k = rune_kit::Kit::neutral(false);
    let posts: usize = s.projects[0].live.tickets.values().map(|v| v.posts.len()).sum();
    println!("project: {} tickets, {posts} posts, {} agents working, chats of 3000 entries", s.projects[0].live.tickets.len(), AGENTS.len());
    let big = s.projects[0].live.tickets.values().max_by_key(|v| v.posts.len()).map(|v| v.n);
    for (what, sel, which, shown) in [
        ("the agents page (tiles, chat tails)", Sel::Agents, None, None),
        ("paper-research's page, his stream", Sel::Agent("paper-research".into()), Some(crate::streams::Which::You), None),
        ("paper-research's page, the agents' stream", Sel::Agent("paper-research".into()), Some(crate::streams::Which::Agents), None),
        ("paper-research's page, the work (chat)", Sel::Agent("paper-research".into()), Some(crate::streams::Which::Work), None),
        ("docket-edge's page, his stream", Sel::Agent("docket-edge".into()), Some(crate::streams::Which::You), None),
        ("the Threads page, the longest thread open", Sel::Tickets, None, big),
    ] {
        s.sel = sel.clone();
        s.tickets.shown = shown;
        if let (Sel::Agent(a), Some(w)) = (&sel, which) {
            s.streams.which.insert((0, a.clone()), w);
        }
        let (mean, most) = time(&s, &k, 20);
        if let (Sel::Agent(a), Some(crate::streams::Which::You)) = (&sel, which) {
            let c = crate::streams::cost(&s.projects[0], a).map(|d| format!("{:.2}", d.as_secs_f64() * 1e3));
            println!("  his stream's parts, ms: posts {}, lines {}, targets {}, 1000 days {}, 1000 spans {}", c[0], c[1], c[2], c[3], c[4]);
        }
        println!("{what}: {:.2} ms a frame (most {:.2} ms)", mean.as_secs_f64() * 1e3, most.as_secs_f64() * 1e3);
    }
    // whole frames: the view, layout and what is drawn, with the window's size; the pages above again
    let mut rt = window(s);
    for (what, sel, which, shown) in [
        ("the agents page", Sel::Agents, None, None),
        ("paper-research's page, his stream", Sel::Agent("paper-research".into()), Some(crate::streams::Which::You), None),
        ("the Threads page, the longest thread open", Sel::Tickets, None, big),
    ] {
        rt.update(|s, _| {
            s.sel = sel.clone();
            s.tickets.shown = shown;
            if let (Sel::Agent(a), Some(w)) = (&sel, which) {
                s.streams.which.insert((0, a.clone()), w);
            }
        });
        let (first, mean) = frames(&mut rt);
        println!("{what}, whole frames: the first {first:.1} ms, then {mean:.2} ms a frame");
    }
}

/// The spinners of the agents' pages turn with the system's animations off, as on his Windows ("Show animations" off,
/// which Rune reads as reduced motion and held every loop still: his "they hang", N-912.p32; N-1048), with no input:
/// the frames go on and the phase moves. They cost no more than their steps: at 20 a second, and a second frame of the
/// same picture after each on Windows, the window took a core of his laptop (N-1048 post 3), so they step ten times a
/// second (rune `edge5-steady`) and the window draws nothing between two steps. With nobody working there is no
/// spinner, so nothing asks for their frames.
#[test]
fn the_working_marks_turn_with_the_systems_animations_off_and_stop_with_the_work() {
    let (_held, mut s) = app();
    s.sel = Sel::Agents;
    let mut rt = window(s);
    rt.set_system_reduced_motion(true);
    let phases = |rt: &rune::runtime::Runtime<App>| rt.elements().iter().filter_map(|e| e.loop_phase).collect::<Vec<f64>>();
    rt.frame(0.016);
    let first = phases(&rt);
    assert!(!first.is_empty(), "the six working agents' spinners are on the page");
    // the page's own entrance settles first, so what is left are the spinners alone
    let mut settling = 0;
    while rt.frame(1.0 / 30.0) {
        settling += 1;
        assert!(settling < 60, "the page still animates two seconds in, with nobody touching it");
    }
    // no key, no pointer, no message: two seconds of only the frames the window's clock asks for
    let mut moved = 0;
    let mut last = phases(&rt);
    let start = rt.wake_deadline().expect("a frame is asked for while they work");
    rt.frame((start - rt.clock).max(0.0));
    let (start, mut frames) = (rt.clock, 0);
    while rt.clock < start + 2.0 - 1e-6 {
        let gap = rt.wake_deadline().expect("a frame is asked for while they work") - rt.clock;
        assert!((0.1 - 1e-3..=0.1 + 1e-3).contains(&gap), "the spinners step ten times a second: the next frame is a tenth of a second on, not {gap:.3} s");
        assert!(!rt.frame(gap), "and nothing else on the page asks for a frame between two steps");
        frames += 1;
        let now = phases(&rt);
        moved += now.iter().zip(&last).filter(|(a, b)| (*a - *b).abs() > 1e-6).count();
        last = now;
    }
    assert!(frames <= 20, "at most ten frames a second while only the spinners move: {frames} in two seconds");
    assert!(moved > 0, "the spinners' phase moves with nobody touching the window: {first:?} then {last:?}");
    // the work ends: no spinner left, and nothing of theirs asks for frames
    rt.update(|s, _| {
        for a in &mut s.projects[0].live.agents {
            a.state = "idle".into();
        }
    });
    rt.frame(0.016);
    assert!(phases(&rt).is_empty(), "no spinner once nobody works");
    // the rows' marks now fade to idle (they jumped while the system's setting held the window still); once they have
    // faded, within a second, nothing on the page animates and the window sleeps until its own timers or a message
    let mut frames = 0;
    while rt.frame(1.0 / 30.0) {
        frames += 1;
        assert!(frames < 30, "the page still animates a second after the work ended");
    }
    assert!(!rt.frame(0.1), "and nothing on the page animates once the fades are done");
}

/// A project holding thread N-101 of POSTS posts, shown on the Threads page, in a window.
fn long_thread(posts: usize) -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
    let (held, s) = test_app();
    (held, window(long_thread_on(s, posts)))
}

fn long_thread_on(mut s: App, posts: usize) -> App {
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let mut clock = 0;
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(thread(101, "owner", posts, &mut clock)) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(101);
    s
}

/// Frames until the scroll has come to rest.
fn settle(rt: &mut rune::runtime::Runtime<App>) {
    for _ in 0..90 {
        rt.update(|s, tx| _ = crate::find::seek(s, tx));
        rt.frame(0.016);
    }
}

/// A post found by search on a long thread (docket-edge, N-422.p9): its row is not built while far up, and the jump
/// brings it into view. Never read, the thread opens at its foot (item 15, paper-research N-1446 p2603 (a)).
#[test]
fn a_search_brings_a_post_of_a_long_thread_into_view() {
    let (_held, mut rt) = long_thread(400);
    assert!(rt.find("post-N-101-400").is_some(), "the foot is drawn");
    assert!(rt.find("post-N-101-50").is_none(), "far up, its row is not built");
    rt.update(|s, _| s.tickets.find.seek = Some((101, 50, 3)));
    settle(&mut rt);
    let (_, y) = rt.find("post-N-101-50").expect("its row is built");
    assert!((0.0..900.0).contains(&y), "in view: {y}");
}

/// The rows built again with one more above (a ticket opened under the thread early on), the post in view stays where
/// it was on the screen (docket-edge, N-422.p9).
#[test]
fn the_post_in_view_stays_put_when_the_rows_are_built_again() {
    let (_held, mut rt) = long_thread(400);
    rt.update(|s, _| s.tickets.find.seek = Some((101, 200, 3)));
    settle(&mut rt);
    let (_, before) = rt.find("post-N-101-200").expect("in view");
    let mut clock = 0;
    let mut child = thread(500, "owner", 1, &mut clock);
    child.opened_at = at(4);
    let mut v = rt.state.projects[0].live.tickets[&101].clone();
    v.children.push(child.id.clone());
    rt.update(|s, _| {
        s.projects[0].live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Ticket(child) });
        s.projects[0].live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Ticket(v) });
    });
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let p = &rt.state.projects[0];
    let rows = crate::threads::seq(p, &p.live.tickets[&101]);
    let at_child = rows.iter().position(|r| *r == crate::threads::TRow::Opened(500)).expect("its row");
    let at_post = rows.iter().position(|r| *r == crate::threads::TRow::Post(200)).unwrap();
    assert!(at_child < at_post, "the new row is above");
    let (_, after) = rt.find("post-N-101-200").expect("still drawn");
    assert!((after - before).abs() < 1.0, "it stays put: {before} then {after}");
}

/// The words of thread V's posts as his long threads have them (N-1104): a report's lines naming a post, one post in
/// seven with a table and one with a block of code.
fn marked_up(mut v: TicketView) -> TicketView {
    let id = v.id.clone();
    for x in v.posts.iter_mut() {
        let i = x.n;
        let mut words = format!("post {i} of {id}: {}", "words of a report, as N-101.p3 has them, with a few lines. ".repeat(1 + i as usize % 4));
        match i % 7 {
            0 => words.push_str(&format!("\n\n| piece | head | runs |\n|---|---|---|\n| typing | `4eccc80b` | {i} |\n| spinner | `997b9052` | 2 |\n| tables | none | 0 |\n")),
            3 => words.push_str(&format!("\n\n```rust\nfn row_{i}(s: &App) -> El {{\n    col().child(text(\"{i}\"))\n}}\n```\n")),
            _ => {}
        }
        x.text = serde_json::from_value(json!(words)).unwrap();
    }
    v
}

/// With you as he has it on N-1104 (N-1272): the research-sized project with its six agents working, paper-research's
/// With you on a thread of 300 posts, the box under it focused.
fn with_you_long() -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
    let (held, mut s) = app();
    let mut clock = 0;
    s.projects[0].live.tickets.insert(1104, marked_up(thread(1104, "owner", 300, &mut clock)));
    let who = "paper-research".to_string();
    s.sel = Sel::Agent(who.clone());
    s.streams.which.insert((0, who.clone()), crate::streams::Which::You);
    s.withyou.inside.insert((0, who), 1104);
    let mut rt = window(s);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let f = rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-thread:N-1104")).map(|e| e.rect).expect("the thread's box");
    let (x, y) = (f[0] + 12.0, f[1] + 12.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    (held, rt)
}

/// N-1104 as coordd sends it with one post more, its 301st.
fn one_post_more(rt: &rune::runtime::Runtime<App>) -> coord_types::Event {
    let p = &rt.state.projects[0];
    let mut v = p.live.tickets[&1104].clone();
    let mut x = v.posts.last().cloned().unwrap();
    x.n = 301;
    x.by = "paper-research".into();
    x.text = serde_json::from_value(json!("post 301 of N-1104: the head is named.")).unwrap();
    v.posts.push(x);
    coord_types::Event { seq: p.live.seq + 1, body: coord_types::Body::Ticket(v) }
}

/// The text in N-1104's box.
fn in_the_box(rt: &rune::runtime::Runtime<App>) -> Option<&str> {
    rt.state.composers.get(&(0, crate::message_box::thread_place("N-1104"))).map(|e| e.text.as_str())
}

/// Key to letter in With you (N-1272): each key typed into the box of a 300-post thread, six agents working, timed with
/// the frame that draws its letter (the view, the commit, the layout and the text; no GPU in a test), then an update
/// from coordd that adds a post, timed the same way. Ignored: a timing, not a check; release, alone:
/// `cargo test --release -p yaaihh --lib with_you_typing -- --ignored --nocapture --test-threads 1`.
#[test]
#[ignore]
fn with_you_typing() {
    let (_held, mut rt) = with_you_long();
    let words = "the stills first, then the tables, dark and light both";
    let mut ms = Vec::new();
    for c in words.chars() {
        let t = Instant::now();
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
        rt.frame(0.016);
        ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    assert_eq!(in_the_box(&rt), Some(words), "every letter in the box");
    let ev = one_post_more(&rt);
    let t = Instant::now();
    rt.update(|s, _| _ = s.projects[0].live.apply(ev));
    rt.frame(0.016);
    let update = t.elapsed().as_secs_f64() * 1e3;
    ms.sort_by(f64::total_cmp);
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    println!(
        "with you, N-1104 of 300 posts, six agents working: key to letter over {} keys: median {:.1} ms, p90 {:.1} ms, most {:.1} ms; an update adding a post: {update:.1} ms",
        ms.len(),
        at(0.5),
        at(0.9),
        at(1.0)
    );
}

/// Where a key's frame in With you goes (N-1793, the owner's overnight list, item 1): his frames.jsonl on 25 Nov, the
/// seconds he typed in With you, slowest frame p50 33 ms, p90 79 ms, with rune's `view` and `commit` the most of it.
/// The same 54 keys as `with_you_typing`, with rune's per-second log on (`RUNE_FRAME_WHY`, the one his window
/// writes), then its phases summed over the keys' frames, and the app's own views counted. Ignored: a timing, not a
/// check; alone: `cargo test --profile quick -p yaaihh --lib with_you_key_phases -- --ignored --nocapture --test-threads 1`.
#[test]
#[ignore]
fn with_you_key_phases() {
    let log = std::env::temp_dir().join(format!("yaaihh-key-phases-{}.jsonl", std::process::id()));
    let _ = std::fs::remove_file(&log);
    // SAFETY: set before this thread's first frame, which reads it once; the test runs alone
    unsafe { std::env::set_var("RUNE_FRAME_WHY", &log) };
    let (_held, mut rt) = with_you_long();
    // rune writes a row once a second has passed, at a frame's end: a key after a second's pause is a row of its own
    let words = "the stills first, then";
    let mut views = Vec::new();
    for c in words.chars() {
        std::thread::sleep(Duration::from_millis(1010));
        crate::VIEWS.with(|n| n.set(0));
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
        rt.frame(0.016);
        views.push(crate::VIEWS.with(|n| n.get()));
    }
    assert!(in_the_box(&rt).is_some_and(|t| t.ends_with(words)), "every letter in the box");
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&log).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let _ = std::fs::remove_file(&log);
    // the last rows, one a key (the first rows are the set-up's)
    let keys: Vec<&serde_json::Value> = rows.iter().rev().take(words.chars().count()).filter(|r| r["frames"] == 1).collect();
    let n = keys.len() as f64;
    let mean = |f: &dyn Fn(&serde_json::Value) -> f64| keys.iter().map(|r| f(r)).sum::<f64>() / n.max(1.0);
    let ph = |r: &serde_json::Value, k: &str| r["max_phases"][k].as_f64().unwrap_or(0.0);
    let mut total: Vec<f64> = keys.iter().map(|r| r["ms"].as_f64().unwrap_or(0.0)).collect();
    total.sort_by(f64::total_cmp);
    println!(
        "with you, {} keys each a frame of its own: frame median {:.1} ms, most {:.1}; means: view {:.1}, commit without the view {:.1}, layout {:.1}, text {:.1}, paint {:.1}, apply {:.1}, outside the phases {:.1}; app views {:?}",
        keys.len(),
        total.get(total.len() / 2).copied().unwrap_or(0.0),
        total.last().copied().unwrap_or(0.0),
        mean(&|r| ph(r, "view")),
        mean(&|r| ph(r, "commit") - ph(r, "view")),
        mean(&|r| ph(r, "layout")),
        mean(&|r| ph(r, "text")),
        mean(&|r| ph(r, "paint")),
        mean(&|r| ph(r, "apply")),
        mean(&|r| r["ms"].as_f64().unwrap_or(0.0) - ph(r, "commit") - ph(r, "layout") - ph(r, "paint") - ph(r, "apply")),
        views
    );
}

/// A message of his kind, made-up words with his typos as often as he makes them (about one a dozen characters:
/// "sjsut", "tlakign", "ptograms"), 812 characters: 65 words marked once the spelling has read it.
const HIS_LONG: &str = "yeha so the thign i mean is when i opne the map i whant to see waht each agnet is doign rihgt now and waht it \
waits on, not jsut the thraeds but the acutal steps and who hlods them. and if somethign is stuck for mroe then a few mintues it shoudl \
say so claerly wiht one mark not fvie. aslo the csots: per goal, tokesn and moeny and the shaer of my limtis, but behidn a clcik so it \
deos not cluttr. and dont mkae me ask twcie, if i siad it once in the caht it shoudl be pikced up and put somwhere i can see it. the \
brian shoudl odrer thigns before they get delivred, long streches of focsu for each agnet, not constnat interupts. i thnik msot of \
this is alreday tehre in some from but its not wierd rihgt and the inptus to the modles are nosiy. lets get the bascis sloid frist \
tehn we add the fnacy stuff ltaer ok";

/// His long messages in With you (docket-edge5, N-1446 p2357): on build 73 a key's frame grew with what he had typed of
/// the message, about 20 ms at its start, 120–170 ms after 700 keys, and 1.2 s after 800 on 23 Nov (his frames.jsonl,
/// rune's commit outside the view and the view both growing). A message of his kind, made-up words with his typos as
/// often as he makes them (about one a dozen characters: "sjsut", "tlakign", "ptograms"), about 800 characters, typed into N-1104's box, the spelling on; after each key Harper's marks of the text as
/// it stands, and a frame for them, as his window draws one when they come. At each 100 characters it prints the keys'
/// frames (the mean and the most of that hundred), the views each key built, the marks' frame, a frame with nothing
/// changed, and each part the box builds on each view, timed alone. Ignored: a timing, not a check; release, alone:
/// `cargo test --release -p yaaihh --lib his_long_message -- --ignored --nocapture --test-threads 1`.
#[test]
#[ignore]
fn his_long_message() {
    let (_held, mut rt) = with_you_long();
    rt.update(|s, _| {
        s.spell.on = true;
        s.projects[0].protocol = coord_types::PROTOCOL;
    });
    rt.frame(0.016);
    let _ = crate::marks::symspell("warm");
    let _ = crate::suggest::complete("wai", &[]);
    let words = HIS_LONG;
    let k = rune_kit::Kit::neutral(false);
    let key = (0, crate::message_box::thread_place("N-1104"));
    let target = crate::message_box::Target::Thread { project: 0, t: "N-1104".into(), post: None, way: crate::message_box::Way::Here };
    println!("his long message: {} characters, the checker {}, the spelling on {}", words.chars().count(), crate::spell::available(), rt.state.spell.on);
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1e3;
    // the mean of N runs of F, in ms
    fn alone(n: usize, mut f: impl FnMut()) -> f64 {
        let t = Instant::now();
        for _ in 0..n {
            f();
        }
        t.elapsed().as_secs_f64() * 1e3 / n as f64
    }
    let (mut keys, mut marks, mut views) = (Vec::new(), Vec::new(), 0usize);
    for (i, c) in words.chars().enumerate() {
        crate::VIEWS.with(|n| n.set(0));
        let t = Instant::now();
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
        rt.frame(0.016);
        keys.push(ms(t));
        views += crate::VIEWS.with(|n| n.get());
        let text = in_the_box(&rt).unwrap_or_default().to_string();
        let _ = crate::marks::wait(&text);
        let t = Instant::now();
        rt.invalidate();
        rt.frame(0.016);
        marks.push(ms(t));
        let n = i + 1;
        if n % 100 != 0 && n != words.chars().count() {
            continue;
        }
        let hundred = &keys[keys.len().saturating_sub(100)..];
        let mean = hundred.iter().sum::<f64>() / hundred.len() as f64;
        let most = hundred.iter().copied().fold(0.0, f64::max);
        let marks_mean = marks[marks.len().saturating_sub(100)..].iter().sum::<f64>() / hundred.len() as f64;
        crate::VIEWS.with(|v| v.set(0));
        let t = Instant::now();
        rt.invalidate();
        rt.frame(0.016);
        let still = ms(t);
        let still_views = crate::VIEWS.with(|v| v.get());
        let s = &rt.state;
        let e = s.composers.get(&key).expect("the box's text");
        let page = time(s, &k, 5).0.as_secs_f64() * 1e3;
        let (marked, changed) = (crate::spell::marked_box(s, &key, e).len(), crate::spell::changed_at(s, &key, e).len());
        let parts = [
            ("marked", alone(5, || _ = std::hint::black_box(crate::spell::marked(s, 0, e)))),
            ("marked_box", alone(5, || _ = std::hint::black_box(crate::spell::marked_box(s, &key, e)))),
            ("changed_at", alone(5, || _ = std::hint::black_box(crate::spell::changed_at(s, &key, e)))),
            ("ghost", alone(5, || _ = std::hint::black_box(crate::suggest::ghost(s, &key)))),
            ("harper of", alone(5, || _ = std::hint::black_box(crate::marks::of(&e.text)))),
            ("names", alone(5, || _ = std::hint::black_box(crate::spell::names(s, 0)))),
            ("groups", alone(5, || _ = std::hint::black_box(crate::message_box::with_groups(s, &target, String::new(), &e.text)))),
            ("words shown", alone(5, || _ = std::hint::black_box(crate::words::shown(s, 0, &key.1, &k)))),
            ("split chips", alone(5, || _ = std::hint::black_box(crate::split::chips(s, &target, &k)))),
        ];
        println!(
            "  at {n:>4} chars, {marked} words marked and {changed} changed: key frame mean {mean:.1} ms, most {most:.1} ms, {:.1} views a key; marks' frame {marks_mean:.1} ms; a frame with nothing changed {still:.1} ms ({still_views} views); the page's view alone {page:.1} ms; parts, ms a call: {}",
            views as f64 / hundred.len() as f64,
            parts.iter().map(|(w, t)| format!("{w} {t:.3}")).collect::<Vec<_>>().join(", ")
        );
        views = 0;
    }
    assert_eq!(in_the_box(&rt), Some(words), "every letter in the box");
}

/// The cost stays flat (N-1446 p2358): a frame of With you with his long draft in N-1104's box, 65 words marked, costs
/// about what one with its first 100 characters costs (8 marked). Each marked word's hover region is placed by a layout
/// read of its highlight (message_box.rs `regions`), and rune laid out every highlight of the text for each read, so a
/// frame cost the marks squared: on 2dec9b2c the long draft's frame took 5.4 times the short one's
/// (`his_long_message`: 158 ms against 29 ms). With rune `edge5-marks` a read lays out its own highlight alone. The
/// least of five frames of each, at most twice the short one's. The same for his comment on a passage of N-1104, in
/// the same message box at the passage (ask.rs; docket-edge6 N-1446 p2373: typing a comment slowed the same way).
#[test]
fn a_frame_costs_about_the_same_with_few_or_many_words_marked() {
    let (_held, mut rt) = with_you_long();
    rt.update(|s, _| {
        s.spell.on = true;
        s.projects[0].protocol = coord_types::PROTOCOL;
    });
    let _ = crate::marks::symspell("warm");
    let key = (0, crate::message_box::thread_place("N-1104"));
    let short: String = HIS_LONG.chars().take(100).collect();
    // box KEY holding DRAFT, its marks in: the words marked, and the least of five frames with nothing changed, in ms
    let cost = |rt: &mut rune::runtime::Runtime<App>, key: &(usize, String), draft: &str| -> (usize, f64) {
        rt.update(|s, _| {
            let e = s.composers.entry(key.clone()).or_insert_with(|| rune::TextEdit::new(""));
            e.replace_text(draft);
            e.focused = true;
        });
        let _ = crate::marks::wait(draft);
        for _ in 0..10 {
            rt.frame(0.016);
        }
        let marked = crate::spell::marked_box(&rt.state, key, &rt.state.composers[key]).len();
        let least = (0..5)
            .map(|_| {
                let t = Instant::now();
                rt.invalidate();
                rt.frame(0.016);
                t.elapsed().as_secs_f64() * 1e3
            })
            .fold(f64::MAX, f64::min);
        (marked, least)
    };
    let (few, a) = cost(&mut rt, &key, &short);
    let (many, b) = cost(&mut rt, &key, HIS_LONG);
    rt.update(|s, _| _ = s.composers.remove(&key));
    // his comment on the first words of a post of N-1104, as the selection menu's Comment opens it
    rt.frame(0.016);
    let words = rt.elements().into_iter().find(|e| e.path.contains("/post-N-1104-") && e.role != "mark" && e.text.as_deref().is_some_and(|t| t.len() > 40)).expect("a post's words drawn");
    let text = words.text.clone().unwrap_or_default();
    let end = text.char_indices().nth(20).map_or(text.len(), |(i, _)| i);
    let path = words.path.clone();
    rt.update(move |_, tx| tx.select_text(path, 0..end));
    rt.frame(0.016);
    let sel = rt.selection().expect("a post's words selected");
    rt.update(move |s, tx| crate::ask::comment_on(s, 0, "thread:N-1104", &sel, tx));
    settle(&mut rt);
    let comment = (0, crate::message_box::COMMENT_PLACE.to_string());
    assert!(rt.state.notes.picked.is_some(), "his comment begun on a passage of N-1104");
    let (cfew, ca) = cost(&mut rt, &comment, &short);
    let (cmany, cb) = cost(&mut rt, &comment, HIS_LONG);
    println!("a frame with his draft in the box: {few} words marked {a:.1} ms, {many} words marked {b:.1} ms ({:.2} times)", b / a);
    println!("a frame with his draft in his comment's box: {cfew} words marked {ca:.1} ms, {cmany} words marked {cb:.1} ms ({:.2} times)", cb / ca);
    assert!(few <= 12 && many >= 40 && cfew <= 12 && cmany >= 40, "the drafts mark {few}, {many}, {cfew} and {cmany} words: the check needs few and many");
    assert!(b <= 2.0 * a, "{many} words marked: a frame takes {b:.1} ms, {:.1} times the {a:.1} ms of {few} marked", b / a);
    assert!(cb <= 2.0 * ca, "his comment, {cmany} words marked: a frame takes {cb:.1} ms, {:.1} times the {ca:.1} ms of {cfew} marked", cb / ca);
}

/// The post bodies built since the last call, by their sources (`N-1104.p12`), and the count cleared.
fn bodies_built() -> Vec<String> {
    crate::threads::BODIES.with_borrow_mut(std::mem::take)
}

/// A key in With you's box on a long thread builds the box, and no post's body again (N-1272 post 4, point a). With
/// you draws the rows in view: the newest posts at its foot, the first ones far up not built.
#[test]
fn a_key_in_with_you_builds_no_post_body_again() {
    let (_held, mut rt) = with_you_long();
    assert!(rt.find("post-N-1104-300").is_some(), "the newest post drawn, at the foot");
    assert!(rt.find("post-N-1104-1").is_none(), "the first, far up, not built");
    bodies_built();
    for c in ["o", "k", " "] {
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.into()), mods: Default::default() });
        rt.frame(0.016);
    }
    assert_eq!(in_the_box(&rt), Some("ok "));
    let built = bodies_built();
    assert!(built.is_empty(), "no body built again: {built:?}");
}

/// A key in With you's box on a long thread makes no post's words again, nor reads his comments once for each post
/// (the owner's N-1793, item 1: each key rewrote the words of the 36 posts built, links and labels, 2.4 ms, and read and
/// parsed every post of his in the thread once for each of them, 3 ms, to find whether a memo's version moved).
#[test]
fn a_key_in_with_you_makes_no_posts_words_again() {
    let (_held, mut rt) = with_you_long();
    crate::post::MADE.with(|n| n.set(0));
    for c in ["o", "k", " "] {
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.into()), mods: Default::default() });
        rt.frame(0.016);
    }
    assert_eq!(in_the_box(&rt), Some("ok "));
    assert_eq!(crate::post::MADE.with(|n| n.get()), 0, "no post's words made again");
    // an update that adds a post: its words made, and every other post's again, as the tickets moved (their labels)
    let ev = one_post_more(&rt);
    rt.update(|s, _| _ = s.projects[0].live.apply(ev));
    rt.frame(0.016);
    assert!(crate::post::MADE.with(|n| n.get()) > 0, "made again once the tickets moved");
}

/// An update from coordd that adds one post builds that post's body and no other (N-1272 post 4, point b), and it is
/// drawn, the box under it.
#[test]
fn an_update_adding_a_post_builds_that_post_alone() {
    let (_held, mut rt) = with_you_long();
    let ev = one_post_more(&rt);
    bodies_built();
    rt.update(|s, _| _ = s.projects[0].live.apply(ev));
    for _ in 0..30 {
        rt.frame(0.016);
    }
    assert!(rt.find("post-N-1104-301").is_some(), "the new post drawn");
    assert_eq!(bodies_built(), ["N-1104.p301"], "its body alone");
}

thread_local! {
    /// The window's polls run on this thread: one a frame.
    static POLLS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// The window's poll, counted in POLLS.
fn counted_poll(s: &mut App, clock: f64, tx: &mut rune::Tx) -> bool {
    POLLS.with(|n| n.set(n.get() + 1));
    crate::poll(s, clock, tx)
}

/// Frames while he types in With you (N-1272 post 15): 54 keys at his rate, 5.5 a second (his frames.jsonl on build
/// 24), 152 to 212 ms apart, into the box of the stand-in thread, his words cleaned (the spelling on, a coord that cleans
/// them) and the window's poll running. The loop runs as his window's does: a frame for each key, and one for each
/// timer the poll asks, at its own moment and at most one a frame interval after the last frame (rune `TIMER_FRAME`);
/// each frame polls once. On build 24 each frame of the pause asked for one more frame half a second later. The agents
/// are idle: the working marks' frames are not typing's.
#[test]
fn typing_in_with_you_draws_a_frame_per_key_and_per_tick() {
    let (_held, mut rt) = with_you_long();
    rt.update(|s, _| {
        s.spell.on = true;
        s.projects[0].protocol = coord_types::PROTOCOL;
        for a in &mut s.projects[0].live.agents {
            a.state = "idle".into();
        }
    });
    rt.frame(0.016);
    // the lists, built before the keys: SymSpell's as the window's start does it (marks::warm, off its thread; a test
    // has no start), the completions' own on the first completion; each one's time printed, as it costs once
    let t = Instant::now();
    let _ = crate::marks::symspell("warm");
    let sym = t.elapsed();
    let t = Instant::now();
    let _ = crate::suggest::complete("wai", &[]);
    println!("lists: SymSpell's built in {} ms, the completions' in {} ms", sym.as_millis(), t.elapsed().as_millis());
    rt.poll = Some(counted_poll);
    let words: Vec<char> = "the stills first, then the tables, dark and light both".chars().collect();
    let frame = Duration::from_micros(16_667).as_secs_f64();
    let began = Instant::now();
    let base = rt.clock;
    // a moment of the runtime's clock in real time, the two kept together as in a window
    let real = |clock: f64| began + Duration::from_secs_f64((clock - base).max(0.0));
    let (mut k, mut key_at) = (0, base);
    // the window's own time: its keys and frames, without the waits between them
    let mut busy = Duration::ZERO;
    POLLS.with(|n| n.set(0));
    while k < words.len() {
        match rt.wake_deadline().map(|d| d.max(rt.clock + frame)).filter(|t| *t < key_at) {
            Some(t) => {
                std::thread::sleep(real(t).saturating_duration_since(Instant::now()));
                let at = Instant::now();
                rt.frame(t - rt.clock);
                busy += at.elapsed();
            }
            None => {
                std::thread::sleep(real(key_at).saturating_duration_since(Instant::now()));
                let at = Instant::now();
                rt.key(rune::KeyEvent { key: rune::Key::Text(words[k].to_string()), mods: Default::default() });
                rt.frame((key_at - rt.clock).max(0.0));
                busy += at.elapsed();
                k += 1;
                key_at += (182 + (k as u64 * 37) % 61 - 30) as f64 / 1000.0;
            }
        }
    }
    let frames = POLLS.with(|n| n.get());
    let secs = began.elapsed().as_secs_f64();
    assert_eq!(in_the_box(&rt).map(|t| t.chars().collect::<Vec<_>>()), Some(words.clone()), "every letter in the box");
    // the bound: the keys; the cleaning pause's tick, one each half second (changed.rs); the saved draft's settle, whose
    // wakes, asked on each frame until the draft is written, fall on one moment: one or two frames each half second
    // (state.rs, the diagnostic job of N-412 p1756); and six for the slower ticks (the files' ten seconds)
    let (keys, halves) = (words.len() as u32, (secs / 0.5).ceil() as u32);
    let most = keys + halves + 2 * halves + 6;
    println!("typing in with you: {keys} keys in {secs:.1} s, {frames} frames (at most {most}), busy {} ms", busy.as_millis());
    assert!(frames <= most, "{frames} frames for {keys} keys in {secs:.1} s: more than the keys and the ticks ({most})");
}

/// TEXT typed into the box with the keyboard (here N-1104's in With you), a frame after each key.
pub(crate) fn type_in(rt: &mut rune::runtime::Runtime<App>, text: &str) {
    for c in text.chars() {
        press(rt, rune::Key::Text(c.to_string()));
    }
}

/// The words of the element keyed KEY on RT's page, when it is drawn and has words.
pub(crate) fn words_at(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text)
}

/// A click on the element keyed KEY on RT's page, then two frames.
pub(crate) fn click(rt: &mut rune::runtime::Runtime<App>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} drawn"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    rt.frame(0.016);
}

pub(crate) fn press(rt: &mut rune::runtime::Runtime<App>, key: rune::Key) {
    rt.key(rune::KeyEvent { key, mods: Default::default() });
    rt.frame(0.016);
}

/// His writing to an agent from its own box in With you (proto-reader N-1446 p394): his first word says it on, to the
/// agent with no thread; the box emptied before coordd answered, the off waits for that answer (lib.rs
/// `writing_changed`: the bridge asks each request on a thread of its own), and goes once it comes. Written again and
/// sent, the off waits for coordd's answer to his message, so the agent is not given what was kept for him before it
/// (consult-3 p442).
#[test]
fn his_writing_to_an_agent_says_its_off_after_coordds_answer_to_the_on() {
    let (_held, mut s) = app();
    let who = "paper-research".to_string();
    s.sel = Sel::Agent(who.clone());
    s.streams.which.insert((0, who.clone()), crate::streams::Which::You);
    // "+ New thread" chosen: With you's box is the agent's own (withyou.rs `view`)
    s.withyou.on.insert((0, who.clone()), None);
    s.spell.on = false;
    // a link whose requests are kept, to read what went
    let file = std::env::temp_dir().join(format!("yaaihh-writing-agent-{}.jsonl", std::process::id()));
    let mut keep = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", file.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(keep.stdin.take().unwrap()));
    let mut rt = window(s);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let f = rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-paper-research")).map(|e| e.rect).expect("the agent's box");
    let (x, y) = (f[0] + 12.0, f[1] + 12.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    let key = (0usize, who.clone());
    // coordd's answer to the one request on its way that WHAT picks out
    let answer = |rt: &mut rune::runtime::Runtime<App>, what: &dyn Fn(&crate::Pending) -> bool| {
        let ids: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| what(w)).map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 1, "one on its way");
        rt.state.take(0, crate::link::Incoming::Reply(ids[0], coord_types::Reply { ok: true, out: "M-12".into(), post: None }));
    };
    let writing = |w: &crate::Pending| matches!(w, crate::Pending::Writing { .. });
    let mailed = |w: &crate::Pending| matches!(w, crate::Pending::Mailed { .. });
    type_in(&mut rt, "ok");
    assert_eq!(rt.state.composers.get(&key).map(|e| e.text.as_str()), Some("ok"), "typed in the agent's box");
    assert!(rt.state.writing.contains(&key), "said on at his first word");
    press(&mut rt, rune::Key::Backspace);
    press(&mut rt, rune::Key::Backspace);
    assert!(rt.state.writing.contains(&key), "the off waits for the on's answer");
    answer(&mut rt, &writing);
    assert!(!rt.state.writing.contains(&key), "then the off goes");
    answer(&mut rt, &writing);
    // written again and sent: no off while his message is on its way
    type_in(&mut rt, "ship it");
    answer(&mut rt, &writing);
    rt.update(|s, _| crate::message_box::send(s, &crate::message_box::Target::Agent { project: 0, name: "paper-research".into() }));
    assert!(rt.state.composers.get(&key).is_none_or(|e| e.text.is_empty()), "the box emptied at Send");
    assert!(rt.state.writing.contains(&key) && !rt.state.projects[0].pending.values().any(writing), "no off before coordd has his message");
    answer(&mut rt, &mailed);
    assert!(!rt.state.writing.contains(&key), "said off once coordd has answered his message");
    rt.state.projects[0].link = None;
    let _ = keep.wait();
    let asked: Vec<serde_json::Value> = std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).collect();
    let said: Vec<String> = asked
        .iter()
        .filter_map(|r| match r["op"].as_str() {
            Some("writing") => Some(format!("{} {}", r["agent"].as_str().unwrap_or(""), r["on"])),
            Some("say" | "post") => Some("message".to_string()),
            _ => None,
        })
        .collect();
    assert_eq!(said, ["paper-research true", "paper-research false", "paper-research true", "message", "paper-research false"], "{asked:?}");
    assert!(asked.iter().filter(|r| r["op"] == "writing").all(|r| r.get("thread").is_none()), "to the agent, no thread: {asked:?}");
    let _ = std::fs::remove_file(&file);
}

/// A draft he leaves behind ends his writing, so it ends the hold it makes (paper-research N-1446 p2189, proto-mind
/// p2138): he writes while his box is drawn, holds words and has the keyboard, in the window he is in (lib.rs
/// `writing_changed`), each frame's poll asking again (`writing_checked`). He leaves the window: off; he comes back:
/// on. He goes to another page: off, though the box keeps its words and, in rune, its focus; he comes back to it: on.
/// The keyboard leaves the box (rune's `sync_input_focus` writes that into the box's editor): off. Each goes once
/// coordd has answered the one before.
#[test]
fn his_writing_ends_with_the_draft_he_leaves_and_starts_again_when_he_is_back() {
    let (_held, mut s) = app();
    let who = "paper-research".to_string();
    s.sel = Sel::Agent(who.clone());
    s.streams.which.insert((0, who.clone()), crate::streams::Which::You);
    s.withyou.on.insert((0, who.clone()), None);
    s.spell.on = false;
    let file = std::env::temp_dir().join(format!("yaaihh-writing-left-{}.jsonl", std::process::id()));
    let mut keep = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", file.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(keep.stdin.take().unwrap()));
    let mut rt = window(s);
    // the window's poll, which asks his writing again each frame
    rt.poll = Some(crate::poll);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let f = rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-paper-research")).map(|e| e.rect).expect("the agent's box");
    let (x, y) = (f[0] + 12.0, f[1] + 12.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    let key = (0usize, who.clone());
    // coordd's answer to his writing's one request on its way
    let answer = |rt: &mut rune::runtime::Runtime<App>| {
        let ids: Vec<u64> = rt.state.projects[0].pending.iter().filter(|(_, w)| matches!(w, crate::Pending::Writing { .. })).map(|(id, _)| *id).collect();
        assert_eq!(ids.len(), 1, "one on its way");
        rt.state.take(0, crate::link::Incoming::Reply(ids[0], coord_types::Reply { ok: true, out: "M-12".into(), post: None }));
    };
    let three_frames = |rt: &mut rune::runtime::Runtime<App>| {
        for _ in 0..3 {
            rt.frame(0.016);
        }
    };
    type_in(&mut rt, "the median");
    assert!(rt.state.writing.contains(&key), "on at his first word");
    answer(&mut rt);
    // the window left, as its `on_focus` says it (lib.rs), and back
    rt.update(|s, _| s.window_focused = false);
    three_frames(&mut rt);
    assert!(!rt.state.writing.contains(&key), "off once he left the window");
    answer(&mut rt);
    rt.update(|s, _| s.window_focused = true);
    three_frames(&mut rt);
    assert!(rt.state.writing.contains(&key), "on again when he is back");
    answer(&mut rt);
    // another page: the box is not drawn, its draft kept
    rt.update(|s, _| s.sel = Sel::Agents);
    three_frames(&mut rt);
    assert!(rt.state.composers.get(&key).is_some_and(|e| e.text == "the median" && e.focused), "the draft keeps its words and its focus");
    assert!(!rt.state.writing.contains(&key), "off once his box is not drawn");
    answer(&mut rt);
    rt.update(|s, _| s.sel = Sel::Agent("paper-research".into()));
    three_frames(&mut rt);
    assert!(rt.state.writing.contains(&key), "on again with his box drawn");
    answer(&mut rt);
    // the keyboard gone from the box, asked at once (no frame after, so rune moves nothing back)
    rt.update(|s, _| {
        if let Some(e) = s.composers.get_mut(&(0, "paper-research".to_string())) {
            e.focused = false;
        }
        s.writing_checked();
    });
    assert!(!rt.state.writing.contains(&key), "off once the keyboard left his box");
    answer(&mut rt);
    rt.state.projects[0].link = None;
    let _ = keep.wait();
    let asked: Vec<serde_json::Value> = std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).collect();
    let said: Vec<String> = asked.iter().filter(|r| r["op"] == "writing").map(|r| format!("{} {}", r["agent"].as_str().unwrap_or(""), r["on"])).collect();
    assert_eq!(
        said,
        ["paper-research true", "paper-research false", "paper-research true", "paper-research false", "paper-research true", "paper-research false"],
        "{asked:?}"
    );
    let _ = std::fs::remove_file(&file);
}

/// The glyphs the last frame drew.
fn glyphs(rt: &rune::runtime::Runtime<App>) -> usize {
    rt.instances.iter().filter(|i| i.params[1] == rune::render::KIND_GLYPH).count()
}

/// 0.2 line 27, the drawing (consult-3's cases, N-1396 p33): a word the local list completes shows the rest of it right
/// after the caret, in the frame of the key, drawn by rune and never typed. The box holds his letters alone (case 2);
/// Esc takes away the ghost's letters and nothing else, the caret where it was (cases 1 and 3). Tab, pressed as he
/// presses it (through the box's whole key chain: the card's keys, the mention's, then the ghost's; consult-3, N-1396
/// p74 note 2), puts the rest in with the caret after it.
#[test]
fn a_completion_stands_after_the_caret_and_is_never_typed() {
    let (_held, mut rt) = with_you_long();
    rt.update(|s, _| s.spell.on = true);
    // SymSpell's list built, as the spelling thread has it a few seconds after the window's start: until then nothing
    // is completed (suggest.rs `complete`, consult-3 N-1396 p74 (a))
    let _ = crate::marks::symspell("warm");
    // the window's poll, which tells typing from moving the caret (suggest.rs `typing`)
    rt.poll = Some(crate::poll);
    rt.frame(0.016);
    let key = (0usize, crate::message_box::thread_place("N-1104"));
    type_in(&mut rt, "we wait beca");
    let g = crate::suggest::ghost(&rt.state, &key).expect("a ghost for beca");
    assert_eq!((g.kind, g.word.as_str()), (crate::suggest::Kind::Complete, "beca"), "{g:?}");
    assert!(!g.rest().trim().is_empty(), "{g:?}");
    assert_eq!(in_the_box(&rt), Some("we wait beca"), "the ghost is never his text");
    let (with, caret) = (glyphs(&rt), rt.ime_area);
    press(&mut rt, rune::Key::Escape);
    assert!(crate::suggest::ghost(&rt.state, &key).is_none(), "Esc drops it");
    assert_eq!(in_the_box(&rt), Some("we wait beca"), "and nothing he typed");
    assert_eq!(with - glyphs(&rt), g.rest().chars().filter(|c| !c.is_whitespace()).count(), "the ghost's letters alone go");
    assert_eq!(rt.ime_area, caret, "the caret stays where it was");
    // the same start of a word typed again after a space: a word of its own, so its completion shows, and Tab takes it
    type_in(&mut rt, " beca");
    let g = crate::suggest::ghost(&rt.state, &key).expect("a ghost for the second beca");
    assert_eq!((g.kind, g.range.start), (crate::suggest::Kind::Complete, 13), "{g:?}");
    press(&mut rt, rune::Key::Tab);
    let whole = format!("we wait beca {}{}", g.word, g.rest());
    assert_eq!(in_the_box(&rt), Some(whole.as_str()), "Tab puts the rest in");
    assert_eq!(rt.state.composers[&key].focus, whole.len(), "the caret after it");
}

/// A marked word is not underlined while he types it (the line would come and go under each half-typed word); once
/// he ends it, it is. The caret moved back into it shows its best candidate under it at once, with no pointer and no
/// click, the box keeping the keyboard, and Tab puts it in (0.2 line 27, the owner: "for the suggestion i need to like
/// click instead of just move my cursor").
#[test]
fn a_marked_word_shows_its_fix_when_the_caret_moves_into_it() {
    if !crate::spell::available() {
        eprintln!("no dictionary here: skipped");
        return;
    }
    let (_held, mut rt) = with_you_long();
    rt.update(|s, _| s.spell.on = true);
    // the window's poll, which tells typing from moving the caret (suggest.rs `typing`)
    rt.poll = Some(crate::poll);
    rt.frame(0.016);
    let key = (0usize, crate::message_box::thread_place("N-1104"));
    let marked = |rt: &rune::runtime::Runtime<App>| crate::spell::marked_box(&rt.state, &key, &rt.state.composers[&key]);
    type_in(&mut rt, "we wiat");
    assert!(!marked(&rt).contains(&(3..7)), "not while he types it: {:?}", marked(&rt));
    type_in(&mut rt, " here");
    assert!(marked(&rt).contains(&(3..7)), "ended, it is: {:?}", marked(&rt));
    for _ in 0..7 {
        press(&mut rt, rune::Key::Left);
    }
    rt.frame(0.016);
    let g = crate::suggest::ghost(&rt.state, &key).expect("its fix at the caret");
    assert_eq!((g.kind, g.range.clone()), (crate::suggest::Kind::Fix, 3..7), "{g:?}");
    let row = format!("spell-card-0-{}", key.1);
    let under = rt
        .elements()
        .into_iter()
        .find(|e| e.role == "button" && e.path.contains(&row) && (e.text.as_deref() == Some(g.with.as_str()) || e.label.as_deref() == Some(g.with.as_str())))
        .map(|e| e.rect)
        .expect("its best candidate as a ghost under the word");
    let (_, wy) = rt.find(&format!("{}-at", crate::spell::mark_key(&key, &(3..7)))).expect("the word's place");
    assert!(under[1] > wy, "under the word: {under:?} against {wy}");
    assert!(rt.state.composers[&key].focused, "the box keeps the keyboard");
    // Tab as he presses it, through the box's whole key chain with the card open under the word (the card's keys and
    // the mention's come before the ghost's; consult-3, N-1396 p74 note 2)
    assert!(crate::spell::caret_card(&rt.state).is_some(), "the card is open");
    press(&mut rt, rune::Key::Tab);
    assert_eq!(in_the_box(&rt).map(String::from), Some(format!("we {} here", g.with)), "Tab puts it in");
    assert!(crate::spell::caret_card(&rt.state).is_none(), "and the ghost goes");
}

/// Stills of the ghost at the caret (0.2 line 27), on made-up text in With you, each cut to the box and each reached as
/// he reaches it, by keys in real time (`--script`, `--realtime`): a completion right after the caret as he types; a
/// marked word's fix under it once the caret is moved back into the word by the arrows; and in a thread holding
/// "thread", "there" and "these", `the` with nothing after it (a common word whole) and `thr` with the thread's
/// "thread" (consult-3, N-1396 p74 note 3). `STILL_OUT=DIR cargo test -p yaaihh --lib suggest_stills -- --ignored`.
#[test]
#[ignore]
fn suggest_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    // the checker and the lists built first, as the window's start builds them before he types
    let _ = crate::marks::wait("warm the checker");
    let _ = crate::suggest::complete("wai", &[]);
    fn with_you() -> App {
        let (_held, mut s) = app();
        let mut clock = 0;
        let mut t = marked_up(thread(1104, "owner", 40, &mut clock));
        if let Some(x) = t.posts.last_mut() {
            x.text = serde_json::from_value(json!("the thread is there, and these are its posts.")).unwrap();
        }
        s.projects[0].live.tickets.insert(1104, t);
        let who = "paper-research".to_string();
        s.sel = Sel::Agent(who.clone());
        s.streams.which.insert((0, who.clone()), crate::streams::Which::You);
        s.withyou.inside.insert((0, who), 1104);
        s.spell.on = true;
        s.projects[0].protocol = coord_types::PROTOCOL;
        let mut e = rune::TextEdit::new("");
        e.focused = true;
        s.composers.insert((0, crate::message_box::thread_place("N-1104")), e);
        s
    }
    // his pace: a key each 90 ms
    let (from, every) = (0.3, 0.09);
    let typed = |text: &str| json!({"t": from, "type": "type", "text": text, "every": every});
    let end = |text: &str| from + every * text.chars().count() as f64;
    let complete = "the stills first, then the tables, dark and light both, becau";
    // past the pause, then back into "wiat" by 18 Lefts: the caret between "wi" and "at"
    let fix = "the stills first, then wiat for the tables";
    let lefts = end(fix) + 1.2;
    let mut back = vec![typed(fix)];
    back.extend((0..18).map(|k| json!({"t": lefts + 0.08 * k as f64, "type": "key", "name": "Left"})));
    let shots = [
        ("ghost-complete", vec![typed(complete)], end(complete) + 0.15),
        ("ghost-fix", back, lefts + 18.0 * 0.08 + 0.5),
        ("the-none", vec![typed("we wait for the")], end("we wait for the") + 0.15),
        ("the-thr", vec![typed("we wait for the thr")], end("we wait for the thr") + 0.15),
    ];
    let crop = format!("composer-0-{}", crate::message_box::thread_place("N-1104"));
    for (name, script, at) in shots {
        let path = out.join(format!("{name}.json"));
        std::fs::write(&path, serde_json::to_string(&script).unwrap()).unwrap();
        let (snap, script, at) = (out.join(format!("{name}.png")).display().to_string(), path.display().to_string(), format!("{at:.2}"));
        let args: [&str; 11] = ["--snap", &snap, "--script", &script, "--realtime", "--crop", &crop, "--size", "1400x900", "--at", &at];
        // the window's poll and keys, as lib.rs gives them: `typing` reads the poll (suggest.rs), so without it a word
        // he is half-way through gets a fix offered under it
        let spec = rune::AppSpec { poll: Some(crate::poll), on_key: Some(crate::on_key), families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), with_you, crate::view) };
        rune::App::new(spec)
            .args(args.map(String::from).to_vec())
            .try_run()
            .unwrap();
    }
}

/// A post far up, sought (find.rs `seek`, as a search's jump does), is built and brought into With you's view, as on
/// the Threads page (N-1272: rows not in view are not built).
#[test]
fn a_post_far_up_in_with_you_is_brought_into_view() {
    let (_held, mut rt) = with_you_long();
    assert!(rt.find("post-N-1104-40").is_none(), "far up, not built");
    rt.update(|s, _| s.tickets.find.seek = Some((1104, 40, 3)));
    settle(&mut rt);
    let (_, y) = rt.find("post-N-1104-40").expect("its row is built");
    assert!((0.0..900.0).contains(&y), "in view: {y}");
}

/// Two stills of a 120-post thread, at the top and scrolled down (docket-edge, N-422.p9): the header is the list's first
/// row and scrolls away with the posts. `STILL_OUT=DIR cargo test --release long_thread_stills -- --ignored`.
#[test]
#[ignore]
fn long_thread_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let script = out.join("scroll.json");
    std::fs::write(&script, r#"[{"t":0.5,"type":"wheel","x":900,"y":450,"dy":-900},{"t":0.6,"type":"wheel","x":900,"y":450,"dy":-900}]"#).unwrap();
    fn init() -> App {
        let (_held, s) = test_app();
        long_thread_on(s, 120)
    }
    for (name, extra) in [("thread-top.png", vec![]), ("thread-scrolled.png", vec!["--script".to_string(), script.display().to_string()])] {
        let mut args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "2".into()];
        args.extend(extra);
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) })
            .args(args)
            .try_run()
            .unwrap();
    }
}

/// N-453: "nothing to answer" pressed with nothing typed reaches the request. With no pipe here, a press that arrives
/// leaves the "not sent" notice; a press that is lost leaves nothing.
#[test]
fn nothing_to_answer_is_pressed_with_the_box_empty() {
    let (_held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "one box", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "waits_on": [{"ticket": "N-450", "agent": "owner", "call": "asks", "post": 2}],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"]},
                  {"n": 2, "at": at(2), "by": "paper-research", "text": "Which one first? And the stills?", "told": ["owner"], "asks_open": true,
                   "asks": [{"at": [0, 16]}, {"at": [17, 32]}]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    let mut rt = window(s);
    for _ in 0..5 {
        rt.frame(0.016);
    }
    let found = rt.elements().into_iter().filter(|e| e.label.as_deref() == Some("nothing to answer") || e.text.as_deref() == Some("nothing to answer")).collect::<Vec<_>>();
    let e = found.iter().find(|e| e.role == "button").or(found.first()).expect("the button is drawn");
    let (x, y) = (e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    assert_eq!(rt.state.box_said.get(&(0, "thread:N-450".to_string())), Some(&Err("not sent: the box is not connected".to_string())), "the press reached the request");
    // said once: not also at the window's top (paper-research N-460.p4)
    assert_eq!(rt.state.notice, None);
    // and it says so at the card, where he pressed, not only at the thread's top
    rt.frame(0.016);
    let at: Vec<[f32; 4]> = rt.elements().into_iter().filter(|x| x.text.as_deref() == Some("not sent: the box is not connected")).map(|x| x.rect).collect();
    assert!(at.iter().any(|r| (r[1] - e.rect[1]).abs() < 80.0), "beside the button {:?}: {at:?}", e.rect);
    // coordd's answer to a press, refused or not, shows there too
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Nothing(0, "thread:N-450".into()) }, true, "Post 2 of N-450 asks nothing open.".into());
    rt.invalidate();
    rt.frame(0.016);
    assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some("Post 2 of N-450 asks nothing open.")));
}

/// Box step 2 (N-449.p6): on a thread whose post asks him things, the box answers the oldest such post by default, its
/// line says so in the accent, a chip puts its words under its ask's quote, and Send posts the answers to the asker.
#[test]
fn the_box_answers_the_oldest_post_that_asks_him() {
    use crate::message_box::{Target, Way};
    let (_held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"]},
                  {"n": 2, "at": at(2), "by": "paper-research", "text": "Which one first? And the stills?", "told": ["owner"], "asks_open": true,
                   "asks": [{"at": [0, 16]}, {"at": [17, 32]}]},
                  {"n": 3, "at": at(3), "by": "paper-research", "text": "Also: ship it?", "told": ["owner"], "asks_open": true, "asks": [{"at": [6, 14]}]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    let mut rt = window(s);
    for _ in 0..3 {
        rt.frame(0.016);
    }
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect();
    assert!(texts.iter().any(|t| t == "→ Windows window › answers paper-research's questions in post 2"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Send answers to paper-research"), "{texts:?}");
    assert!(texts.iter().any(|t| t == "nothing to answer"));
    // no line of its own under an ask: the one box is where he writes (N-449.p2)
    assert!(!texts.iter().any(|t| t == "your answer…"));
    // an option clicked (scene_card does what a chip does): that question's answer, in its own box under it (0.2 line
    // C15; consult-lead N-1335 p3, 1); the box's Send sends it under its ask's quote
    crate::threads::scene_card(&mut rt.state, "2: 2=both, dark first", false).unwrap();
    rt.invalidate();
    rt.frame(0.016);
    let place = ("thread:N-450").to_string();
    assert_eq!(crate::threads::answer_draft(&rt.state, 0, "N-450", 2, 1), "both, dark first");
    assert!(rt.state.composers.get(&(0, place.clone())).is_none_or(|e| e.text.is_empty()), "nothing put in the thread's box");
    let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect();
    assert!(texts.iter().any(|t| t == "Send 1 answer to paper-research"), "{texts:?}");
    let target = Target::Thread { project: 0, t: "N-450".into(), post: Some(2), way: Way::Here };
    match crate::message_box::request(&rt.state, &target).unwrap() {
        coord_types::Request::Post { text, to, answers, .. } => {
            assert_eq!(text, "> And the stills?\n\nboth, dark first");
            assert_eq!((to.as_deref(), answers), (Some("paper-research"), Some(2)));
        }
        r => panic!("{r:?}"),
    }
    // a chip of the later post: the box answers that one, chosen, with × back to the oldest
    crate::threads::scene_card(&mut rt.state, "3: 1=yes", false).unwrap();
    assert_eq!(rt.state.box_targets.get(&(0, place.clone())), Some(&Target::Thread { project: 0, t: "N-450".into(), post: Some(3), way: Way::Here }));
    assert_eq!(crate::message_box::line(&rt.state, &rt.state.box_targets[&(0, place)]), ("→ Windows window › answers paper-research's question in post 3".to_string(), true));
}

/// Steps 3 and 4 of the one box (N-449.p6): a thread with a step of his open aims its box at that step by default,
/// the line names it with its answers, its results are the box's actions (no Send, no second box on the card), and a
/// press answers with the box's words. The other ways send their requests from protocol 9 only: a subthread at a post
/// (`under`, to its author) and a new thread (`thread`); the foot's line says when his post there closes an ask
/// (`would_close`).
#[test]
fn the_box_answers_his_step_and_opens_threads_from_protocol_9() {
    use crate::message_box::{Target, Way};
    use coord_types::Request;
    let (_held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.protocol = coord_types::ASKS_CLOSED_PROTOCOL;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window",
        "status": {"is": "awaits", "agent": "owner", "call": "decide", "step": 1, "since": at(3), "taken": false, "refusals": 0, "forming": false,
                   "head": {"Variant": [["Go", []], ["Hold", [["why", "Text"]]]]}, "args": {}},
        "waits_on": [{"ticket": "N-450", "agent": "owner", "call": "decide"}],
        "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"], "told": ["paper-research"]},
                  {"n": 2, "at": at(2), "by": "paper-research", "text": "Ready. Ship it?", "told": ["owner"], "asks_open": true, "asks": [{"at": [7, 15]}],
                   "would_close": {"foot": "yes", "reply": "yes", "subthread": "routed"}},
                  {"n": 3, "at": at(3), "by": "owner", "text": "one more thing", "names": ["paper-research"]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    let mut rt = window(s);
    for _ in 0..3 {
        rt.frame(0.016);
    }
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect::<Vec<_>>();
    let place = "thread:N-450".to_string();
    // step 3: aimed at his step by default; its answers beside the box, no Send, no box on the card
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "→ Windows window › answers your step: Go or Hold"), "{t:?}");
    assert!(t.iter().any(|x| x == "Go") && t.iter().any(|x| x == "Hold"), "{t:?}");
    assert!(!t.iter().any(|x| x == "Send" || x == "Answer it in the box"), "{t:?}");
    let step = Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Step { s: "N-450".into() } };
    assert!(crate::message_box::request(&rt.state, &step).unwrap_err().starts_with("not sent: pick an answer"));
    // Hold with his words: the step's answer, its text the box's (coordd fills the one text field from it)
    rt.update(|s, _| _ = s.composers.insert((0, place.clone()), rune::TextEdit::new("the stills first")));
    let fs = vec![("why".to_string(), docket_machine::Type::Text)];
    match crate::ask::answer_request(&rt.state, 450, "Hold", &fs, "the stills first") {
        Some(Request::Answer { t, head, text }) => {
            assert_eq!((t.as_str(), head, text.as_deref()), ("N-450", json!({"head": "Hold"}), Some("the stills first")));
        }
        r => panic!("{r:?}"),
    }
    // pressed: no pipe here, so it says so under the box, and his words stay
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
    // his post 3, handed to nobody yet: Take back on it (protocol 9), while the pointer is on it, not before (reveal
    // little; consult-3, N-412 p2113)
    assert!(!texts(&rt).iter().any(|x| x == "Take back"), "not before the pointer is on it: {:?}", texts(&rt));
    rt.update(|s, _| s.tickets.hover_post = Some((450, 3)));
    rt.invalidate();
    rt.frame(0.016);
    assert!(texts(&rt).iter().any(|x| x == "Take back"), "{:?}", texts(&rt));
    let hold = rt.elements().into_iter().filter(|e| e.role == "button").find(|e| e.text.as_deref() == Some("Hold") || e.label.as_deref() == Some("Hold")).expect("Hold");
    let (x, y) = (hold.rect[0] + hold.rect[2] / 2.0, hold.rect[1] + hold.rect[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    assert!(texts(&rt).iter().any(|x| x == "not sent: the box is not connected"), "{:?}", texts(&rt));
    assert_eq!(rt.state.composers.get(&(0, place.clone())).map(|e| e.text.as_str()), Some("the stills first"));
    // taken: the box empties, its answer under it
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Step(0, place.clone(), 450) }, true, "Answered N-450.".into());
    rt.invalidate();
    rt.frame(0.016);
    assert!(rt.state.composers.get(&(0, place.clone())).is_none_or(|e| e.text.is_empty()));
    assert!(texts(&rt).iter().any(|x| x == "Answered N-450."));
    // the foot, chosen: the line says his post there answers post 2's question (the view's would_close)
    let foot = Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Here };
    assert_eq!(crate::message_box::line(&rt.state, &foot), ("→ Windows window; answers paper-research's question in post 2".to_string(), true));
    // step 4: a subthread at post 2 goes to its author, tied there; the line says it may answer (routed)
    let sub = Target::Thread { project: 0, t: "N-450".into(), post: Some(2), way: Way::Subthread };
    assert_eq!(crate::message_box::line(&rt.state, &sub), ("→ Windows window › post 2, as a subthread; may answer paper-research's question there".to_string(), true));
    rt.update(|s, _| _ = s.composers.insert((0, place.clone()), rune::TextEdit::new("which still?")));
    match crate::message_box::request(&rt.state, &sub).unwrap() {
        Request::Say { to, under, text, thread, .. } => assert_eq!((to.as_deref(), under.as_deref(), text.as_str(), thread), (Some("paper-research"), Some("N-450.p2"), "which still?", false)),
        r => panic!("{r:?}"),
    }
    rt.update(|s, _| _ = s.composers.insert((0, "project".into()), rune::TextEdit::new("a new one")));
    match crate::message_box::request(&rt.state, &Target::NewThread { project: 0 }).unwrap() {
        Request::Say { to, under, thread, .. } => assert_eq!((to, under, thread), (None, None, true)),
        r => panic!("{r:?}"),
    }
    // a coord before protocol 9 would refuse them: not sent, said why, and nothing offered to take back
    rt.update(|s, _| s.projects[0].protocol = coord_types::ASKS_CLOSED_PROTOCOL - 1);
    assert!(crate::message_box::request(&rt.state, &sub).unwrap_err().contains("needs a newer engine (9"));
    assert!(crate::message_box::request(&rt.state, &Target::NewThread { project: 0 }).unwrap_err().contains("needs a newer engine (9"));
    rt.invalidate();
    rt.frame(0.016);
    assert!(!texts(&rt).iter().any(|x| x == "Take back"));
}

/// The one box's stills for its hand-over (N-449.p2: judged by the look rules): `STILL_OUT=DIR cargo test box_stills --
/// --ignored`. Each place the box serves, with its line: his step answered from it, a post that asks him, his look at
/// his cleaned words, "being cleaned", an agent's page, and the pane with no thread open.
#[test]
#[ignore]
fn box_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn base() -> App {
        let (_held, mut s) = test_app();
        let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        s.spell.on = true;
        s.sel = Sel::Tickets;
        s
    }
    fn thread(s: &mut App, status: serde_json::Value, waits: serde_json::Value, posts: serde_json::Value) {
        let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "Windows window", "status": status, "waits_on": waits, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [], "posts": posts}))
        .unwrap();
        s.projects[0].live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
        s.tickets.shown = Some(450);
    }
    fn resting(s: &mut App) {
        thread(s, json!({"is": "resting"}), json!([]), json!([{"n": 1, "at": at(1), "by": "owner", "text": "the box, one for every place", "names": ["paper-research"], "told": ["paper-research"]},
            {"n": 2, "at": at(2), "by": "paper-research", "text": "Done: the box is in every place now.", "told": ["owner"]}]));
    }
    fn step() -> App {
        let mut s = base();
        thread(&mut s, json!({"is": "awaits", "agent": "owner", "call": "decide", "step": 1, "since": at(3), "taken": false, "refusals": 0, "forming": false,
                "head": {"Variant": [["Go", []], ["Hold", [["why", "Text"]]]]}, "args": {}}),
            json!([{"ticket": "N-450", "agent": "owner", "call": "decide"}]),
            json!([{"n": 1, "at": at(1), "by": "owner", "text": "the box, one for every place", "names": ["paper-research"], "told": ["paper-research"]},
                   {"n": 2, "at": at(2), "by": "paper-research", "text": "The box is ready for your window. Go, or hold it?", "told": ["owner"]},
                   {"n": 3, "at": at(3), "by": "owner", "text": "one more thing first", "names": ["paper-research"]}]));
        s.composers.insert((0, "thread:N-450".into()), rune::TextEdit::new("the stills first, then go"));
        s
    }
    fn asks() -> App {
        let mut s = base();
        thread(&mut s, json!({"is": "resting"}), json!([]),
            json!([{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"], "told": ["paper-research"]},
                   {"n": 2, "at": at(2), "by": "paper-research", "text": "Which one first? And the stills?", "told": ["owner"], "asks_open": true,
                    "asks": [{"at": [0, 16]}, {"at": [17, 32]}], "would_close": {"foot": "yes", "reply": "yes", "subthread": "routed"}}]));
        s
    }
    fn look() -> App {
        let mut s = base();
        resting(&mut s);
        let w: TicketView = serde_json::from_value(json!({"id": "N-460", "n": 460, "kind": "words", "opener": "owner", "ctx": {}, "form": {},
            "title": "words", "status": {"is": "awaits", "agent": "owner", "call": "look", "step": 2, "since": at(4), "taken": false, "refusals": 0, "forming": false,
                "head": {"Variant": [["Go", []], ["Use", [["text", "Text"]]], ["AsTyped", []]]},
                "args": {"text": "pls merge teh box adn ship it", "cleaned": "Please merge the box and ship it.", "doubts": [{"typed": "ship it", "would": ["ship it", "ship it now"]}]}},
            "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(4), "given": [], "posts": []}))
        .unwrap();
        s.projects[0].live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Ticket(w) });
        s.words.pending.push(crate::words::for_still((0, "thread:N-450".into()), "pls merge teh box adn ship it", Some("N-460"), std::time::Duration::from_secs(3)));
        s
    }
    fn cleaning() -> App {
        let mut s = base();
        resting(&mut s);
        s.words.pending.push(crate::words::for_still((0, "thread:N-450".into()), "pls merge teh box adn ship it", None, std::time::Duration::from_secs(3)));
        s
    }
    fn agent() -> App {
        let (_held, mut s) = test_app();
        let mut p = project();
        p.protocol = coord_types::PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        s.spell.on = true;
        s.sel = Sel::Agent("paper-research".into());
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.composers.insert((0, "paper-research".into()), rune::TextEdit::new("thanks, the box looks right"));
        s
    }
    fn intake() -> App {
        let mut s = base();
        resting(&mut s);
        s.tickets.shown = None;
        s
    }
    // the ghost under a marked word, as the pointer resting on it shows it: its best candidate, and ⋯ (N-815.p13)
    fn card() -> App {
        let mut s = agent();
        let key = (0usize, "paper-research".to_string());
        s.composers.insert(key.clone(), rune::TextEdit::new("I think it is wau so the box goes in tonight"));
        s.spell.card = Some(crate::spell::Card { key, range: 14..17, word: "wau".into(), keys: false });
        s.spell.on_word = true;
        s
    }
    // two words changed by the cleaning, dotted; his "thnik" under "think", a press puts it back (N-858.p3)
    fn changed() -> App {
        let mut s = agent();
        let key = (0usize, "paper-research".to_string());
        let typed = "I thnik the box is redy for tonight";
        let mut e = rune::TextEdit::new(typed);
        e.set_caret(typed.len());
        s.composers.insert(key.clone(), e);
        crate::spell::apply_changes(&mut s, &key, &[(2..7, "think".to_string()), (19..23, "ready".to_string())]);
        s.spell.card = Some(crate::spell::Card { key, range: 2..7, word: "think".into(), keys: false });
        s.spell.on_word = true;
        s
    }
    let shots: [(&str, fn() -> App); 8] = [("box-changed.png", changed), ("box-step.png", step), ("box-asks.png", asks), ("box-look.png", look), ("box-cleaning.png", cleaning), ("box-agent.png", agent), ("box-intake.png", intake), ("box-card.png", card)];
    for (name, init) in shots {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) })
            .args(args)
            .try_run()
            .unwrap();
    }
}

/// A message that could not be cleaned (coordd's cleaning call had no model that answered: his words go as they came,
/// and the words ticket carries a notice) says so under the box, faint, with Accounts one click away; his next message
/// there takes the line away (morning.md:136; consult-2 N-591.p37, 3; paper-research N-584.p59, 1).
#[test]
fn a_message_that_could_not_be_cleaned_says_so_and_leads_to_accounts() {
    let (_held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    let words = |n: u32, text: &str| -> TicketView {
        serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "words", "opener": "owner", "ctx": {}, "form": {},
            "title": "words", "status": {"is": "done", "result": {"head": "Send", "text": text}},
            "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(4), "given": [], "posts": []}))
        .unwrap()
    };
    p.live.apply(coord_types::Event { seq: 2, body: coord_types::Body::Ticket(words(460, "pls merge teh box")) });
    p.live.apply(coord_types::Event { seq: 3, body: coord_types::Body::Ticket(words(461, "Please merge the box.")) });
    p.live.apply(coord_types::Event { seq: 4, body: coord_types::Body::Ticket(words(462, "pls merge teh box")) });
    // coordd's notices on the words tickets whose cleaning failed: no model answered (N-460); a model answered and its
    // answer was refused (N-462)
    p.live.apply(coord_types::Event { seq: 5, body: coord_types::Body::Notices(vec![
        coord_types::Notice { ticket: Some("N-460".into()), text: "your words were filed as a thread without being sorted: the call failed (no model answered: every model failed)".into(), at: at(5) },
        coord_types::Notice { ticket: Some("N-462".into()), text: "your words were filed as a thread without being sorted: the call failed (not cleaned: refused; the call changed a code, id or name)".into(), at: at(5) },
    ]) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let key = (0usize, "thread:N-450".to_string());
    // a message cleaned as usual: no line
    s.words.pending.push(crate::words::for_still(key.clone(), "pls merge teh box", Some("N-461"), std::time::Duration::from_secs(3)));
    crate::words::step(&mut s);
    assert!(!s.words.unclean.contains_key(&key), "cleaned: nothing to say");
    // one whose cleaning a model answered and was refused: said, no Accounts
    s.words.pending.push(crate::words::for_still(key.clone(), "pls merge teh box", Some("N-462"), std::time::Duration::from_secs(3)));
    crate::words::step(&mut s);
    assert_eq!(s.words.unclean.get(&key), Some(&crate::words::Unclean::Other));
    s.box_said.clear();
    let mut rt = window(s);
    rt.frame(0.016);
    // the pane's words: the sidebar's foot holds the window's own Accounts on every page
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter(|e| !e.path.contains("/sidebar/")).filter_map(|e| e.text.or(e.label)).collect::<Vec<_>>();
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Sent as typed; not cleaned."), "{t:?}");
    assert!(!t.iter().any(|x| x == "Accounts"), "a refused answer is no account's: no Accounts: {t:?}");
    // one where no model answered: said, with Accounts beside it
    rt.state.words.pending.push(crate::words::for_still(key.clone(), "pls merge teh box", Some("N-460"), std::time::Duration::from_secs(3)));
    crate::words::step(&mut rt.state);
    assert_eq!(rt.state.words.unclean.get(&key), Some(&crate::words::Unclean::NoModel));
    rt.state.box_said.clear();
    rt.invalidate();
    rt.frame(0.016);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "Sent as typed; not cleaned."), "{t:?}");
    assert!(!t.iter().any(|x| x.contains("call failed") || x.contains("model answered")), "no engine words under the box: {t:?}");
    let a = rt.elements().into_iter().find(|e| !e.path.contains("/sidebar/") && (e.label.as_deref() == Some("Accounts") || e.text.as_deref() == Some("Accounts"))).expect("Accounts beside it").rect;
    let (x, y) = (a[0] + a[2] / 2.0, a[1] + a[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    assert_eq!(rt.state.sel, Sel::Ext("accounts".into(), String::new()), "one click opens Accounts");
    // his next message there takes the line away, and goes as it stands, at once: no words ticket (0.2 B9)
    rt.state.composers.insert(key.clone(), rune::TextEdit::new("and the stills"));
    let req = coord_types::Request::Post { kind: None, t: "N-450".into(), text: "and the stills".into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
    rt.state.projects[0].protocol = coord_types::PROTOCOL;
    rt.state.spell.on = true;
    let target = crate::message_box::Target::Thread { project: 0, t: "N-450".into(), post: None, way: crate::message_box::Way::Here };
    assert!(crate::message_box::sent(&mut rt.state, &target, req), "it goes");
    assert!(rt.state.words.pending.is_empty(), "nothing held in cleaning after Send");
    assert!(!rt.state.words.unclean.contains_key(&key), "a new message: the line goes");
    let _ = child.kill();
}

/// An agent's box says its message could not be cleaned, and its name in the bar ends before the bar's Spin off and ⋯.
#[test]
fn an_agents_box_says_it_too_and_its_name_never_runs_under_fork() {
    let (_held, mut s) = app();
    s.projects[0].down = None;
    s.projects[0].protocol = coord_types::PROTOCOL;
    s.sel = Sel::Agent("paper-research".into());
    s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
    s.words.unclean.insert((0, "paper-research".into()), crate::words::Unclean::NoModel);
    s.notice = Some("not sent: the box is not connected, and a notice long enough to fill the bar's right side".into());
    let mut rt = window(s);
    rt.frame(0.016);
    let els = rt.elements();
    assert!(els.iter().any(|e| e.text.as_deref() == Some("Sent as typed; not cleaned.")), "the agent's box says it");
    assert!(els.iter().any(|e| !e.path.contains("/sidebar/") && (e.label.as_deref() == Some("Accounts") || e.text.as_deref() == Some("Accounts"))), "with Accounts");
    let is = |e: &rune::runtime::ElementInfo, t: &str| e.label.as_deref() == Some(t) || e.text.as_deref() == Some(t);
    // Spin off on the bar, one press away (agents.rs `spin_presses`), and in the bar's ⋯ too
    let spin = els.iter().filter(|e| e.path.contains("main-pane/toolbar") && is(e, "Spin off")).map(|e| e.rect).min_by(|a, b| a[0].total_cmp(&b[0])).expect("Spin off on the bar");
    // the bar's "⋯" by its place: the trigger of zoom.rs's `bar-menu` (the box has a "⋯" of its own)
    let dots = els.iter().find(|e| is(e, "⋯") && e.path.contains("bar-menu-trigger")).map(|e| e.rect).unwrap_or_else(|| {
        panic!("the bar's ⋯: {:?}", els.iter().filter(|e| is(e, "⋯")).map(|e| (&e.path, e.rect)).collect::<Vec<_>>())
    });
    let name = els.iter().filter(|e| e.text.as_deref() == Some("paper-research")).min_by(|a, b| a.rect[1].total_cmp(&b.rect[1])).expect("the name in the bar").rect;
    assert!(name[1] < dots[1] + dots[3], "the name is in the bar: {name:?}");
    assert!(name[0] + name[2] <= spin[0], "the name ends before the bar's Spin off: name {name:?}, Spin off {spin:?}");
    assert!(name[0] + name[2] <= dots[0], "the name ends before the bar's ⋯: name {name:?}, ⋯ {dots:?}");
    // opened as his press on it opens it; a change from outside a handler is drawn only once the view is invalidated
    rt.state.bar_menu = true;
    rt.invalidate();
    rt.frame(0.016);
    let open = rt.elements();
    let menu = open.iter().any(|e| e.path.contains("bar-menu-menu") && e.label.as_deref().or(e.text.as_deref()).is_some_and(|t| t.starts_with("Spin off")));
    assert!(menu, "Spin off is in the bar's ⋯ too: {:?}", open.iter().filter(|e| e.path.contains("bar-menu")).map(|e| (&e.path, &e.text, &e.label)).collect::<Vec<_>>());
}

/// The one message box (N-449, N-460): at a thread's foot it says where the words go only when that says more than the
/// thread's name, a chosen target replaces that line until × puts the default back, and Send makes the target's request, its answer under the box.
#[test]
fn the_one_box_says_where_the_words_go_and_sends_there() {
    use crate::message_box::{Target, Way};
    let (_held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"]},
                  {"n": 2, "at": at(2), "by": "paper-research", "text": "Done, see the still.", "told": ["owner"]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    let mut rt = window(s);
    rt.frame(0.016);
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text).collect::<Vec<_>>();
    // on the thread's own page a line that only names it is not drawn (paper-research N-1446 p3207, 6)
    assert!(!texts(&rt).iter().any(|t| t.starts_with("→ ")), "no line that only names the thread: {:?}", texts(&rt));
    // Reply on post 2 (docket-edge4's button sets it): the line says so, and × is there
    rt.update(|s, _| crate::message_box::choose(s, Target::Thread { project: 0, t: "N-450".into(), post: Some(2), way: Way::Here }));
    rt.frame(0.016);
    assert!(texts(&rt).iter().any(|t| t == "→ Windows window › post 2"));
    assert!(texts(&rt).iter().any(|t| t == "×"));
    // written and sent: no pipe here, so his message stands as not sent, said once there, the box emptied (N-1396 p51)
    rt.update(|s, _| {
        s.composers.insert((0, "thread:N-450".into()), rune::TextEdit::new("thanks, merge it"));
        let t = crate::message_box::chosen(s, Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Here });
        crate::message_box::send(s, &t);
    });
    rt.frame(0.016);
    assert!(texts(&rt).iter().any(|t| t.ends_with("· not sent · the box is not connected")), "{:?}", texts(&rt));
    assert!(texts(&rt).iter().any(|t| t == "thanks, merge it"));
    assert!(!texts(&rt).iter().any(|t| t == "not sent: the box is not connected"), "said once");
    // taken by coordd: the box empties and its answer is under it
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Box(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 3.".into());
    rt.invalidate();
    rt.frame(0.016);
    assert!(rt.state.composers.get(&(0, "thread:N-450".into())).is_none_or(|e| e.text.is_empty()), "emptied");
    assert!(texts(&rt).iter().any(|t| t == "Posted on N-450 as post 3."));
    // × puts the foot back
    rt.update(|s, _| _ = s.box_targets.remove(&(0, "thread:N-450".into())));
    rt.frame(0.016);
    assert!(!texts(&rt).iter().any(|t| t.starts_with("→ ")), "the foot back: no line that only names the thread");
}

/// A marked word (paper-research N-815.p13, 1; N-487.p88): the pointer resting on "wau" for the system's hover time
/// shows its best candidate under it as a ghost, drawn in place from its first frame, no card; a click on the ghost
/// puts it in and the box keeps the keyboard. The ⋯ beside it opens the whole card: "wau" struck through, the best
/// candidate, `Add to dictionary`. It goes when the pointer leaves the word and on a key; Ctrl+. with the caret in the
/// word opens the whole card from the keyboard. No row of candidates under the box.
#[test]
fn a_marked_word_opens_its_card_on_hover() {
    if !crate::spell::available() {
        eprintln!("no dictionary here: skipped");
        return;
    }
    let (_held, mut s) = app();
    s.sel = Sel::Agent("paper-research".into());
    s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
    s.spell.on = true;
    let key = (0usize, "paper-research".to_string());
    s.composers.insert(key.clone(), rune::TextEdit::new("I think it is wau so"));
    let mut rt = window(s);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let marked = crate::spell::marked(&rt.state, 0, &rt.state.composers[&key]);
    assert!(marked.contains(&(14..17)), "wau is marked: {marked:?}");
    let region = format!("{}-at", crate::spell::mark_key(&key, &(14..17)));
    let shown = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().find(|e| e.label.as_deref() == Some(t) || e.text.as_deref() == Some(t)).map(|e| e.rect);
    let rows: Vec<String> = rt.elements().into_iter().filter(|e| e.path.contains("spell-candidates")).map(|e| e.path).collect();
    assert!(rows.is_empty() && shown(&rt, "Add to dictionary").is_none(), "no row under the box: {rows:?}");
    let (wx, wy) = rt.find(&region).expect("the word's place");
    // a pass over the word shorter than the hover time opens nothing
    rt.pointer_move(wx, wy);
    rt.frame(0.1);
    rt.pointer_move(wx, wy + 300.0);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.card.is_none(), "a pass does not open it");
    // resting on it does
    rt.pointer_move(wx, wy);
    rt.frame(0.016);
    rt.pointer_move(wx + 1.0, wy);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    rt.invalidate();
    rt.frame(0.016);
    assert_eq!(rt.state.spell.card.as_ref().map(|c| c.word.as_str()), Some("wau"), "resting on the word shows what is under it");
    let cands = crate::spell::candidates_at(&rt.state, 0, &rt.state.composers[&key], &(14..17));
    let best = cands.first().cloned().expect("a candidate for wau");
    let in_card = |rt: &rune::runtime::Runtime<App>, t: &str| {
        rt.elements().into_iter().find(|e| e.role == "button" && e.path.contains("spell-card") && (e.text.as_deref() == Some(t) || e.label.as_deref() == Some(t))).map(|e| e.rect)
    };
    let b = in_card(&rt, best.as_str()).expect("its best candidate, as a ghost under the word");
    assert!(b[1] > wy, "the ghost stands under the word: {b:?} against {wy}");
    assert!(shown(&rt, "Add to dictionary").is_none(), "the ghost alone, no card (paper-research N-815.p13)");
    for _ in 0..30 {
        rt.frame(0.016);
    }
    assert_eq!(in_card(&rt, best.as_str()), Some(b), "drawn where it stands from its first frame");
    // onto the ghost: it stays
    let (bx, by) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
    rt.pointer_move(bx, by);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.card.is_some(), "moving onto the ghost keeps it");
    rt.pointer_down(bx, by);
    rt.frame(0.016);
    rt.pointer_up(bx, by);
    for _ in 0..5 {
        rt.frame(0.016);
    }
    let e = &rt.state.composers[&key];
    assert_eq!(e.text, format!("I think it is {best} so"), "the best candidate put in");
    assert!(e.focused, "the box keeps the keyboard");
    assert!(rt.state.spell.card.is_none(), "and the ghost is gone");
    // ⋯ beside the ghost: the whole card, the other candidates and `Add to dictionary`
    rt.update(|s, _| _ = s.composers.insert((0, "paper-research".into()), rune::TextEdit::new("I think it is wau so")));
    rt.pointer_move(wx, wy - 200.0);
    for _ in 0..10 {
        rt.frame(0.016);
    }
    rt.pointer_move(wx, wy);
    rt.frame(0.016);
    rt.pointer_move(wx + 1.0, wy);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    rt.invalidate();
    rt.frame(0.016);
    let m = in_card(&rt, "⋯").expect("⋯ beside the ghost");
    let (mx, my) = (m[0] + m[2] / 2.0, m[1] + m[3] / 2.0);
    rt.pointer_move(mx, my);
    for _ in 0..10 {
        rt.frame(0.016);
    }
    rt.pointer_down(mx, my);
    rt.frame(0.016);
    rt.pointer_up(mx, my);
    for _ in 0..5 {
        rt.frame(0.016);
    }
    rt.invalidate();
    rt.frame(0.016);
    assert!(rt.state.spell.whole, "⋯ opens the whole card");
    let add = shown(&rt, "Add to dictionary").expect("the card's Add to dictionary");
    assert!(add[1] > wy, "the card stands under the word: {add:?} against {wy}");
    // the pointer leaving the word closes it
    rt.update(|s, _| _ = s.composers.insert((0, "paper-research".into()), rune::TextEdit::new("I think it is wau so")));
    rt.pointer_move(wx, wy - 200.0);
    for _ in 0..10 {
        rt.frame(0.016);
    }
    rt.pointer_move(wx, wy);
    rt.frame(0.016);
    rt.pointer_move(wx + 1.0, wy);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.card.is_some(), "open again on resting");
    rt.pointer_move(wx, wy - 200.0);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.card.is_none(), "the pointer leaving the word closes it");
    // a key closes it, and goes on to the box
    rt.update(|s, _| {
        let e = s.composers.get_mut(&(0, "paper-research".into())).unwrap();
        e.focused = true;
        e.set_caret(15);
        s.spell.card = Some(crate::spell::Card { key: (0, "paper-research".into()), range: 14..17, word: "wau".into(), keys: false });
    });
    rt.frame(0.016);
    rt.key(rune::KeyEvent { key: rune::Key::Right, mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.spell.card.is_none(), "a key closes it");
    // Ctrl+. with the caret in the word: its card, from the keyboard
    rt.key(rune::KeyEvent { key: rune::Key::Command('.'), mods: Default::default() });
    for _ in 0..5 {
        rt.frame(0.016);
    }
    assert_eq!(rt.state.spell.card.as_ref().map(|c| (c.word.as_str(), c.keys)), Some(("wau", true)), "Ctrl+. opens it");
    assert!(shown(&rt, "Add to dictionary").is_some(), "drawn");
    // Escape closes it and the box has the keyboard again
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
    for _ in 0..5 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.card.is_none(), "Escape closes the card opened from the keyboard");
    assert!(rt.state.composers[&key].focused, "and the box has the keyboard again");
    rt.key(rune::KeyEvent { key: rune::Key::Text("x".into()), mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.composers[&key].text.contains('x'), "typing goes on in the box");
}

/// The changed words (paper-research N-487.p90): the cleaning's answer lined up word by word goes into the box as one
/// step, the caret where he types, each changed word with its dotted mark; Ctrl+Z takes the batch back at once; the
/// card on a changed word shows his word struck and the word now in the box, and `Undo` puts his back for that word
/// only, which is then not marked again in this box. A word he chose in a marked word's card is a changed word too.
#[test]
fn the_cleaning_changes_words_each_with_undo() {
    if !crate::spell::available() {
        eprintln!("no dictionary here: skipped");
        return;
    }
    let (_held, mut s) = app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    s.sel = Sel::Agent("paper-research".into());
    s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
    s.spell.on = true;
    let key = (0usize, "paper-research".to_string());
    let typed = "I thnik it is teh box";
    let mut e = rune::TextEdit::new(typed);
    e.focused = true;
    e.set_caret(typed.len());
    s.composers.insert(key.clone(), e);
    let mut rt = window(s);
    for _ in 0..10 {
        rt.frame(0.016);
    }
    let clean = |rt: &mut rune::runtime::Runtime<App>| {
        rt.update(|s, _| {
            let key = (0usize, "paper-research".to_string());
            let text = s.composers[&key].text.clone();
            crate::changed::asked_for(s, &key, &text);
            crate::changed::answer(s, key, true, json!({"head": "Sure", "cleaned": "I think it is the box", "model": "m", "ms": 5}).to_string());
        });
        for _ in 0..10 {
            rt.frame(0.016);
        }
        rt.invalidate();
        rt.frame(0.016);
    };
    clean(&mut rt);
    let e = &rt.state.composers[&key];
    assert_eq!(e.text, "I think it is the box", "the cleaning's edits put in");
    assert_eq!(e.focus, e.text.len(), "the caret where he types");
    let changed = crate::spell::changed_at(&rt.state, &key, e);
    assert_eq!(changed, vec![(2..7, "thnik".to_string()), (14..17, "teh".to_string())], "each a changed word");
    let region = format!("{}-at", crate::spell::changed_key(&key, &(2..7)));
    assert!(rt.find(&region).is_some(), "the changed word's mark");
    // Ctrl+Z: the whole batch back in one step
    rt.key(rune::KeyEvent { key: rune::Key::Command('z'), mods: Default::default() });
    for _ in 0..3 {
        rt.frame(0.016);
    }
    assert_eq!(rt.state.composers[&key].text, typed, "Ctrl+Z takes the batch back at once");
    assert!(crate::spell::changed_at(&rt.state, &key, &rt.state.composers[&key]).is_empty(), "and the marks with it");
    assert!(!crate::changed::would_ask(&rt.state, &key, typed), "the batch he took back is not asked again after the pause");
    for _ in 0..120 {
        rt.frame(0.016);
    }
    assert_eq!(rt.state.composers[&key].text, typed, "after the pause the box still reads what he typed");
    // he types on after Ctrl+Z: the words of the batch he took back stay as he typed them (paper-research N-584.p34, 2)
    rt.update(|s, _| _ = crate::changed::step(s)); // the window's poll (a headless runtime does not poll)
    for c in " now".chars() {
        rt.key(rune::KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
        rt.frame(0.016);
    }
    assert_eq!(rt.state.composers[&key].text, format!("{typed} now"), "his typing goes on");
    rt.update(|s, _| {
        let key = (0usize, "paper-research".to_string());
        let text = s.composers[&key].text.clone();
        crate::changed::asked_for(s, &key, &text);
        crate::changed::answer(s, key, true, json!({"head": "Sure", "cleaned": "I think it is the box now", "model": "m", "ms": 5}).to_string());
    });
    rt.frame(0.016);
    assert_eq!(rt.state.composers[&key].text, "I thnik it is teh box now", "after the pause the batch stays taken back");
    // a fresh box for the rest
    rt.update(|s, _| {
        let key = (0usize, "paper-research".to_string());
        crate::spell::forget_left(s, &key);
        let e = s.composers.get_mut(&key).unwrap();
        *e = rune::TextEdit::new("I thnik it is teh box");
        e.focused = true;
        e.set_caret(e.text.len());
    });
    rt.frame(0.016);
    // again; then his "thnik" put back from under "think" (the owner, N-858.p3: "shows the original text")
    clean(&mut rt);
    let (wx, wy) = rt.find(&region).expect("the changed word's place");
    rt.pointer_move(wx, wy);
    rt.frame(0.016);
    rt.pointer_move(wx + 1.0, wy);
    for _ in 0..40 {
        rt.frame(0.016);
    }
    rt.invalidate();
    rt.frame(0.016);
    // a ghost standing under its word: one on its way out (`exiting`, its fade) no longer stands there
    let ghost = |rt: &rune::runtime::Runtime<App>, t: &str| {
        rt.elements().into_iter().find(|e| !e.exiting && e.role == "button" && e.path.contains("spell-card") && (e.text.as_deref() == Some(t) || e.label.as_deref() == Some(t))).map(|e| e.rect)
    };
    let u = ghost(&rt, "thnik").expect("his word, as a ghost under the changed word");
    assert!(u[1] > wy, "under the word: {u:?} against {wy}");
    let (ux, uy) = (u[0] + u[2] / 2.0, u[1] + u[3] / 2.0);
    rt.pointer_move(ux, uy);
    rt.frame(0.016);
    rt.pointer_down(ux, uy);
    rt.frame(0.016);
    rt.pointer_up(ux, uy);
    for _ in 0..5 {
        rt.frame(0.016);
    }
    let e = &rt.state.composers[&key];
    assert_eq!(e.text, "I thnik it is the box", "a press on his word puts back that word only");
    assert_eq!(crate::spell::changed_at(&rt.state, &key, e), vec![(14..17, "teh".to_string())], "the other stays changed");
    assert!(!crate::spell::marked_box(&rt.state, &key, e).contains(&(2..7)), "his word is not marked again in this box");
    // the cleaning does not change it again
    clean(&mut rt);
    assert!(rt.state.composers[&key].text.starts_with("I thnik"), "nor changed again");
    // Ctrl+. with the caret in a changed word: his word under it, put back from the keyboard
    rt.update(|s, _| {
        let e = s.composers.get_mut(&(0, "paper-research".into())).unwrap();
        e.focused = true;
        e.set_caret(15);
        s.spell.card = None;
    });
    rt.frame(0.016);
    rt.key(rune::KeyEvent { key: rune::Key::Command('.'), mods: Default::default() });
    for _ in 0..5 {
        rt.frame(0.016);
    }
    assert_eq!(rt.state.spell.card.as_ref().map(|c| c.word.as_str()), Some("the"), "Ctrl+. opens a changed word's card");
    assert!(ghost(&rt, "teh").is_some(), "with his word");
    rt.key(rune::KeyEvent { key: rune::Key::Escape, mods: Default::default() });
    rt.frame(0.016);
    assert!(rt.state.spell.card.is_none(), "Escape closes a changed word's card opened from the keyboard");
    assert!(rt.state.composers[&key].focused, "and gives the box the keyboard back");
    // a click in a changed word puts the caret in it: his word under it, with no card open; the caret after the word,
    // nothing (the owner, N-858.p3: "clicking on the changed word … shows the original text")
    rt.invalidate();
    rt.frame(0.016);
    let g = ghost(&rt, "teh").expect("the caret in the changed word shows his word under it");
    let (_, ry) = rt.find(&format!("{}-at", crate::spell::changed_key(&key, &(14..17)))).expect("the changed word's place");
    assert!(g[1] > ry, "under the word: {g:?} against {ry}");
    rt.update(|s, _| {
        let e = s.composers.get_mut(&(0, "paper-research".into())).unwrap();
        let end = e.text.len();
        e.set_caret(end);
    });
    rt.invalidate();
    rt.frame(0.016);
    let st = &rt.state;
    let e = &st.composers[&key];
    let why = format!(
        "card {:?}, caret card {:?}, on card {}, on word {}, caret {}..{} of {}, focused {}",
        st.spell.card,
        crate::spell::caret_card(st),
        st.spell.on_card,
        st.spell.on_word,
        e.anchor,
        e.focus,
        e.text.len(),
        e.focused
    );
    let leaving = rt.elements().into_iter().any(|e| e.exiting && e.path.contains("spell-card") && (e.text.as_deref() == Some("teh") || e.label.as_deref() == Some("teh")));
    assert!(ghost(&rt, "teh").is_none(), "the caret after it: nothing under it ({why}, one leaving {leaving})");
    // a word chosen in a marked word's card is a changed word
    rt.update(|s, _| {
        let key = (0usize, "paper-research".to_string());
        s.composers.insert(key.clone(), rune::TextEdit::new("it is wau"));
        crate::spell::choose(s, &key, 6..9, "way");
    });
    rt.frame(0.016);
    assert_eq!(crate::spell::changed_at(&rt.state, &key, &rt.state.composers[&key]), vec![(6..9, "wau".to_string())], "his choice is a changed word");
    // two typos side by side are one edit: one of them put back keeps the pair (consult-2 N-584.p43, 3)
    assert!(crate::spell::left_in(&rt.state, &key, "thnik it"), "a pair with a put-back word in it is left");
    // sent: the texts whose edits went in are forgotten, so typing one again later is not a Ctrl+Z (p43, 1)
    assert!(crate::changed::put_count(&rt.state, &key) > 0, "the cleaning's texts are remembered while he writes");
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Box(0, "paper-research".into()) }, true, "Posted N-1.p1.".into());
    assert_eq!(crate::changed::put_count(&rt.state, &key), 0, "and forgotten at Send");
}

/// A pause sends only the paragraphs not cleaned yet (paper-research N-584.p46, item 6): after the first paragraph is
/// cleaned and he writes a second, the second alone goes, and its edits land in the box at its place.
#[test]
fn the_pause_sends_only_the_paragraphs_that_changed() {
    let (_held, mut s) = app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    s.spell.on = true;
    let key = (0usize, "paper-research".to_string());
    let first = "I thnik it is teh box";
    s.composers.insert(key.clone(), rune::TextEdit::new(first));
    assert_eq!(crate::changed::to_ask(&s, &key, first), Some(0..first.len()), "a new box: all of it");
    crate::changed::asked_for(&mut s, &key, first);
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "I think it is the box", "model": "m", "ms": 5}).to_string());
    assert_eq!(s.composers[&key].text, "I think it is the box");
    let text = "I think it is the box\n\nsend it adn ship";
    s.composers.get_mut(&key).unwrap().text = text.to_string();
    let part = crate::changed::to_ask(&s, &key, text).expect("the new paragraph");
    assert_eq!(&text[part], "send it adn ship", "only the paragraph he wrote since");
    crate::changed::asked_for(&mut s, &key, text);
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "send it and ship", "model": "m", "ms": 5}).to_string());
    let e = &s.composers[&key];
    assert_eq!(e.text, "I think it is the box\n\nsend it and ship", "its edit at its place");
    assert_eq!(crate::spell::changed_at(&s, &key, e).last(), Some(&(31..34, "adn".to_string())), "a changed word there");
    assert_eq!(crate::changed::to_ask(&s, &key, &e.text.clone()), None, "nothing left to send");
    // `Undo` on a pair changed as one ("wnat ot" → "want to") holds when he types on (consult-2 N-584.p48)
    let t = "I wnat ot go";
    s.composers.insert(key.clone(), rune::TextEdit::new(t));
    crate::changed::asked_for(&mut s, &key, t);
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "I want to go", "model": "m", "ms": 5}).to_string());
    assert_eq!(s.composers[&key].text, "I want to go");
    let (r, his) = crate::spell::changed_at(&s, &key, &s.composers[&key]).into_iter().find(|(_, h)| h == "wnat ot").expect("the pair is one changed word");
    crate::spell::put_back(&mut s, &key, r, &his);
    assert_eq!(s.composers[&key].text, t, "Undo puts the pair back");
    let t2 = "I wnat ot go now";
    s.composers.get_mut(&key).unwrap().text = t2.to_string();
    crate::changed::asked_for(&mut s, &key, t2);
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "I want to go now", "model": "m", "ms": 5}).to_string());
    assert_eq!(s.composers[&key].text, t2, "the pause after does not change the pair again");
    // sent: the same words in a later message are cleaned again (p48, UX)
    crate::threads::replied(&mut s, crate::Pending::TicketDone { clear: crate::threads::Cleared::Box(0, "paper-research".into()) }, true, "Posted N-1.p1.".into());
    assert!(crate::changed::would_ask(&s, &key, "I thnik it is teh box"), "a text once asked is asked again in a later message");
}

/// The cleaning came back while he typed on (the owner: "the being cleaned basically is too late", N-107.p15): its
/// edits go in where his words stand as sent, the caret stays where he types, and the next pause asks only for what
/// he wrote since.
#[test]
fn the_cleaning_lands_while_he_types_on() {
    let (_held, mut s) = app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    s.spell.on = true;
    let key = (0usize, "paper-research".to_string());
    let set = |s: &mut App, t: &str| {
        let mut e = rune::TextEdit::new(t);
        e.focused = true;
        e.set_caret(t.len());
        s.composers.insert(key.clone(), e);
    };
    let sent = "I thnik it is teh box";
    set(&mut s, sent);
    crate::changed::asked_for(&mut s, &key, sent);
    set(&mut s, "I thnik it is teh box\n\nsend it adn");
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "I think it is the box", "model": "m", "ms": 4654}).to_string());
    let e = &s.composers[&key];
    assert_eq!(e.text, "I think it is the box\n\nsend it adn", "its edits went in while he typed on");
    assert_eq!(e.focus, e.text.len(), "the caret where he types");
    assert_eq!(crate::spell::changed_at(&s, &key, e).len(), 2, "each a changed word, with its Undo");
    let now = e.text.clone();
    let part = crate::changed::to_ask(&s, &key, &now).expect("what he wrote since");
    assert_eq!(&now[part], "send it adn", "the next pause asks only for the paragraph he wrote since");
    assert!(crate::changed::would_ask(&s, &key, &now), "and it does ask");
    assert!(!crate::changed::would_ask(&s, &key, "I thnik it is teh box\n\nsend it adn"), "a batch he takes back with Ctrl+Z stays taken back");
    // he went back to fix a word it was cleaning, and typed on: his fix stands, the word after it is cleaned
    let sent = "we sohuld go nwo";
    set(&mut s, sent);
    crate::changed::asked_for(&mut s, &key, sent);
    set(&mut s, "we shuold go nwo please");
    crate::changed::answer(&mut s, key.clone(), true, json!({"head": "Sure", "cleaned": "we should go now", "model": "m", "ms": 4654}).to_string());
    assert_eq!(s.composers[&key].text, "we shuold go now please");
    let now = s.composers[&key].text.clone();
    assert!(crate::changed::to_ask(&s, &key, &now).is_some(), "the paragraph he changed is asked again");
    // he sent the message to the agent while it was out (its chat's Send, at once: 0.2 B9) and began the next with the
    // same words: the answer is for a message that is gone and goes nowhere; the new one is asked at its own pause, and
    // its own answer lands (consult-5, N-412 post 2029)
    let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    let sent = "I know its";
    set(&mut s, sent);
    let old = crate::changed::asked_for(&mut s, &key, sent);
    s.send(0, "paper-research", false);
    assert!(!s.composers.contains_key(&key), "it went at once, nothing held");
    assert!(s.words.pending.is_empty(), "no words ticket");
    let next = "I know its red";
    set(&mut s, next);
    assert_eq!(crate::changed::to_ask(&s, &key, next), Some(0..next.len()), "the new message is asked whole");
    let new = crate::changed::asked_for(&mut s, &key, next);
    crate::changed::replied(&mut s, key.clone(), old, true, json!({"head": "Sure", "cleaned": "I know it's", "model": "m", "ms": 4654}).to_string());
    let e = &s.composers[&key];
    assert_eq!(e.text, next, "the sent message's answer changes nothing in the next one");
    assert!(crate::spell::changed_at(&s, &key, e).is_empty(), "and marks nothing");
    crate::changed::replied(&mut s, key.clone(), new, true, json!({"head": "Sure", "cleaned": "I know it's red", "model": "m", "ms": 4654}).to_string());
    assert_eq!(s.composers[&key].text, "I know it's red", "its own answer lands");
    // the comment box: an answer still out when the comment is added lands on no later comment
    let picked = crate::ask::Picked { target: crate::message_box::thread_place("N-450"), source: "paper-research@0".into(), label: "a post".into(), quote: "a passage".into(), at: Vec::new(), pi: 0, anchor: None, block: None, pane: None, texts: Vec::new() };
    crate::ask::write(&mut s, picked.clone(), "I know its");
    let ckey = (0usize, crate::message_box::COMMENT_PLACE.to_string());
    let old = crate::changed::asked_for(&mut s, &ckey, "I know its");
    crate::ask::unwrite(&mut s);
    crate::ask::write(&mut s, picked, "I know its red");
    crate::changed::replied(&mut s, ckey.clone(), old, true, json!({"head": "Sure", "cleaned": "I know it's", "model": "m", "ms": 4654}).to_string());
    assert_eq!(s.composers[&ckey].text, "I know its red", "the added comment's answer changes nothing in the next");
    s.projects[0].link = None;
    let _ = child.wait();
}

/// Typing in and out of a marked word moves nothing: there is no row of candidates under the box any more (its card
/// opens on hover), so the field and what is under it stay where they are (paper-research N-433.p69, N-487.p88).
#[test]
fn typing_in_a_marked_word_moves_nothing() {
    if !crate::spell::available() {
        eprintln!("no dictionary here: skipped");
        return;
    }
    let (_held, mut s) = app();
    s.sel = Sel::Agent("paper-research".into());
    s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
    s.spell.on = true;
    s.composers.insert((0, "paper-research".into()), rune::TextEdit::new("the wa"));
    let mut rt = window(s);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let field_at = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-paper-research")).map(|e| e.rect).expect("the composer's field");
    let switch_at = |rt: &rune::runtime::Runtime<App>| {
        rt.elements().into_iter().find(|e| (e.label.as_deref() == Some("Send") || e.text.as_deref() == Some("Send")) && e.path.contains("composer-actions-0-paper-research")).map(|e| e.rect).expect("the box's Send")
    };
    let f = field_at(&rt);
    let (x, y) = (f[0] + f[2] - 6.0, f[1] + 12.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.016);
    let (field0, switch0) = (field_at(&rt), switch_at(&rt));
    rt.key(rune::KeyEvent { key: rune::Key::Text("u".into()), mods: Default::default() });
    for _ in 0..5 {
        rt.frame(0.016);
    }
    let keep = rt.elements().into_iter().any(|e| e.role == "button" && (e.label.as_deref() == Some("Add to dictionary") || e.text.as_deref() == Some("Add to dictionary")));
    assert!(!keep, "the caret in a marked word opens nothing by itself");
    let (field1, switch1) = (field_at(&rt), switch_at(&rt));
    rt.key(rune::KeyEvent { key: rune::Key::Text(" ".into()), mods: Default::default() });
    for _ in 0..5 {
        rt.frame(0.016);
    }
    let (field2, switch2) = (field_at(&rt), switch_at(&rt));
    assert!(field0 == field1 && field1 == field2, "the field stays where it is");
    assert!(switch0 == switch1 && switch1 == switch2, "what is under it stays where it is");
}

/// The way back from `Add to dictionary` (paper-research N-515.p1): "Words you added" lists his word, and Take out
/// takes it off the list.
#[test]
fn the_words_he_added_are_listed_and_taken_out() {
    let (_held, mut s) = app();
    crate::spell::keep(&mut s, "Grisette");
    s.spell.words_open = true;
    let mut rt = window(s);
    for _ in 0..30 {
        rt.frame(0.016);
    }
    let shown = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().find(|e| e.label.as_deref() == Some(t) || e.text.as_deref() == Some(t)).map(|e| e.rect);
    assert!(shown(&rt, "grisette").is_some(), "his word is listed");
    let b = shown(&rt, "Take out").expect("Take out beside it");
    let (x, y) = (b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
    rt.pointer_move(x, y);
    rt.frame(0.016);
    rt.pointer_down(x, y);
    rt.frame(0.016);
    rt.pointer_up(x, y);
    for _ in 0..60 {
        rt.frame(0.016);
    }
    assert!(rt.state.spell.added().is_empty(), "taken out of his words");
    let left: Vec<String> = rt.elements().into_iter().filter(|e| e.path.contains("added-word")).map(|e| format!("{} {:?} {:?} {:?}", e.path, e.role, e.text, e.rect)).collect();
    assert!(shown(&rt, "grisette").is_none() && shown(&rt, "Take out").is_none(), "and off the list: {left:#?}");
}

/// Stills of the agent page's box as he types his N-515 message (paper-research N-483.p10, N-487.p51): the caret in
/// "dictiony", its candidates and `Add to dictionary` under the box; then the whole message, the words still marked
/// underlined. The project is named yaaihh, as his is. `STILL_OUT=DIR cargo test --profile quick
/// n515_stills -- --ignored`.
#[test]
#[ignore]
fn n515_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let script = out.join("type.json");
    const FIRST: &str = "oh yeah and for yaaihh add some way for it tatleast have add htis to dictiony";
    let first = FIRST;
    let rest = " button for eg yaaihh";
    std::fs::write(
        &script,
        json!([{"t": 0.5, "type": "tap", "key": "cf-0-paper-research"}, {"t": 0.8, "type": "key", "name": "End"},
            {"t": 4.5, "type": "type", "text": rest, "every": 0.02}])
        .to_string(),
    )
    .unwrap();
    fn init() -> App {
        let (_held, mut s) = test_app();
        let mut p = project();
        p.name = "yaaihh".into();
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Agent("paper-research".into());
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.spell.on = true;
        s.composers.insert((0, "paper-research".into()), rune::TextEdit::new(FIRST));
        s
    }
    // the spelling thread's marks of both texts first (an unoptimised build reads SymSpell's list in about 10 s), and
    // the first half in the box from the start: a headless window is not woken when marks come, and typed letter by
    // letter it asks for more texts than the KEPT marks, so it would show only the machine's checker's candidates
    crate::marks::wait(first);
    crate::marks::wait(&format!("{first}{rest}"));
    for (name, at) in [("n515-in-dictiony.png", "4.0"), ("n515-whole.png", "7.0")] {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), script.display().to_string()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) })
            .args(args)
            .try_run()
            .unwrap();
    }
}

/// The agent page's box (docket-edge6, N-815.p10): it says "To paper-research" and nothing else; in sight beside it
/// only "?", "⋯", the mic and Send, and Stop (Esc) apart at the row's left end while the agent is in a turn. Spelling and the
/// other switches are the items of the menu "⋯" opens, over the chat: nothing in the chat moves (consult-2, N-815.p23;
/// paper-research p24). It offers no "When done" while the agent is in a turn: what he sends then waits for the turn's
/// end, as every post to it does, and goes with no `wait` of its own (paper-research N-1446 p2112, 1). A press on "?"
/// shows its line.
#[test]
fn the_agents_box_shows_whom_and_its_switches_behind_dots() {
    let (_held, mut s) = app();
    s.sel = Sel::Agent("paper-research".into());
    s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
    let mut rt = window(s);
    rt.frame(0.016);
    let seen = |rt: &rune::runtime::Runtime<App>, t: &str| rt.elements().into_iter().any(|e| e.text.as_deref() == Some(t) || e.label.as_deref() == Some(t));
    let how = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().any(|e| e.text.as_deref().is_some_and(|t| t.contains("Cmd/Ctrl+Enter")));
    let find = |rt: &rune::runtime::Runtime<App>, t: &str| {
        rt.elements()
            .into_iter()
            .find(|e| (e.text.as_deref() == Some(t) || e.label.as_deref() == Some(t)) && e.path.contains("composer-actions-0-paper-research"))
            .unwrap_or_else(|| panic!("the box's {t}: {:?}", rt.elements().into_iter().filter(|e| e.path.contains("composer-actions")).map(|e| (e.path, e.text, e.label)).collect::<Vec<_>>()))
            .rect
    };
    let field_at = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().find(|e| e.role == "textbox" && e.path.contains("composer-0-paper-research")).map(|e| e.rect).expect("the composer's field");
    let press = |rt: &mut rune::runtime::Runtime<App>, at: [f32; 4]| {
        let (x, y) = (at[0] + at[2] / 2.0, at[1] + at[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..3 {
            rt.frame(0.016);
        }
    };
    assert!(seen(&rt, "To paper-research"), "whom");
    assert!(!how(&rt), "the how-to is behind ?");
    assert!(seen(&rt, "?") && seen(&rt, "Send"));
    assert!(!seen(&rt, "Spelling"), "Spelling behind ⋯");
    let (esc, send) = (find(&rt, "Stop (Esc)"), find(&rt, "Send"));
    let (dots, ask) = (find(&rt, "⋯"), find(&rt, "?"));
    assert!(esc[0] < send[0] && esc[0] + esc[2] < ask[0] && esc[0] + esc[2] < dots[0], "Esc apart at the left end: {esc:?} {ask:?} {dots:?}");
    let field0 = field_at(&rt);
    press(&mut rt, dots);
    assert!(seen(&rt, "Spelling"), "⋯ shows its items");
    assert!(!rt.elements().into_iter().any(|e| e.text.as_deref().is_some_and(|t| t.starts_with("When done")) || e.label.as_deref().is_some_and(|l| l.starts_with("When done"))), "and no When done, the agent in its turn");
    assert!(matches!(&rt.state.box_open, Some((_, crate::message_box::Open::More))));
    assert_eq!(field_at(&rt), field0, "nothing in the chat moves");
    assert_eq!(find(&rt, "Send"), send, "nor the row");
    // an item: Spelling turns over and the menu closes
    let spelling = rt.state.spell.on;
    let item = rt.elements().into_iter().find(|e| e.role == "menuitem" && (e.label.as_deref().is_some_and(|l| l.starts_with("Spelling")) || e.text.as_deref() == Some("Spelling"))).map(|e| e.rect);
    let item = item.unwrap_or_else(|| rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Spelling")).map(|e| e.rect).expect("the Spelling item"));
    press(&mut rt, item);
    assert_eq!(rt.state.spell.on, !spelling, "Spelling turned over");
    assert!(rt.state.box_open.is_none() && !seen(&rt, "Spelling"), "the menu closed");
    // what he sends meanwhile goes as any message, with no wait of its own: coordd keeps it for the turn's end
    rt.state.composers.insert((0, "paper-research".to_string()), rune::TextEdit::new("check the lock"));
    let sent = rt.state.composed(0, "paper-research").map(|(r, _)| r);
    assert!(matches!(sent, Some(coord_types::Request::Say { wait: false, .. } | coord_types::Request::Post { wait: false, .. })), "{sent:?}");
    // "?" on a press
    let ask = find(&rt, "?");
    press(&mut rt, ask);
    assert!(how(&rt), "a press on ? shows its line");
    assert_eq!(field_at(&rt), field0, "nothing moves for it either");
    press(&mut rt, ask);
    assert!(rt.state.box_open.is_none(), "a second press closes it");
}

/// Stills of the agent page's box, closed, with "⋯" open, and with "?" pressed. `STILL_OUT=DIR cargo test box_row_stills -- --ignored`.
#[test]
#[ignore]
fn box_row_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn closed() -> App {
        let (_held, mut s) = test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = Sel::Agent("paper-research".into());
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.spell.on = true;
        s
    }
    fn open() -> App {
        let mut s = closed();
        s.box_open = Some((crate::message_box::Target::Agent { project: 0, name: "paper-research".into() }, crate::message_box::Open::More));
        s
    }
    fn help() -> App {
        let mut s = closed();
        s.box_open = Some((crate::message_box::Target::Agent { project: 0, name: "paper-research".into() }, crate::message_box::Open::Help("Cmd/Ctrl+Enter sends; @ tags another agent".into())));
        s
    }
    for (name, init) in [("box-row.png", closed as fn() -> App), ("box-row-more.png", open), ("box-row-help.png", help)] {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) })
            .args(args)
            .try_run()
            .unwrap();
    }
}

/// Two stills of the agent page's box as he types (paper-research N-433.p69): the caret in a marked word ("wau", its
/// candidates under the box), then after the next word ("so", known). `STILL_OUT=DIR cargo test --release
/// spelling_row_stills -- --ignored`.
#[test]
#[ignore]
fn spelling_row_stills() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let script = out.join("type.json");
    std::fs::write(&script, r#"[{"t":0.5,"type":"tap","key":"cf-0-paper-research"},{"t":0.8,"type":"type","text":"I think it is wau","every":0.03},{"t":2.0,"type":"type","text":" so","every":0.03}]"#).unwrap();
    fn init() -> App {
        let (_held, mut s) = test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = Sel::Agent("paper-research".into());
        s.streams.which.insert((0, "paper-research".into()), crate::streams::Which::Work);
        s.spell.on = true;
        s
    }
    for (name, at) in [("box-in-marked-word.png", "1.8"), ("box-after-it.png", "2.8")] {
        let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), at.into(), "--script".into(), script.display().to_string()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) })
            .args(args)
            .try_run()
            .unwrap();
    }
}

/// Research with thread N-450 (one post of his) shown on Threads, its pipe a `cat` that takes every request and
/// answers none; the cat to kill at the end.
fn sending_app() -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
    let (held, mut s) = test_app();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    let v: TicketView = serde_json::from_value(json!({"id": "N-450", "n": 450, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
        "title": "Windows window", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at(1), "given": [],
        "posts": [{"n": 1, "at": at(1), "by": "owner", "text": "the box", "names": ["paper-research"]},
                  {"n": 2, "at": at(2), "by": "paper-research", "text": "Done, see the still.", "told": ["owner"]}]}))
    .unwrap();
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(v) });
    s.projects = vec![p];
    s.cur = 0;
    s.sel = Sel::Tickets;
    s.tickets.shown = Some(450);
    // sent as typed (cleaning's way is words.rs's tests'; its id is given below as words::go gives it)
    s.spell.on = false;
    let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
    (held, s, child)
}

/// His WORDS typed in N-450's foot box and sent there.
fn send_at_foot(s: &mut App, words: &str) {
    use crate::message_box::{Target, Way};
    s.composers.insert((0, "thread:N-450".into()), rune::TextEdit::new(words));
    let t = crate::message_box::chosen(s, Target::Thread { project: 0, t: "N-450".into(), post: None, way: Way::Here });
    crate::message_box::send(s, &t);
}

/// N-450 with post 3, his WORDS, cleaned in words ticket CLEANED (none: as typed).
fn n450_with_his_post_3(s: &mut App, words: &str, cleaned: Option<&str>) {
    let mut v = s.projects[0].live.tickets[&450].clone();
    let mut post = json!({"n": 3, "at": at(3), "by": "owner", "text": words, "names": []});
    if let Some(w) = cleaned {
        post["cleaned_from"] = json!(w);
    }
    v.posts.push(serde_json::from_value(post).unwrap());
    s.projects[0].live.apply(coord_types::Event { seq: 9, body: coord_types::Body::Ticket(v) });
}

/// His message stands above the box from the press on, `sending…` (paper-research N-412 post 1844, 1; consult-lead
/// post 1900, 2), and the engine's copy, coming while that bubble is drawn and before the engine's answer, takes its
/// place: one, not two, known by the words ticket the window opened for it (its `cleaned_from`), not by its text.
#[test]
fn his_message_shows_at_once_and_its_copy_takes_its_place_by_id() {
    let (_held, mut s, mut child) = sending_app();
    send_at_foot(&mut s, "ship the stills");
    assert!(s.composers.get(&(0, "thread:N-450".into())).is_none_or(|e| e.text.trim().is_empty()), "the box empties at the press");
    assert_eq!(s.outgoing.list.len(), 1);
    // it went through its words ticket (as words::go says it)
    let (id, req) = (s.outgoing.list[0].id, s.outgoing.list[0].req.clone());
    crate::outgoing::flying(&mut s, id, &req, Some("N-460".into()));
    let mut rt = window(s);
    rt.frame(0.016);
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text).collect::<Vec<_>>();
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| *x == "ship the stills").count(), 1, "his words above the box: {t:?}");
    assert!(t.iter().any(|x| x.ends_with("· sending…")), "marked sending…: {t:?}");
    // the copy comes while the bubble is drawn, before the engine's answer, its text cleaned to other words: matched
    // by its words ticket, so the post alone, from the first frame that has it (before the poll drops the message)
    n450_with_his_post_3(&mut rt.state, "Ship the stills.", Some("N-460"));
    rt.invalidate();
    rt.frame(0.016);
    let t = texts(&rt);
    assert!(!t.iter().any(|x| x == "ship the stills" || x.ends_with("· sending…") || x.ends_with("· sent")), "the bubble gave way: {t:?}");
    assert!(t.iter().any(|x| x == "Ship the stills."), "the post in its place: {t:?}");
    assert!(crate::outgoing::step(&mut rt.state) && rt.state.outgoing.list.is_empty(), "and the poll drops it");
    // the engine's answer after it changes nothing and says nothing more under the box
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 3.".into());
    assert!(rt.state.outgoing.list.is_empty());
    assert!(!rt.state.box_said.contains_key(&(0, "thread:N-450".into())), "said on the bubble, not again under the box");
    let _ = child.kill();
}

/// The pipe ends between the press and the engine's word: the bubble stays, `not sent · the engine does not
/// answer`, with Send again, which sends it once; and a message not sent stays over a window restart.
#[test]
fn a_pipe_that_ends_before_the_answer_leaves_it_not_sent_and_send_again_sends_it_once() {
    let (_held, mut s, mut child) = sending_app();
    send_at_foot(&mut s, "ship the stills");
    assert!(s.outgoing.list[0].flying);
    s.take(0, crate::link::Incoming::Down("the pipe ended (1)".into()));
    assert_eq!(s.outgoing.list[0].is, crate::outgoing::Is::NotSent("the engine does not answer".into()));
    let mut rt = window(s);
    rt.frame(0.016);
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect::<Vec<_>>();
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.ends_with("· not sent · the engine does not answer")), "{t:?}");
    assert!(t.iter().any(|x| x == "ship the stills"), "his words stay: {t:?}");
    // over a restart: kept in ui.json and put back as it was
    let kept: Vec<(String, crate::outgoing::Out)> = crate::outgoing::kept(&rt.state).into_iter().map(|(p, o)| (p, o.clone())).collect();
    rt.state.outgoing.list.clear();
    crate::outgoing::put_back(&mut rt.state, serde_json::from_value(serde_json::to_value(&kept).unwrap()).unwrap());
    assert_eq!(rt.state.outgoing.list.len(), 1, "a message not sent stays over a restart");
    // the pipe back: Send again sends it once, and it is sending… again, with no second Send again
    let mut cat2 = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    rt.state.projects[0].link = Some(crate::link::Link::to_child(cat2.stdin.take().unwrap()));
    rt.state.projects[0].down = None;
    let before = rt.state.projects[0].pending.len();
    let id = rt.state.outgoing.list[0].id;
    rt.update(|s, _| crate::outgoing::again(s, id));
    assert_eq!(rt.state.projects[0].pending.len(), before + 1, "sent once");
    rt.invalidate();
    rt.frame(0.016);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.ends_with("· sending…")) && !t.iter().any(|x| x == "Send again"), "{t:?}");
    // one with the engine when the window ended, and no answer known: not sent, said as not known
    let kept: Vec<(String, crate::outgoing::Out)> = crate::outgoing::kept(&rt.state).into_iter().map(|(p, o)| (p, o.clone())).collect();
    crate::outgoing::put_back(&mut rt.state, kept);
    assert_eq!(rt.state.outgoing.list[0].is, crate::outgoing::Is::NotSent("not known whether the engine had it".into()));
    let _ = child.kill();
    let _ = cat2.kill();
}

/// Two sent before the first is taken: both stand, in the order he sent them; the engine's answer goes to the first,
/// and its copy takes the first's place alone.
#[test]
fn two_sent_before_the_first_is_taken_stay_in_order() {
    let (_held, mut s, mut child) = sending_app();
    send_at_foot(&mut s, "first of two");
    send_at_foot(&mut s, "second of two");
    let mut rt = window(s);
    rt.frame(0.016);
    let at_y = |rt: &rune::runtime::Runtime<App>, w: &str| rt.elements().into_iter().find(|e| e.text.as_deref() == Some(w)).map(|e| e.rect[1]);
    let (a, b) = (at_y(&rt, "first of two").expect("first"), at_y(&rt, "second of two").expect("second"));
    assert!(a < b, "in the order he sent them: {a} {b}");
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 3.".into());
    let is: Vec<crate::outgoing::Is> = rt.state.outgoing.list.iter().map(|o| o.is.clone()).collect();
    assert_eq!(is, [crate::outgoing::Is::Sent, crate::outgoing::Is::Sending], "the answer is the first's");
    assert_eq!(rt.state.outgoing.list[0].post, Some(("N-450".into(), 3)));
    rt.invalidate();
    rt.frame(0.016);
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(t.iter().any(|x| x.ends_with("· sent")) && t.iter().any(|x| x.ends_with("· sending…")), "first sent, second sending…: {t:?}");
    // the first's copy: the post takes the first's place, the second stays on its way
    n450_with_his_post_3(&mut rt.state, "first of two", None);
    rt.invalidate();
    rt.frame(0.016);
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(!t.iter().any(|x| x.ends_with("· sent")), "the first's bubble gave way to its post: {t:?}");
    assert_eq!(t.iter().filter(|x| x.ends_with("· sending…")).count(), 1, "{t:?}");
    assert!(t.iter().any(|x| x == "second of two"), "the second still on its way: {t:?}");
    crate::outgoing::step(&mut rt.state);
    assert_eq!(rt.state.outgoing.list.iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), ["second of two"]);
    let _ = child.kill();
}

/// `Take back` on a bubble not sent (0.2 line B8, N-1396; plan N-818 post 72): it goes, his words back in the box,
/// nothing sent.
#[test]
fn take_back_of_a_message_not_sent_puts_his_words_back_and_sends_nothing() {
    let (_held, mut s, mut child) = sending_app();
    send_at_foot(&mut s, "ship the stills");
    s.take(0, crate::link::Incoming::Down("the pipe ended (1)".into()));
    let before = s.projects[0].pending.len();
    let id = s.outgoing.list[0].id;
    crate::outgoing::take_back(&mut s, id);
    assert!(s.outgoing.list.is_empty(), "the bubble goes");
    assert_eq!(s.composers.get(&(0, "thread:N-450".into())).map(|e| e.text.as_str()), Some("ship the stills"), "his words back in the box");
    assert_eq!(s.projects[0].pending.len(), before, "nothing sent");
    let _ = child.kill();
}

/// `Take back` pressed while its request is out: `taking back…`, and the engine's answer decides. Its post named, the
/// take back goes once; taken back, the bubble goes; refused (handed meanwhile), it stays as the engine has it and the
/// refusal is said under the box.
#[test]
fn take_back_while_its_request_is_out_goes_when_the_engine_answers() {
    let (_held, mut s, mut child) = sending_app();
    send_at_foot(&mut s, "first of two");
    send_at_foot(&mut s, "second of two");
    let (a, b) = (s.outgoing.list[0].id, s.outgoing.list[1].id);
    crate::outgoing::take_back(&mut s, a);
    crate::outgoing::take_back(&mut s, b);
    let mut rt = window(s);
    rt.frame(0.016);
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect::<Vec<_>>();
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.ends_with("· taking back…")).count(), 2, "{t:?}");
    assert!(!t.iter().any(|x| x == "Take back"), "pressed once: {t:?}");
    // the engine answers the first: its post is named, and its take back goes, once
    let box_key = (0usize, "thread:N-450".to_string());
    let before = rt.state.projects[0].pending.len();
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 3.".into());
    assert_eq!(rt.state.projects[0].pending.len(), before + 1, "one take back sent");
    assert!(rt.state.outgoing.list[0].taking);
    crate::outgoing::step(&mut rt.state);
    assert_eq!(rt.state.projects[0].pending.len(), before + 1, "not sent twice");
    // and the second's: its post is named too
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 4.".into());
    assert_eq!(rt.state.projects[0].pending.len(), before + 2);
    // the first is taken back: it goes
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::TakeBack(0, "thread:N-450".into()) }, true, "N-450.p3 is taken back.".into());
    assert_eq!(rt.state.outgoing.list.iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), ["second of two"]);
    // the second was handed between the read and the act: refused, it stands as the engine has it, the refusal said
    crate::threads::replied(&mut rt.state, crate::Pending::TicketDone { clear: crate::threads::Cleared::TakeBack(0, "thread:N-450".into()) }, false, "Post 4 of N-450 was handed to lead already: it cannot be taken back.".into());
    let o = &rt.state.outgoing.list[0];
    assert!(o.is == crate::outgoing::Is::Sent && !o.take_back && !o.taking, "{o:?}");
    assert!(matches!(rt.state.box_said.get(&box_key), Some(Err(e)) if e.contains("handed to lead")));
    rt.invalidate();
    rt.frame(0.016);
    let t = texts(&rt);
    assert!(t.iter().any(|x| x.ends_with("· sent")) && t.iter().any(|x| x == "Take back"), "{t:?}");
    let _ = child.kill();
}

/// `Take back` on a message whose request is out, and the pipe ends before the answer: his words back in the box, and
/// back before what he typed there since. With the spelling on too, as nothing is held in cleaning after Send (0.2 B9,
/// dcd87905): the message is on its way from the press on, so no take back finds it in a cleaning.
#[test]
fn take_back_before_a_pipe_ends_puts_his_words_back() {
    let (_held, mut s, mut child) = sending_app();
    s.projects[0].protocol = coord_types::PROTOCOL;
    s.spell.on = true;
    send_at_foot(&mut s, "pls ship teh stills");
    assert!(s.words.pending.is_empty(), "nothing held in cleaning after Send");
    let o = &s.outgoing.list[0];
    assert!(o.flying && o.is == crate::outgoing::Is::Sending, "on its way at once: {o:?}");
    let id = o.id;
    crate::outgoing::take_back(&mut s, id);
    assert!(s.outgoing.list.iter().any(|o| o.id == id && o.take_back), "taken back when the engine answers");
    s.composers.insert((0, "thread:N-450".into()), rune::TextEdit::new("a later one"));
    s.take(0, crate::link::Incoming::Down("the pipe ended (1)".into()));
    assert!(s.outgoing.list.is_empty());
    assert_eq!(s.composers.get(&(0, "thread:N-450".into())).map(|e| e.text.as_str()), Some("pls ship teh stills\n\na later one"));
    let _ = child.kill();
}

/// 0.2 B8 (N-1396): his post taken back after an agent had it comes in the view's `struck`, apart from its posts (a
/// window from before draws it gone): drawn in its place, his words struck through, `taken back HH:MM`. Take back is
/// on his posts that told an agent, until each answered; not on a thread's first post, its words.
#[test]
fn his_post_taken_back_after_it_was_given_is_drawn_struck_in_its_place() {
    use crate::threads::TRow;
    let (_held, mut s, mut child) = sending_app();
    s.projects[0].protocol = coord_types::ASKS_CLOSED_PROTOCOL;
    let mut v = s.projects[0].live.tickets[&450].clone();
    // his post 3 was given to paper-research and taken back; its post 4 came after; his post 5 is handed, not answered
    v.posts.push(serde_json::from_value(json!({"n": 4, "at": at(5), "by": "paper-research", "text": "Stopped.", "told": ["owner"]})).unwrap());
    v.posts.push(serde_json::from_value(json!({"n": 5, "at": at(6), "by": "owner", "text": "and the docs", "names": ["paper-research"], "told": ["paper-research"]})).unwrap());
    v.given = serde_json::from_value(json!([{"to": "paper-research", "delivery": "N-450.p5", "upto": 5, "at": at(7)}])).unwrap();
    v.struck = vec![serde_json::from_value(json!({"post": {"n": 3, "at": at(3), "by": "owner", "text": "ship the stills", "names": ["paper-research"], "told": ["paper-research"]}, "at": at(4), "to": ["paper-research"]})).unwrap()];
    s.projects[0].live.apply(coord_types::Event { seq: 9, body: coord_types::Body::Ticket(v) });
    let p = &s.projects[0];
    let v = &p.live.tickets[&450];
    let rows = crate::threads::seq(p, v);
    let i = |r: TRow| rows.iter().position(|x| *x == r).unwrap_or_else(|| panic!("{rows:?}"));
    assert!(i(TRow::Post(2)) < i(TRow::Struck(3)) && i(TRow::Struck(3)) < i(TRow::Post(4)), "in its place: {rows:?}");
    let k = rune_kit::Kit::neutral(false);
    let post = |n: u32| v.posts.iter().find(|x| x.n == n).unwrap();
    assert!(crate::threads::take_back(&s, p, v, post(5), &k).is_some(), "on his post handed and not answered");
    assert!(crate::threads::take_back(&s, p, v, post(1), &k).is_none(), "not on the thread's words");
    assert!(crate::threads::take_back(&s, p, v, post(4), &k).is_none(), "not on an agent's");
    let mut rt = window(s);
    rt.frame(0.016);
    let els = rt.elements();
    let texts: Vec<String> = els.iter().filter_map(|e| e.text.clone()).collect();
    assert!(!rows.contains(&TRow::Post(3)), "not drawn as a post: {rows:?}");
    // his words once, in the struck row, faint beside his live post (the stroke itself is rune's `strike`, in the still)
    let color = |t: &str| els.iter().filter(|e| e.text.as_deref() == Some(t)).map(|e| e.text_color).collect::<Vec<_>>();
    assert_eq!(color("ship the stills").len(), 1, "{texts:?}");
    let live = color("and the docs");
    assert!(!live.is_empty() && live.iter().all(|c| *c != color("ship the stills")[0]), "faint: {:?} {live:?}", color("ship the stills"));
    assert!(texts.iter().any(|t| t.contains(&format!("taken back {}", crate::when::at(&at(4))))), "{texts:?}");
    let _ = child.kill();
}

// ---- the prod fix on build 27 (N-1396 p47–p57): what he sent shows where he sent it, the engine's answer read for his
// post's number, his words kept from the press

/// The engine's three answers to a post (coordd ticket/mod.rs `ticket_post`), read for his post's number: none for the
/// quote reply's, which gives none; never a bare `N-12` read as post 1 when the answer speaks of a post.
#[test]
fn the_engines_answer_names_his_post_or_none() {
    use crate::outgoing::post_named;
    let named = |t: &str, n: u32| Some((String::from(t), n));
    assert_eq!(post_named("Posted on N-1104 as post 496."), named("N-1104", 496));
    assert_eq!(post_named("Posted on N-1104 as post 496, under post 495."), named("N-1104", 496));
    assert_eq!(post_named("Posted on N-1104 under post 495."), None, "a quote reply's answer gives no number of his");
    assert_eq!(post_named("Posted on N-1104."), None);
    assert_eq!(post_named("N-12 is closed: a new thread follows it. Taken: to lead."), None);
    assert_eq!(post_named("Taken: to lead, under N-12 at post 4."), None);
    assert_eq!(post_named("N-450.p3 is taken back."), named("N-450", 3));
    use crate::outgoing::posted_on;
    assert!(posted_on("Posted on N-12 under post 4.", "N-12") && posted_on("Posted on N-12.", "N-12"));
    assert!(!posted_on("Posted on N-123.", "N-12"), "N-123 is another ticket");
}

/// The engine's answer comes before its copy is in the snapshot: his message stays above the box, `sent`, until the
/// post the answer names is there; a quote reply's answer, which names none, keeps it until his post is found by what
/// it is (N-1396 p55: every post on a thread was read as post 1, already there, and the bubble went at once).
#[test]
fn his_message_stays_until_its_own_post_is_in_the_snapshot() {
    let (_held, mut s, mut child) = sending_app();
    let clear = || crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) };
    send_at_foot(&mut s, "ship the stills");
    send_at_foot(&mut s, "and the docs");
    crate::threads::replied(&mut s, clear(), true, "Posted on N-450 as post 3.".into());
    crate::threads::replied(&mut s, clear(), true, "Posted on N-450 under post 2.".into());
    assert_eq!(s.outgoing.list.iter().map(|o| o.post.clone()).collect::<Vec<_>>(), [Some((String::from("N-450"), 3)), None]);
    crate::outgoing::step(&mut s);
    assert_eq!(s.outgoing.list.len(), 2, "neither post is in the snapshot yet: both stay");
    let mut rt = window(s);
    rt.frame(0.016);
    let texts = |rt: &rune::runtime::Runtime<App>| rt.elements().into_iter().filter_map(|e| e.text).collect::<Vec<_>>();
    let t = texts(&rt);
    assert_eq!(t.iter().filter(|x| x.ends_with("· sent")).count(), 2, "{t:?}");
    assert!(t.iter().any(|x| x == "ship the stills") && t.iter().any(|x| x == "and the docs"), "{t:?}");
    // the copies come: post 3, named; post 4, found by his words after the last post there when he pressed
    n450_with_his_post_3(&mut rt.state, "ship the stills", None);
    let mut v = rt.state.projects[0].live.tickets[&450].clone();
    v.posts.push(serde_json::from_value(json!({"n": 4, "at": at(4), "by": "owner", "text": "and the docs", "names": [], "answers": 2})).unwrap());
    rt.state.projects[0].live.apply(coord_types::Event { seq: 10, body: coord_types::Body::Ticket(v) });
    assert!(crate::outgoing::step(&mut rt.state) && rt.state.outgoing.list.is_empty(), "each gave way to its own post");
    let _ = child.kill();
}

/// Take back on a `sent` message on a thread whose post 1 is his: the take back names his post, not post 1 (N-1396
/// p55: it named post 1, which the engine refuses; on a thread he had just opened, his first words).
#[test]
fn take_back_after_sent_names_his_post_not_post_1() {
    let (_held, mut s, mut child) = sending_app();
    // a link whose requests are kept, to read what went
    let file = std::env::temp_dir().join(format!("yaaihh-take-back-names-{}.jsonl", std::process::id()));
    let mut keep = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", file.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
    s.projects[0].link = Some(crate::link::Link::to_child(keep.stdin.take().unwrap()));
    assert_eq!(s.projects[0].live.tickets[&450].posts[0].by, "owner", "post 1 is his");
    send_at_foot(&mut s, "and the docs");
    crate::threads::replied(&mut s, crate::Pending::TicketDone { clear: crate::threads::Cleared::Out(0, "thread:N-450".into()) }, true, "Posted on N-450 as post 3.".into());
    let id = s.outgoing.list[0].id;
    crate::outgoing::take_back(&mut s, id);
    crate::outgoing::step(&mut s);
    s.projects[0].link = None;
    let _ = keep.wait();
    let backs: Vec<serde_json::Value> = std::fs::read_to_string(&file).unwrap_or_default().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).filter(|r| r["op"] == "take_back").collect();
    assert_eq!(backs, [json!({"op": "take_back", "t": "N-450", "n": 3})], "his post, once");
    let _ = std::fs::remove_file(&file);
    let _ = child.kill();
}

/// Send while the engine restarts (no pipe): his message stands as `not sent · the box is not connected`, the box
/// empty, Send again beside it, said once there; once the pipe is back it goes by itself, exactly once (N-1396 p51).
#[test]
fn a_send_with_no_pipe_stays_not_sent_and_goes_once_when_the_pipe_is_back() {
    let (_held, mut s, mut child) = sending_app();
    let link = s.projects[0].link.take();
    s.projects[0].down = Some("connecting".into());
    send_at_foot(&mut s, "ship the stills");
    let key = (0usize, String::from("thread:N-450"));
    assert!(s.composers.get(&key).is_none_or(|e| e.text.trim().is_empty()), "the box empties as on any send");
    let o = &s.outgoing.list[0];
    assert!(o.is == crate::outgoing::Is::NotSent("the box is not connected".into()) && o.unsent && !o.flying, "{o:?}");
    assert!(!s.box_said.contains_key(&key), "said once, on his message");
    let mut rt = window(s);
    rt.frame(0.016);
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect();
    assert!(t.iter().any(|x| x.ends_with("· not sent · the box is not connected")) && t.iter().any(|x| x == "Send again"), "{t:?}");
    assert!(t.iter().any(|x| x == "ship the stills"), "his words: {t:?}");
    // no pipe yet: nothing goes
    crate::outgoing::step(&mut rt.state);
    assert!(!rt.state.outgoing.list[0].flying);
    // the pipe back: it goes by itself, once
    rt.state.projects[0].link = link;
    rt.state.projects[0].down = None;
    let before = rt.state.projects[0].pending.len();
    assert!(crate::outgoing::step(&mut rt.state));
    let o = &rt.state.outgoing.list[0];
    assert!(o.flying && o.is == crate::outgoing::Is::Sending && !o.unsent, "{o:?}");
    crate::outgoing::step(&mut rt.state);
    assert_eq!(rt.state.projects[0].pending.len(), before + 1, "sent exactly once");
    let _ = child.kill();
}

/// A new window of the project and thread SENT_FROM has (state.rs reads ui.json into it).
fn next_window(sent_from: &App) -> App {
    let mut n = crate::init();
    let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
    p.down = None;
    p.live.apply(coord_types::Event { seq: 1, body: coord_types::Body::Ticket(sent_from.projects[0].live.tickets[&450].clone()) });
    n.projects = vec![p];
    n
}

/// Send, and the window ends before the engine's answer: his words went to ui.json at the press, so the next window
/// shows them, `not sent · not known whether the engine had it`; when the engine had it, its post takes the message's
/// place there, found by what it is, and nothing goes twice (N-1396 p51).
#[test]
fn a_window_killed_after_send_shows_his_message_when_it_starts_again() {
    let (_held, mut s, mut child) = sending_app();
    // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
    let dir = std::env::temp_dir().join(format!("yaaihh-sent-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock (`sending_app` holds it), which every test that reads or sets it holds
    unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
    send_at_foot(&mut s, "ship the stills");
    // killed here: no poll, no settle, nothing written after the press
    let mut n = next_window(&s);
    crate::state::restore(&mut n);
    assert_eq!(n.outgoing.list.iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), ["ship the stills"]);
    assert_eq!(n.outgoing.list[0].is, crate::outgoing::Is::NotSent("not known whether the engine had it".into()));
    let mut rt = window(n);
    rt.frame(0.016);
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text.or(e.label)).collect();
    assert!(t.iter().any(|x| x == "ship the stills") && t.iter().any(|x| x == "Send again"), "{t:?}");
    // the engine had it: its post comes, and takes the message's place
    n450_with_his_post_3(&mut rt.state, "ship the stills", None);
    let before = rt.state.projects[0].pending.len();
    assert!(crate::outgoing::step(&mut rt.state) && rt.state.outgoing.list.is_empty());
    assert_eq!(rt.state.projects[0].pending.len(), before, "nothing sent again");
    let _ = child.kill();
}

/// Refused with no pipe (an engine restart), then a swap onto a new build at `go`, then the pipe back: it goes once,
/// from the new window; the old one, handed over, sends nothing (consult-3, N-1396 p70: each sent it). His install
/// restarts the engine, then the window.
#[test]
fn a_message_refused_before_a_swap_goes_once_from_the_new_window() {
    let (_held, mut s, mut child) = sending_app();
    // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
    let dir = std::env::temp_dir().join(format!("yaaihh-sent-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock (`sending_app` holds it), which every test that reads or sets it holds
    unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
    // the engine restarting: no pipe, his message stands as not sent
    let link = s.projects[0].link.take();
    s.projects[0].down = Some("connecting".into());
    send_at_foot(&mut s, "ship the stills");
    assert!(s.outgoing.list[0].unsent, "{:?}", s.outgoing.list);
    // the swap: the new window, started, reads the old one's texts at "go"; the old one has handed over
    let mut n = next_window(&s);
    crate::restart::hand_over(&mut n, true);
    crate::state::retake(&mut n);
    crate::restart::hand_over(&mut n, false);
    crate::restart::hand_over(&mut s, true);
    assert_eq!(n.outgoing.list.iter().map(|o| (o.text.as_str(), o.unsent)).collect::<Vec<_>>(), [("ship the stills", true)]);
    // the pipe back, to both
    let mut other = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
    s.projects[0].link = link;
    s.projects[0].down = None;
    n.projects[0].link = Some(crate::link::Link::to_child(other.stdin.take().unwrap()));
    n.projects[0].down = None;
    let (old, new) = (s.projects[0].pending.len(), n.projects[0].pending.len());
    for _ in 0..2 {
        crate::outgoing::step(&mut s);
        crate::outgoing::step(&mut n);
    }
    assert_eq!(s.projects[0].pending.len(), old, "the old window, handed over, sends nothing");
    assert_eq!(n.projects[0].pending.len(), new + 1, "the new one sends it, once");
    let _ = child.kill();
    let _ = other.kill();
}

/// Send in the old window while it swaps onto a new build (restart.rs): the new one, started before the press, takes
/// the state again from ui.json at `go` (`retake`), and his message with it.
#[test]
fn a_send_during_a_swap_reaches_the_new_window() {
    let (_held, mut s, mut child) = sending_app();
    // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
    let dir = std::env::temp_dir().join(format!("yaaihh-sent-kept-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock (`sending_app` holds it), which every test that reads or sets it holds
    unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
    assert!(crate::state::write_now(&mut s), "as `begin` writes before the new exe starts");
    let mut n = next_window(&s);
    crate::state::restore(&mut n);
    assert!(n.outgoing.list.is_empty(), "nothing on its way when it started");
    send_at_foot(&mut s, "ship the stills");
    crate::state::retake(&mut n);
    assert_eq!(n.outgoing.list.iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), ["ship the stills"]);
    let _ = child.kill();
}

/// A press in the old window while it swaps (restart.rs), after the new one started and before `go`: the new one takes
/// the whole state again at `go` (`retake`), so it shows the page, the panes, the tabs and the thread beside With
/// you's list he left, not what ui.json held when it started (consult-3, docket-edge6, N-1396 p342, p343); a box he
/// emptied meanwhile comes back empty, and no tab comes twice.
#[test]
fn a_press_during_a_swap_is_what_the_new_window_shows() {
    let (_held, mut s, mut child) = sending_app();
    // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
    let dir = std::env::temp_dir().join(format!("yaaihh-swap-press-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock (`sending_app` holds it), which every test that reads or sets it holds
    unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
    // as `begin` writes before the new exe starts: N-450 on Tickets, one tab, a word in paper-research's box
    s.new_tab();
    s.composers.insert((0, "paper-research".into()), rune::TextEdit::new("half a"));
    assert!(crate::state::write_now(&mut s));
    let mut n = next_window(&s);
    crate::state::restore(&mut n);
    assert_eq!((&n.sel, n.tabs.tabs.len(), n.withyou.inside.len()), (&Sel::Tickets, 2, 0), "as ui.json was at its start");
    // his presses in the old window meanwhile: a second tab, paper-research's page with N-450 beside its list and Files
    // in a pane, the box emptied
    s.new_tab();
    s.sel = Sel::Agent("paper-research".into());
    s.withyou.inside.insert((0, "paper-research".into()), 450);
    s.panes = vec![(0, Sel::Files)];
    s.composers.clear();
    // `go`: the old one writes its state again (restart.rs `follow`), and the new one takes it back
    assert!(crate::state::write_now(&mut s));
    crate::state::retake(&mut n);
    assert_eq!(n.sel, Sel::Agent("paper-research".into()), "the page he pressed");
    assert_eq!(n.withyou.inside.get(&(0, "paper-research".into())), Some(&450), "the thread he picked beside the list");
    assert_eq!(n.panes, vec![(0, Sel::Files)], "the pane");
    assert_eq!((n.tabs.tabs.len(), n.tabs.cur), (3, 2), "his tabs, none twice");
    assert!(n.composers.is_empty(), "the box he emptied: {:?}", n.composers.keys().collect::<Vec<_>>());
    assert!(!n.withyou.restored.contains(&0), "his picks of a moment ago are not checked again");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = child.kill();
}

/// His answer to a post that asks him stands in its own place too, a reply under the thread's last post, where he sent
/// it, besides under the ask it answers (N-1396 p53: post 496, drawn only inside 495, looked lost to him).
#[test]
fn his_answer_to_a_post_that_asks_him_stands_where_he_sent_it() {
    use crate::threads::TRow;
    let (_held, mut s, mut child) = sending_app();
    let mut v = s.projects[0].live.tickets[&450].clone();
    v.posts.push(serde_json::from_value(json!({"n": 3, "at": at(3), "by": "paper-research", "text": "Type this in my terminal: ls", "told": ["owner"], "asks": [{"at": [0, 25]}]})).unwrap());
    v.posts.push(serde_json::from_value(json!({"n": 4, "at": at(4), "by": "owner", "text": "> Type this in my terminal:\nthe search is too slow", "names": ["paper-research"], "answers": 3})).unwrap());
    s.projects[0].live.apply(coord_types::Event { seq: 9, body: coord_types::Body::Ticket(v) });
    let rows = crate::threads::seq(&s.projects[0], &s.projects[0].live.tickets[&450]);
    let i = |r: TRow| rows.iter().position(|x| *x == r).unwrap_or_else(|| panic!("{rows:?}"));
    assert!(i(TRow::Post(3)) < i(TRow::Post(4)), "his answer in its own place, after what it answers: {rows:?}");
    let mut rt = window(s);
    for _ in 0..3 {
        rt.frame(0.016);
    }
    let t: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
    assert!(t.iter().filter(|x| x.contains("the search is too slow")).count() >= 2, "under the ask and in its own place: {t:?}");
    // in its own place, the line it answers quoted and his words apart from it, not run into the quote
    assert!(t.iter().filter(|x| *x == "the search is too slow").count() >= 2, "{t:?}");
    let _ = child.kill();
}


/// 0.2 line 28 (the owner, N-1104 post 496; N-1396 p47): the bar's menu switches the window dark or light, every part
/// in the kit's colours at once, the memoised ones too (Rune's memo reads only the system's look), and ui.json keeps
/// it across a restart.
#[test]
fn the_bar_menu_switches_dark_and_light_and_a_restart_keeps_it() {
    let (_held, s, mut child) = sending_app();
    // ui.json in a folder of this test's own (state.rs's test points YAAIHH_CONFIG at one it removes after)
    let dir = std::env::temp_dir().join(format!("yaaihh-look-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: under the tests' lock (`sending_app` holds it), which every test that reads or sets it holds
    unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
    // the system light (`window`), his look not set: light
    let mut rt = window(s);
    for _ in 0..3 {
        rt.frame(0.016);
    }
    let colour = |rt: &rune::runtime::Runtime<App>, t: &str, place: &str| rt.elements().into_iter().find(|e| e.text.as_deref() == Some(t) && e.path.contains(place)).and_then(|e| e.text_color);
    // a post's words (post.rs's memo) and the sidebar's Threads (sidebar.rs's)
    let words = colour(&rt, "Done, see the still.", "").expect("the post's words");
    let side = colour(&rt, "Threads", "sidebar").expect("the sidebar's Threads");
    // the bar's menu: Dark mode, pressed
    rt.state.bar_menu = true;
    rt.invalidate();
    rt.frame(0.016);
    let item = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("Dark mode") && e.path.contains("bar-menu")).map(|e| e.rect);
    let item = item.unwrap_or_else(|| panic!("Dark mode in the bar's menu: {:?}", rt.elements().into_iter().filter(|e| e.path.contains("bar-menu")).map(|e| (e.path, e.text)).collect::<Vec<_>>()));
    let (x, y) = (item[0] + item[2] / 2.0, item[1] + item[3] / 2.0);
    rt.pointer_move(x, y);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    for _ in 0..3 {
        rt.frame(0.016);
    }
    assert_eq!(rt.state.dark, Some(true), "pressed: dark");
    assert!(!rt.state.bar_menu, "the menu closes");
    let (words2, side2) = (colour(&rt, "Done, see the still.", ""), colour(&rt, "Threads", "sidebar"));
    assert!(words2.is_some_and(|c| c != words), "the post's words in the dark kit's colour: {words:?} → {words2:?}");
    assert!(side2.is_some_and(|c| c != side), "the sidebar's too: {side:?} → {side2:?}");
    // kept: the next window starts dark, on a light system
    assert!(crate::state::write_now(&mut rt.state));
    let mut n = crate::init();
    crate::state::restore(&mut n);
    assert_eq!(n.dark, Some(true));
    let _ = child.kill();
}
