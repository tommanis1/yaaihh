//! The owner's `@all` (consult-lead N-1158 post 1; here-1's bac7efbc, docket-core4's part 2 of N-1104 post 297): his
//! post on a thread of six reaches all six, the policy's `claims` (policy/claims.eflint) says which one is handed it
//! (the agent whose work the words are about, else one out of a turn, else the answerer), the others are informed,
//! and the first answer by any of them is the claim (consult-lead N-807 post 90, hold 2), whatever its words; another's
//! after it is refused. The agent it is handed to may pass it on instead, as an act of its own (`coord pass`, the pass
//! tool; consult-lead N-412 post 1900), to any agent of the project that can be woken and never held it. Only an agent
//! that can be woken now (running, not ended) is handed it or passed it (hold 1).

use super::{run_task, Core, Said, Task};
use crate::core::Who;
use docket_machine::{Input, Line, Text};
use serde_json::{json, Value};

const AGENTS: [&str; 6] = ["ann", "bob", "cid", "dee", "eve", "fay"];

/// A coord of six agents on agent-base, a thread the owner opened to ann that the other five have posted on, its
/// tasks run as coordd runs them (`settle`).
struct Coord {
    core: Core,
    rx: tokio::sync::mpsc::UnboundedReceiver<Task>,
    engine: crate::policy::Engine,
    dir: std::path::PathBuf,
    t: u64,
}

impl Coord {
    fn new(tag: &str) -> Coord {
        Coord::with(tag, None)
    }
    /// As [`Coord::new`], in a project whose policy folder holds POLICY as its own file `answerer.eflint`.
    fn with(tag: &str, policy: Option<&str>) -> Coord {
        Coord::with_more(tag, policy, &[], false)
    }
    /// As [`Coord::with`], with MORE agents in the registry, on no thread.
    fn with_more(tag: &str, policy: Option<&str>, more: &[&str], at_64: bool) -> Coord {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let dir = std::env::temp_dir().join(format!("docket-claims-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // AT_64: the project's thread.kind and asks.call are 64's (docket-machine testdata), and a project's kind of the
        // same name wins: the thread is opened as research's threads from before 65 were, and keeps their `when
        // agent_post` (proto-lead N-1479 p274)
        if at_64 {
            let project = dir.join("project-dir");
            let testdata = concat!(env!("CARGO_MANIFEST_DIR"), "/../docket/machine/testdata");
            for (d, from, to) in [("kinds", "thread-at-64.kind", "thread.kind"), ("calls", "asks-at-64.call", "asks.call")] {
                std::fs::create_dir_all(project.join(d)).unwrap();
                std::fs::copy(format!("{testdata}/{from}"), project.join(d).join(to)).unwrap();
            }
            std::fs::write(project.join("project.toml"), "[project]\nname = \"p\"\n").unwrap();
            std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        }
        // the six on the thread, and `ans`, on no thread, whom a test may give the role "answerer" (research's `here`)
        let reg: String = AGENTS.iter().chain(&["ans"]).chain(more).enumerate().map(|(i, n)| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n")).collect();
        std::fs::write(dir.join("registry.jsonl"), reg).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base);
        if at_64 {
            core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        }
        if let Some(policy) = policy {
            let project = dir.join("project-dir");
            std::fs::create_dir_all(project.join("policy")).unwrap();
            std::fs::write(project.join("policy/answerer.eflint"), policy).unwrap();
            std::fs::write(project.join("project.toml"), "[project]\nname = \"p\"\n").unwrap();
            std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
            core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        }
        // every agent runs: a hand-over can wake it (an agent stopped is set so by its test)
        for a in core.agents.values_mut() {
            a.live.pid = Some(1);
        }
        core.load_tickets();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        core.tickets.tasks = Some(tx);
        // the policy is solved with the proxy's models: a stand-in that serves none (as decide_tests.rs)
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut s in l.incoming().flatten() {
                let mut buf = [0u8; 65536];
                let _ = s.read(&mut buf);
                let body = r#"{"data": []}"#;
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
        let engine = crate::policy::Engine::start(core.places());
        let v = json!({"from": "owner", "to": "ann", "typed": ["ann"], "names": [], "text": "the build is red", "addressed": {"head": "Unnamed"}, "esc": []});
        core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let t = *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        // under a policy that holds his posts his opening words are held too (before intake on today's thread.kind,
        // after intake on 64's): they go first, to ann
        super::tests::opener_goes(&mut core, t);
        core.ticket_answer(docket_machine::WATCHER, t, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        super::tests::opener_goes(&mut core, t);
        for a in &AGENTS[1..] {
            core.ticket_input(Input::Post { t, by: a.to_string(), text: Text::New(format!("{a} here")), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        }
        let mut c = Coord { core, rx, engine, dir, t };
        c.settle();
        c
    }
    fn settle(&mut self) {
        self.settle_then(|_| {});
    }
    /// As [`Coord::settle`], with BETWEEN run once after the first decision among agents is solved and before its
    /// answer is put in: what happens to an agent between the facts and the hand-over.
    fn settle_then(&mut self, between: impl FnOnce(&mut Core)) {
        let mut between = Some(between);
        while let Ok(task) = self.rx.try_recv() {
            for i in run_task(&self.engine, task) {
                if matches!(&i, Input::Decided { question, .. } if question == "decide") {
                    if let Some(f) = between.take() {
                        f(&mut self.core);
                    }
                }
                let _ = self.core.ticket_input(i);
            }
        }
    }
    /// AGENT stops: its process (pid 1, `Coord::with`) exits, so nothing can wake it.
    fn stop(&mut self, agent: &str) {
        self.core.exited(agent, 1);
        self.settle();
    }
    /// The owner's words posted on the thread, as his own post typed to nobody, and what runs on them run: its number.
    fn owner_says(&mut self, words: &str) -> u32 {
        self.owner_says_then(words, |_| {})
    }
    /// As [`Coord::owner_says`], with BETWEEN run between the decision and its answer ([`Coord::settle_then`]).
    fn owner_says_then(&mut self, words: &str, between: impl FnOnce(&mut Core)) -> u32 {
        self.core.ticket_input(Input::Post { t: self.t, by: "owner".into(), text: Text::New(words.into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        self.settle_then(between);
        self.core.tickets.m.tickets[&self.t].posts.iter().rev().find(|p| p.by == "owner").unwrap().n
    }
    /// The text AGENT is given for its delivery of post N, as coordd renders it (posts.tmpl). An agent only informed
    /// may have no delivery due yet: then the handed agent's, as AGENT would be given it.
    fn given(&self, agent: &str, n: u32) -> String {
        let has = |d: &docket_machine::Delivery| matches!(&d.what, docket_machine::What::Posts { unread } if unread.iter().any(|p| p.n == n));
        let mut d = self.core.ticket_due(agent).into_iter().find(|d| has(d)).or_else(|| AGENTS.iter().chain(&["ans"]).flat_map(|a| self.core.ticket_due(a)).find(|d| has(d))).unwrap_or_else(|| panic!("nobody is due post {n}"));
        d.to = agent.to_string();
        let view = self.core.tickets.m.view(self.t).unwrap();
        crate::render::delivery(&self.core.places(), &d, &view, false, false, false, false, None).unwrap().0
    }
    /// A ticket whose step awaits AGENT, which is what `about` reads (a thread awaits nobody: it rests at its hole): a
    /// request (request.kind) with AGENT its approver, as `coord team ask` opens one. Its id (`R-7`).
    fn piece_for(&mut self, agent: &str, words: &str) -> String {
        let form = json!({"who": "owner", "approver": agent, "why": words});
        self.core.ticket_input(Input::Open { kind: "request".into(), under: None, opener: "owner".into(), form, text: None, names: Vec::new(), told: Vec::new() }).unwrap();
        let piece = *self.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        self.settle();
        let id = self.core.tickets.m.id(piece);
        assert!(self.core.tickets.m.about(&format!("who knows {id}?")).contains(agent), "{id} awaits {agent}: {:?}", self.core.tickets.m.tickets[&piece].status);
        id
    }
    /// Whom post N woke and whom it informed, as the record says (its `told` line).
    fn told(&self, n: u32) -> (Vec<String>, Vec<String>) {
        let log = std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap();
        log.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()).find_map(|l| match l {
            Line::Told { t, n: m, to, inform, .. } if t == self.t && m == n => Some((to, inform)),
            _ => None,
        }).unwrap_or_else(|| panic!("no told line for post {n}: {:?}", self.core.tickets.m.tickets[&self.t].posts.iter().find(|p| p.n == n)))
    }
    /// AGENT's answer to post N, as the reply tool posts it.
    fn answer(&mut self, agent: &str, n: u32, words: &str) -> coord_types::Reply {
        let id = self.core.tickets.m.id(self.t);
        self.core.ticket_post(&Who::Agent(agent.into()), &id, words.into(), None, Some(n), Said::default(), None, false)
    }
    /// AGENT passes post N on to TO, with WHY, as the pass tool's request does (`Request::Pass`).
    fn pass(&mut self, agent: &str, n: u32, to: &str, why: Option<&str>) -> coord_types::Reply {
        let id = self.core.tickets.m.id(self.t);
        self.core.ticket_pass(&Who::Agent(agent.into()), &id, n, to, why.map(str::to_string))
    }
    /// A second thread the owner opens to AGENT with WORDS, answered as a Thread by the watcher: its number.
    fn thread_to(&mut self, agent: &str, words: &str) -> u64 {
        let v = json!({"from": "owner", "to": agent, "typed": [agent], "names": [], "text": words, "addressed": {"head": "Unnamed"}, "esc": []});
        self.core.ticket_input(Input::Event { name: "owner_message".into(), value: v }).unwrap();
        let u = *self.core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0;
        self.core.ticket_answer(docket_machine::WATCHER, u, json!({"head": "Thread"}), Text::New(String::new())).unwrap();
        self.settle();
        u
    }
    /// AGENT is given all that is due to it now (`Input::Given`), its turn going on.
    fn give_all(&mut self, agent: &str) {
        for d in self.core.ticket_due(agent) {
            self.core.ticket_input(Input::Given { t: d.t, to: agent.into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
        }
    }
    /// AGENT is given all that is due to it (`give_all`), and its turn ends with TEXT as its last message, written in its
    /// transcript after any turn's end already there (a turn's end is taken once, by where it ends).
    fn turn_ends(&mut self, agent: &str, text: &str) {
        self.give_all(agent);
        let transcript = self.dir.join(format!("{agent}.jsonl"));
        let mut all = std::fs::read_to_string(&transcript).unwrap_or_default();
        let offset = all.len() as u64;
        let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}], "stop_reason": "end_turn"}}));
        all.push_str(&said);
        std::fs::write(&transcript, &all).unwrap();
        self.core.ticket_turn_ended(agent, docket_machine::Loc { file: transcript.display().to_string(), offset, len: said.len() as u64 });
    }
    /// How many posts the thread has.
    fn posts(&self) -> usize {
        self.core.tickets.m.tickets[&self.t].posts.len()
    }
    /// Whom post N of the thread is handed to now.
    fn handed(&self, n: u32) -> Vec<String> {
        self.core.tickets.m.all_state(self.t, n).unwrap().handed
    }
    /// The agents whose posts answer post N.
    fn answered_by(&self, n: u32) -> Vec<String> {
        self.core.tickets.m.tickets[&self.t].posts.iter().filter(|p| p.answers == Some(n)).map(|p| p.by.clone()).collect()
    }
    /// The `decide` lines of the record (the decision `claims` asked, answered).
    fn decided(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("tickets/log.jsonl")).unwrap().lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["ev"] == "decided" && v["question"] == "decide").collect()
    }
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One of six out of a turn, five in one: his `@all` wakes the one out of a turn, though it is fourth on the thread,
/// and informs the five; its answer is posted, and one of the five answering after it posts nothing.
#[test]
fn an_all_wakes_the_one_out_of_a_turn_and_a_later_answer_is_not_posted() {
    let mut c = Coord::new("idle");
    for a in ["ann", "bob", "cid", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all who can look at the build?");
    let (to, mut inform) = c.told(n);
    inform.sort();
    assert_eq!((to, inform), (vec!["dee".to_string()], vec!["ann", "bob", "cid", "eve", "fay"].into_iter().map(String::from).collect::<Vec<_>>()), "{:?}", c.decided());
    let d = c.decided();
    assert_eq!(d.last().unwrap()["answer"], json!("dee"), "{d:?}");
    let r = c.answer("dee", n, "I take it: the build is mine.");
    assert!(r.ok, "{}", r.out);
    let r = c.answer("bob", n, "I can look at it.");
    assert!(!r.ok, "a second answer to his @all is refused");
    assert!(r.out.contains("@all") && r.out.contains("dee answered it"), "{}", r.out);
    assert_eq!(c.answered_by(n), vec!["dee".to_string()], "one answer stands, and no post says it was taken");
}

/// His `@all` in a burst beside a post of his with no choice (E1b's hold; proto-lead N-1446 p475; made up): the policy's
/// claims hands the `@all` to one, the one out of a turn, as it does a lone `@all`, and informs the five; the holder's
/// route then reads only his other post, alone (no burst, no choice of his, the `@all` not one `joins` may name), and
/// its answer sends that post. The first answer to the `@all` is the claim.
#[test]
fn an_all_in_his_burst_goes_through_claims_and_route_reads_only_the_rest() {
    let mut c = Coord::with("burst", Some("+lapse-after(\"gather\", 15).\n"));
    for a in ["ann", "bob", "cid", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let a = c.owner_says("the build is red again");
    let b = c.owner_says("@all who has the lock?");
    let t = c.t;
    let h = c.core.tickets.m.tickets.values().find(|x| x.is_open() && x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == a)).map(|x| x.t).expect("his burst held");
    assert_eq!(c.core.tickets.m.tickets[&h].post_of.as_ref().unwrap().posts(), vec![a, b], "one burst");
    let docket_machine::Status::Awaits { since, .. } = &c.core.tickets.m.tickets[&h].status else { panic!("{:?}", c.core.tickets.m.tickets[&h].status) };
    let since = chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc();
    c.core.lapse_round(since + chrono::Duration::seconds(15));
    c.settle();
    let (to, mut inform) = c.told(b);
    inform.sort();
    assert_eq!((to, inform), (vec!["dee".to_string()], vec!["ann", "bob", "cid", "eve", "fay"].into_iter().map(String::from).collect::<Vec<_>>()), "{:?}", c.decided());
    assert_eq!(c.handed(b), vec!["dee".to_string()]);
    // the holder, for a alone
    let d = c.core.ticket_due(docket_machine::WATCHER).into_iter().find(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")).expect("the holder decides his other post");
    let job = c.core.clerk_job(&d).unwrap();
    let r = job.route.as_ref().unwrap();
    assert_eq!((r.burst.clone(), r.chosen.clone()), (Vec::<u32>::new(), Vec::new()), "a alone: no burst, no choice of his");
    assert_eq!(job.text, "the build is red again");
    assert!(!r.his.contains(&b), "{:?}", r.his);
    assert!(crate::clerk::route_ok(&json!({"head": "To", "agents": ["ann"]}), r));
    assert!(!crate::clerk::route_ok(&json!({"head": "Split", "parts": [{"posts": [a], "to": ["ann"]}, {"posts": [b], "to": ["bob"]}]}), r), "b is not route's");
    c.core.ticket_answer(docket_machine::WATCHER, d.t, json!({"head": "To", "agents": ["ann"]}), Text::New(String::new())).unwrap();
    c.settle();
    assert_eq!(c.told(a).0, vec!["ann".to_string()]);
    assert_eq!(c.handed(b), vec!["dee".to_string()], "b is not told again");
    assert!(c.answer("dee", b, "Me: the lock is mine.").ok);
    assert!(!c.answer("bob", b, "me").ok, "the first answer is the claim");
}

/// All six in a turn: his `@all` still wakes one, the first on the thread, whose step comes at its turn's end; the
/// others are informed and their answers after it are not posted.
#[test]
fn with_all_in_a_turn_an_all_wakes_the_first_on_the_thread_and_the_rest_stay_quiet() {
    let mut c = Coord::new("busy");
    for a in AGENTS {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all is anyone on the red build?");
    let (to, inform) = c.told(n);
    assert_eq!(to, vec!["ann".to_string()]);
    assert_eq!(inform.len(), 5, "{inform:?}");
    assert!(c.answer("ann", n, "Yes, me.").ok);
    for a in ["cid", "fay"] {
        assert!(!c.answer(a, n, "me too").ok, "{a} posts nothing");
    }
    assert_eq!(c.answered_by(n), vec!["ann".to_string()]);
}

/// Two answers to one `@all` at the same moment: coordd takes them one after the other under its lock, so the first
/// stands and the second is not posted, whichever of them the policy woke.
#[test]
fn of_two_answers_to_an_all_at_once_the_first_is_posted_and_the_second_is_not() {
    let mut c = Coord::new("race");
    let n = c.owner_says("@all quick one: who has the runner?");
    let (to, _) = c.told(n);
    assert_eq!(to, vec!["ann".to_string()], "nobody in a turn: the first on the thread");
    let (first, second) = (c.answer("eve", n, "I have it."), c.answer("ann", n, "I have it."));
    assert!(first.ok, "{}", first.out);
    assert!(!second.ok && second.out.contains("eve answered it"), "{}", second.out);
    assert_eq!(c.answered_by(n), vec!["eve".to_string()]);
    // words without @all ask the watcher's route as before, and nothing of `claims`
    let before = c.decided().len();
    c.owner_says("thanks, carry on");
    assert_eq!(c.decided().len(), before);
    assert!(c.core.ticket_due(docket_machine::WATCHER).iter().any(|d| matches!(&d.what, docket_machine::What::Step { call, .. } if call == "route")));
}

/// His words name a ticket whose step awaits cid, who is in a turn, while dee is free: cid is handed it, the agent whose
/// work the words are about, though it is working; the others are informed, not woken.
#[test]
fn an_all_whose_words_name_a_piece_goes_to_that_pieces_agent_though_it_is_working() {
    let mut c = Coord::new("about");
    let id = c.piece_for("cid", "fix the runner's cap");
    for a in ["ann", "bob", "cid", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let n = c.owner_says(&format!("@all where is {id} at?"));
    let (to, inform) = c.told(n);
    assert_eq!(to, vec!["cid".to_string()], "{:?}", c.decided());
    assert!(!inform.contains(&"cid".to_string()) && inform.contains(&"dee".to_string()), "{inform:?}");
}

/// All six in a turn and an agent with the role "answerer": the answerer is handed it, though it is on no thread
/// (consult-lead: "else here"); the six are informed and none of them is woken.
#[test]
fn with_nobody_free_an_all_falls_to_the_answerer() {
    let mut c = Coord::new("answerer");
    assert!(c.core.set_role(&Who::Owner, "ans", "answerer").ok);
    for a in AGENTS {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all anyone?");
    let (to, inform) = c.told(n);
    assert_eq!(to, vec!["ans".to_string()], "{:?}", c.decided());
    assert_eq!(inform.len(), 6, "{inform:?}");
    assert!(c.answer("ans", n, "Here: I take it.").ok);
    assert!(!c.answer("bob", n, "me").ok);
}

/// A second `@all` while the first is unanswered is handed too, not dropped or held behind it.
#[test]
fn a_second_all_while_the_first_is_unanswered_is_handed_too() {
    let mut c = Coord::new("second");
    for a in ["ann", "bob", "cid", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let first = c.owner_says("@all who has the runner?");
    let second = c.owner_says("@all and who has the window?");
    assert_eq!(c.told(first).0, vec!["dee".to_string()]);
    assert_eq!(c.told(second).0, vec!["dee".to_string()]);
    assert!(c.core.tickets.m.tickets[&c.t].posts.iter().filter(|p| p.by == "owner").count() >= 2);
}

/// The handed agent passes it on with the pass tool: one post by ann naming eve, with its reason, that answers nothing;
/// eve is handed it. A third agent's pass is refused, as it is not its own to make. Anybody told may still claim it
/// (hold 2): bob's answer is the claim, and eve's after it is refused.
#[test]
fn the_handed_agent_passes_the_all_on_and_anybody_told_may_still_claim_it() {
    let mut c = Coord::new("pass");
    let n = c.owner_says("@all who can read the diff?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    let id = c.core.tickets.m.id(c.t);
    let r = c.pass("ann", n, "eve", Some("the diff is eve's runner"));
    assert!(r.ok && r.out.contains(&format!("eve holds {id}.p{n} now")), "{}", r.out);
    let p = c.core.tickets.m.tickets[&c.t].posts.last().unwrap().clone();
    assert_eq!((p.by.as_str(), p.names.clone(), p.passes, p.answers), ("ann", vec!["eve".to_string()], Some(n), None));
    assert_eq!(c.core.tickets.m.text(&p.text), Some(Text::New(format!("Passed {id}.p{n}, the owner's @all, to @eve: the diff is eve's runner"))));
    assert_eq!(c.handed(n), vec!["eve".to_string()]);
    assert!(c.core.ticket_due("eve").iter().any(|d| d.t == c.t), "the pass wakes eve");
    let before = c.posts();
    let r = c.pass("cid", n, "fay", None);
    assert!(!r.ok && r.out.contains("handed to eve: only they pass it on"), "{}", r.out);
    assert_eq!((c.posts(), c.handed(n)), (before, vec!["eve".to_string()]), "nothing posted, nothing moved");
    assert!(c.answer("bob", n, "I can.").ok, "anybody told may claim it");
    let r = c.answer("eve", n, "Reading it now.");
    assert!(!r.ok && r.out.contains("bob answered it"), "{}", r.out);
    assert_eq!(c.answered_by(n), vec!["bob".to_string()], "the pass answers nothing; the claim is the one answer");
    let r = c.pass("eve", n, "fay", None);
    assert!(!r.ok && r.out.contains("bob answered it") && r.out.contains("nothing is passed on"), "{}", r.out);
}

/// His `@all` passed to an agent on no thread (consult-lead N-412 post 1900: any agent of the project that can be
/// woken): `ans` is handed it, and its delivery, which brings his `@all` and the pass and none of the thread before
/// them (new to the thread, paper-research N-1446 p2059), says ann passed it and that it holds it now; its answer is the
/// claim.
#[test]
fn a_pass_to_an_agent_off_the_thread_hands_it_the_all_and_its_delivery_says_so() {
    let mut c = Coord::new("passoff");
    let n = c.owner_says("@all who knows the tower's disks?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    assert!(c.pass("ann", n, "ans", Some("ans set the tower up")).ok);
    let id = c.core.tickets.m.id(c.t);
    let d = c.core.ticket_due("ans").into_iter().find(|d| d.t == c.t).expect("ans is due the thread");
    let view = c.core.tickets.m.view(c.t).unwrap();
    let text = crate::render::delivery(&c.core.places(), &d, &view, false, false, false, false, None).unwrap().0;
    assert!(text.contains("@all who knows the tower's disks?") && text.contains("passed to you by ann") && text.contains("it is handed to you"), "{text}");
    assert!(text.contains(&format!("coord pass {id}.p{n} NAME")) && !text.contains("besides you") && !text.contains("bob here"), "{text}");
    assert!(c.answer("ans", n, "Mine: the disks are fine.").ok);
    assert_eq!(c.core.tickets.m.all_state(c.t, n).unwrap().claim.map(|(a, _)| a), Some("ans".to_string()));
}

/// No ping-pong (consult-lead N-412 post 1900): ann passes to eve, eve's pass back to ann is refused in words, and it stays
/// with eve; eve passes to fay, and fay's pass to eve, who held it, is refused too.
#[test]
fn a_pass_back_to_an_agent_that_held_the_all_and_passed_it_is_refused() {
    let mut c = Coord::new("pingpong");
    let n = c.owner_says("@all who owns the replay?");
    assert!(c.pass("ann", n, "eve", None).ok);
    let id = c.core.tickets.m.id(c.t);
    let m = c.core.tickets.m.tickets[&c.t].posts.last().unwrap().n;
    let before = c.posts();
    let r = c.pass("eve", n, "ann", Some("you had it first"));
    assert!(!r.ok && r.out.contains(&format!("ann held {id}.p{n}")) && r.out.contains(&format!("passed it on in {id}.p{m}")) && r.out.contains("stays with you"), "{}", r.out);
    assert_eq!((c.posts(), c.handed(n)), (before, vec!["eve".to_string()]));
    assert!(c.pass("eve", n, "fay", None).ok);
    let r = c.pass("fay", n, "eve", None);
    assert!(!r.ok && r.out.contains("eve held"), "{}", r.out);
    assert_eq!(c.handed(n), vec!["fay".to_string()]);
    let r = c.pass("fay", n, "fay", None);
    assert!(!r.ok && r.out.contains("handed to you already"), "{}", r.out);
}

/// The state changes between the holder's read and its pass (consult-6, N-412 post 1799): ann is given its delivery,
/// "handed to you", and cid answers before ann's pass. The pass is refused in words, naming cid's claim, and nothing is
/// posted or moved.
#[test]
fn a_told_agent_claims_between_the_holders_read_and_its_pass_so_the_pass_is_refused() {
    let mut c = Coord::new("passrace");
    let n = c.owner_says("@all who looks at the bench?");
    assert!(c.given("ann", n).contains("it is handed to you"));
    assert!(c.answer("cid", n, "I look at it.").ok);
    let before = c.posts();
    let r = c.pass("ann", n, "eve", Some("eve ran it"));
    assert!(!r.ok && r.out.contains("cid answered it"), "{}", r.out);
    assert_eq!(c.posts(), before);
    assert_eq!(c.core.tickets.m.all_state(c.t, n).unwrap().claim.map(|(a, _)| a), Some("cid".to_string()));
}

/// The mirror: eve runs when ann reads its delivery and chooses it, and stops before ann's pass reaches coordd. coordd
/// checks that it can be woken under its lock, in the same step as the post, so the pass is refused, the `@all` stays
/// with ann, and nothing is posted. (Once posted, eve is handed it as any post that names it, and a stop after that is a
/// stop after the hand-over.)
#[test]
fn the_receiver_stops_between_the_holders_read_and_its_pass_so_it_stays_with_the_holder() {
    let mut c = Coord::new("passgone");
    let n = c.owner_says("@all who has the logs?");
    assert!(c.given("ann", n).contains("it is handed to you"));
    assert!(c.core.wakeable("eve"));
    c.stop("eve");
    let before = c.posts();
    let r = c.pass("ann", n, "eve", None);
    assert!(!r.ok && r.out.contains("eve cannot be woken now") && r.out.contains("stays with you"), "{}", r.out);
    assert_eq!((c.posts(), c.handed(n)), (before, vec!["ann".to_string()]));
}

/// The pass tool sends coordd the line channel.py writes (agent-base/tests/channel-pass-test.sh checks that line):
/// coordd reads it as `Request::Pass`, `coord pass` sends the same, and its fields pass the `@all` on.
#[test]
fn the_pass_tools_request_line_is_coordds_pass() {
    let mut c = Coord::new("passline");
    let n = c.owner_says("@all who has the proxy?");
    let id = c.core.tickets.m.id(c.t);
    let line = format!(r#"{{"op": "pass", "t": "{id}", "n": {n}, "to": "dee", "why": "dee's proxy head"}}"#);
    let req: coord_types::Request = serde_json::from_str(&line).unwrap();
    let coord_types::Request::Pass { t, n: m, to, why } = req.clone() else { panic!("{req:?}") };
    assert_eq!((t.as_str(), m, to.as_str(), why.as_deref()), (id.as_str(), n, "dee", Some("dee's proxy head")));
    assert_eq!(serde_json::to_value(&req).unwrap(), serde_json::from_str::<Value>(&line).unwrap(), "coord pass sends the same line");
    let bare: coord_types::Request = serde_json::from_str(&format!(r#"{{"op": "pass", "t": "{id}", "n": {n}, "to": "dee"}}"#)).unwrap();
    assert!(matches!(bare, coord_types::Request::Pass { why: None, .. }), "{bare:?}");
    let r = c.core.ticket_pass(&Who::Agent("ann".into()), &t, m, &to, why);
    assert!(r.ok, "{}", r.out);
    assert_eq!(c.handed(n), vec!["dee".to_string()]);
}

/// The handed agent answers by its turn's end (rule Reply), as its delivery lets it: its last message, posted on the
/// thread, answers his `@all` and is the claim, so a told agent's answer after it is refused.
#[test]
fn the_handed_agents_turn_end_reply_is_its_claim() {
    let mut c = Coord::new("turnend");
    let n = c.owner_says("@all who has the train?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    let d = c.core.ticket_due("ann").into_iter().find(|d| d.t == c.t && d.upto >= n).expect("ann is due his post");
    c.core.ticket_input(Input::Given { t: d.t, to: "ann".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
    let transcript = c.dir.join("ann.jsonl");
    let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Mine: the train is at its replay."}], "stop_reason": "end_turn"}}));
    std::fs::write(&transcript, &said).unwrap();
    c.core.ticket_turn_ended("ann", docket_machine::Loc { file: transcript.display().to_string(), offset: 0, len: said.len() as u64 });
    let p = c.core.tickets.m.tickets[&c.t].posts.last().unwrap().clone();
    assert_eq!((p.by.as_str(), p.auto, p.answers), ("ann", true, Some(n)), "{p:?}");
    let r = c.answer("bob", n, "I have it.");
    assert!(!r.ok && r.out.contains("ann answered it"), "{}", r.out);
    assert!(c.answer("ann", n, "and it is green now").ok, "the claim's own agent may answer again");
}

/// The record's era lines (`docket_machine::ERA`) in DIR's record, by their place in it.
fn era_lines(dir: &std::path::Path) -> Vec<usize> {
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
    log.lines().enumerate().filter(|(_, l)| matches!(serde_json::from_str::<Line>(l), Ok(Line::Event { name, .. }) if name == docket_machine::ERA)).map(|(i, _)| i).collect()
}

/// The era of claims is kept out of a record with no `@all` of his (consult-3, N-807 post 118): none at the start, none
/// for his words without it; his first `@all` begins it, one line just before his post, and his second adds none.
#[test]
fn a_record_with_no_all_has_no_era_and_his_first_all_begins_it() {
    let mut c = Coord::new("era");
    assert_eq!((era_lines(&c.dir), c.core.tickets.m.era), (vec![], 0));
    c.owner_says("who has the cache?");
    assert!(era_lines(&c.dir).is_empty(), "his words without @all begin nothing");
    c.owner_says("@all who has the cache now?");
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    let post = log.lines().position(|l| matches!(serde_json::from_str::<Line>(l), Ok(Line::Post { by, text: Text::New(w), .. }) if by == "owner" && w.starts_with("@all"))).expect("his @all's line");
    let era = era_lines(&c.dir);
    assert!(era.len() == 1 && era[0] < post, "{era:?} before {post}");
    assert_eq!(c.core.tickets.m.era, docket_machine::ERA_CLAIMS);
    c.owner_says("@all and the bench?");
    assert_eq!(era_lines(&c.dir).len(), 1, "once");
}

/// A record from before the era (consult-3, N-807 post 118, 2): the handed agent's turn-end reply to his `@all` answered
/// nothing then, and replays so, with no claim; the start begins the era after the record's lines, and nothing else
/// differs.
#[test]
fn an_old_records_turn_end_reply_stays_no_claim_and_the_era_begins_at_the_start() {
    let mut c = Coord::new("eraold");
    let n = c.owner_says("@all who has the train?");
    c.turn_ends("ann", "Mine: the train is at its replay.");
    let p = c.core.tickets.m.tickets[&c.t].posts.last().unwrap().clone();
    assert_eq!((p.by.as_str(), p.auto, p.answers), ("ann", true, Some(n)), "in the era it is the claim: {p:?}");
    // the record as an engine from before wrote it: no era line
    let path = c.dir.join("tickets/log.jsonl");
    let log = std::fs::read_to_string(&path).unwrap();
    let old: String = log.lines().filter(|l| !matches!(serde_json::from_str::<Line>(l), Ok(Line::Event { name, .. }) if name == docket_machine::ERA)).map(|l| format!("{l}\n")).collect();
    std::fs::write(&path, &old).unwrap();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let mut again = Core::load(&c.dir.display().to_string(), &base);
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    let q = again.tickets.m.tickets[&c.t].posts.iter().find(|q| q.n == p.n).unwrap().clone();
    assert_eq!(q.answers, None, "it answered nothing then: {q:?}");
    assert_eq!(again.tickets.m.all_state(c.t, n).unwrap().claim, None);
    assert_eq!(again.tickets.m.era, docket_machine::ERA_CLAIMS);
    let lines = std::fs::read_to_string(&path).unwrap().lines().count();
    assert_eq!(era_lines(&c.dir), vec![lines - 1], "the era's line is the start's last");
}

/// The kinds of the `Opened` lines on T in the record, from line FROM on.
fn opened_on(dir: &std::path::Path, t: u64, from: usize) -> Vec<String> {
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
    log.lines().skip(from).filter_map(|l| match serde_json::from_str::<Line>(l) {
        Ok(Line::Opened { kind, parent, .. }) if parent == t => Some(kind),
        _ => None,
    }).collect()
}

/// The line of the record where AGENT's post on T is.
fn post_line(dir: &std::path::Path, t: u64, agent: &str) -> usize {
    let log = std::fs::read_to_string(dir.join("tickets/log.jsonl")).unwrap();
    log.lines().position(|l| matches!(serde_json::from_str::<Line>(l), Ok(Line::Post { t: u, by, .. }) if u == t && by == agent)).unwrap_or_else(|| panic!("no post by {agent}"))
}

/// An agent named like a ticket's number (prod's consult-3, accounts-2, here-1, which `parse_ticket` reads as tickets
/// 3, 2 and 1) is an agent on the thread from the record's era of names (docket_machine::ERA_NAMES; consult-3, N-807
/// post 118): kit-3's first post there begins it, the line just before the post. On a thread opened on 64's
/// thread.kind, which keeps its `when agent_post` after 65 (proto-lead N-1479 p274), that handler runs on its post
/// (`agent_landed`), and the owner's `@all` after it reaches kit-3 as it reaches the six.
#[test]
fn an_agent_named_like_a_ticket_is_on_the_thread_from_the_era_of_names() {
    let mut c = Coord::with_more("kitnow", None, &["kit-3"], true);
    assert!(c.core.tickets.m.era < docket_machine::ERA_NAMES, "kit-3 on no thread begins nothing");
    c.core.ticket_input(Input::Esc { by: "owner".into(), agent: "kit-3".into(), also: Vec::new(), all: false }).unwrap();
    c.settle();
    assert!(c.core.tickets.m.era < docket_machine::ERA_NAMES, "an esc to kit-3 puts it on no thread and begins nothing");
    c.core.ticket_input(Input::Post { t: c.t, by: "kit-3".into(), text: Text::New("kit-3 here".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    c.settle();
    assert_eq!(c.core.tickets.m.era, docket_machine::ERA_NAMES);
    let post = post_line(&c.dir, c.t, "kit-3");
    assert_eq!(era_lines(&c.dir).last(), Some(&(post - 1)), "the era's line is just before kit-3's post");
    assert!(opened_on(&c.dir, c.t, post).iter().any(|k| k == "agent_post"), "the thread's when agent_post ran on kit-3's post");
    let n = c.owner_says("@all who has the cache?");
    let (to, inform) = c.told(n);
    let mut reached: Vec<String> = to.iter().chain(&inform).cloned().collect();
    reached.sort();
    let mut all: Vec<String> = AGENTS.iter().chain(&["kit-3"]).map(|a| a.to_string()).collect();
    all.sort();
    assert_eq!(reached, all, "woken {to:?}, informed {inform:?}");
}

/// A record an engine from before wrote, with kit-3 on the thread (as it never knew kit-3 for an agent): the owner's
/// `@all` then left kit-3 out and its post ran no `when agent_post` (the thread opened on 64's thread.kind, which has
/// it), and they replay so, with no problem line; the start begins the era of names after the record's lines, and his
/// next `@all` reaches kit-3.
#[test]
fn an_old_records_agent_named_like_a_ticket_stays_off_and_the_start_begins_the_era_of_names() {
    let mut c = Coord::with_more("kitold", None, &["kit-3"], true);
    // an engine from before: no agent of coordd's is named like a ticket, so nothing begins the era of names
    let kit = c.core.agents.remove("kit-3").unwrap();
    c.core.ticket_input(Input::Post { t: c.t, by: "kit-3".into(), text: Text::New("kit-3 here".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    c.settle();
    let post = post_line(&c.dir, c.t, "kit-3");
    assert!(!opened_on(&c.dir, c.t, post).iter().any(|k| k == "agent_post"), "kit-3 read as a ticket");
    let n = c.owner_says("@all who has the cache?");
    let (to, inform) = c.told(n);
    assert!(!to.iter().chain(&inform).any(|a| a == "kit-3"), "kit-3 was left out then: {to:?} {inform:?}");
    assert!(c.core.tickets.m.era < docket_machine::ERA_NAMES);
    c.core.agents.insert("kit-3".into(), kit);
    // the start of this engine
    let path = c.dir.join("tickets/log.jsonl");
    let lines = std::fs::read_to_string(&path).unwrap().lines().count();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let mut again = Core::load(&c.dir.display().to_string(), &base);
    for a in again.agents.values_mut() {
        a.live.pid = Some(1);
    }
    again.load_tickets();
    assert!(again.tickets.errors.is_empty(), "{:?}", again.tickets.errors);
    assert_eq!(again.tickets.m.era, docket_machine::ERA_NAMES);
    let era = era_lines(&c.dir);
    assert!(era.last().is_some_and(|e| *e >= lines), "the era of names begins after the record's lines: {era:?}, {lines}");
    again.tickets.tasks = c.core.tickets.tasks.take();
    c.core = again;
    let m = c.owner_says("@all and the bench?");
    let (to, inform) = c.told(m);
    assert!(to.iter().chain(&inform).any(|a| a == "kit-3"), "his next @all reaches kit-3: {to:?} {inform:?}");
}

/// A last message split over threads claims by the same rule as a one-thread reply (consult-3, N-807 post 118, 1): ann
/// is handed his `@all` on N-1, then given bob's "noted" after it there and a thread of its own; its part for N-1
/// answers the `@all`, not bob's later post, and so claims it, and its part for the other thread goes there.
#[test]
fn a_split_last_messages_part_for_the_thread_claims_the_all_it_holds_there() {
    let mut c = Coord::new("splitclaim");
    let n = c.owner_says("@all who has the cache?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    c.core.ticket_input(Input::Post { t: c.t, by: "bob".into(), text: Text::New("@ann noted".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let u = c.thread_to("ann", "the window's tabs");
    let (id, uid) = (c.core.tickets.m.id(c.t), c.core.tickets.m.id(u));
    c.turn_ends("ann", &format!("#{id} Mine: the cache is warm again.\n\n#{uid} Seen, on it."));
    let p = c.core.tickets.m.tickets[&c.t].posts.iter().rev().find(|p| p.by == "ann").cloned().expect("ann's part on the @all's thread");
    assert_eq!(p.answers, Some(n), "{p:?}");
    assert_eq!(c.core.tickets.m.all_state(c.t, n).unwrap().claim.map(|(a, _)| a), Some("ann".to_string()));
    assert!(c.core.tickets.m.tickets[&u].posts.iter().any(|q| q.by == "ann"), "ann's part on {uid}");
    let r = c.answer("bob", n, "I have it.");
    assert!(!r.ok && r.out.contains("ann answered it"), "{}", r.out);
}

/// The mirror: a last message whose one part is for the other thread claims nothing on N-1, and his `@all` stays with
/// ann there.
#[test]
fn a_split_last_messages_part_for_another_thread_claims_nothing() {
    let mut c = Coord::new("splitother");
    let n = c.owner_says("@all who has the cache?");
    c.core.ticket_input(Input::Post { t: c.t, by: "bob".into(), text: Text::New("@ann noted".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    let u = c.thread_to("ann", "the window's tabs");
    let uid = c.core.tickets.m.id(u);
    c.turn_ends("ann", &format!("#{uid} Seen, on it."));
    assert!(c.core.tickets.m.tickets[&u].posts.iter().any(|q| q.by == "ann"), "ann's part on {uid}");
    assert!(!c.core.tickets.m.tickets[&c.t].posts.iter().any(|q| q.by == "ann"), "nothing of ann's on the @all's thread");
    let st = c.core.tickets.m.all_state(c.t, n).unwrap();
    assert_eq!((st.claim, st.handed), (None, vec!["ann".to_string()]));
}

/// The record's clock (`crate::core::TEST_NOW`) at second S of one minute: a giving and a pass a few seconds apart, as
/// in a session, where the record keeps whole seconds.
fn at_second(s: u32) {
    crate::core::TEST_NOW.with(|n| *n.borrow_mut() = Some(format!("2026-11-24T15:00:{s:02}Z")));
}

/// N-1104 p1015's case (paper-research N-1540 p34: a turn posts nothing at its end on a thread whose post it passed
/// during that turn): ann is handed his `@all`, passes it on to eve with the pass tool, and ends its turn with words.
/// Nothing more of ann's is posted on the thread (the rule Reply wrote it after the pass: live by `kept_reply`, as
/// p1015 was; in this harness by `reply`, an automatic post naming the owner, as the red shows), the `@all` stays with
/// eve, unclaimed, and the journal says why. A pass of an earlier turn does not count: ann's next
/// turn, handed bob's question there, is posted as its reply.
#[test]
fn a_turn_that_passed_his_all_posts_nothing_at_its_end_on_that_thread() {
    at_second(0);
    let mut c = Coord::new("passend");
    let n = c.owner_says("@all who has the bench?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    c.give_all("ann");
    at_second(5);
    assert!(c.pass("ann", n, "eve", Some("the bench is eve's")).ok);
    let before = c.posts();
    at_second(9);
    c.turn_ends("ann", "Passed it to eve: the bench runs on their tower.");
    assert_eq!(c.posts(), before, "nothing of ann's turn end on the thread: {:?}", c.core.tickets.m.tickets[&c.t].posts.last());
    let st = c.core.tickets.m.all_state(c.t, n).unwrap();
    assert_eq!((st.claim, st.handed), (None, vec!["eve".to_string()]));
    let id = c.core.tickets.m.id(c.t);
    let journal = std::fs::read_to_string(c.core.path("reconcile.jsonl")).unwrap_or_default();
    assert!(journal.contains(&format!("passed the owner's @all on {id}")), "{journal}");
    // a pass of an earlier turn: bob's question after it is answered at ann's next turn's end, on the thread
    at_second(20);
    c.core.ticket_input(Input::Post { t: c.t, by: "bob".into(), text: Text::New("@ann is the bench's disk yours?".into()), names: vec!["ann".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    c.settle();
    let asked = c.posts();
    at_second(30);
    c.turn_ends("ann", "No: the disk is the tower's.");
    let p = c.core.tickets.m.tickets[&c.t].posts.last().unwrap().clone();
    assert!(p.by == "ann" && p.auto && p.n as usize > asked, "ann's next turn's reply: {p:?}");
    crate::core::TEST_NOW.with(|n| *n.borrow_mut() = None);
}

/// The same in parts (N-807): ann, handed his `@all` and the owner's other thread, passes the `@all` on and ends with a
/// part for each thread. Its part for the `@all`'s thread is posted nowhere, and the journal says so; its part for the
/// other thread is posted there.
#[test]
fn a_split_last_messages_part_for_the_thread_it_passed_his_all_on_is_posted_nowhere() {
    at_second(0);
    let mut c = Coord::new("passsplit");
    let n = c.owner_says("@all who has the cache?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    let u = c.thread_to("ann", "the window's tabs");
    c.give_all("ann");
    at_second(5);
    assert!(c.pass("ann", n, "eve", None).ok);
    let before = c.posts();
    at_second(9);
    let (id, uid) = (c.core.tickets.m.id(c.t), c.core.tickets.m.id(u));
    c.turn_ends("ann", &format!("#{id} Passed to eve, it is theirs.\n\n#{uid} Seen, on it."));
    assert_eq!(c.posts(), before, "nothing of ann's turn end on {id}: {:?}", c.core.tickets.m.tickets[&c.t].posts.last());
    assert!(c.core.tickets.m.tickets[&u].posts.iter().any(|q| q.by == "ann"), "ann's part on {uid}");
    let st = c.core.tickets.m.all_state(c.t, n).unwrap();
    assert_eq!((st.claim, st.handed), (None, vec!["eve".to_string()]));
    let journal = std::fs::read_to_string(c.core.path("reconcile.jsonl")).unwrap_or_default();
    assert!(journal.contains(&format!("part for {id} is posted nowhere")), "{journal}");
    crate::core::TEST_NOW.with(|n| *n.borrow_mut() = None);
}

/// His `@all` posts in a record, as the head before this one holds them and as this one does (consult-3, N-807 post
/// 118, 2). The head before (a2b67f5d) took a one-line `@NAME` answer by the holder as a pass, counted taken-back
/// answers, and gave a turn-end reply no `answers`; this one passes only by `passes`, skips taken-back posts, and makes
/// the handed agent's turn-end reply its claim. One line per `@all`: its id, when, whom it was told to, whom it is
/// handed to and its claim now and before, `CHANGED` where they differ; then the counts. No words are printed.
/// `REPLAY_COORD=DIR REPLAY_BASE=DIR [REPLAY_PROJECT=FILE] <coordd test binary> --ignored --nocapture --exact
/// ticket::claims_tests::the_records_alls_now_and_before`. It only reads REPLAY_COORD.
#[test]
#[ignore]
fn the_records_alls_now_and_before() {
    let (Ok(coord), Ok(base)) = (std::env::var("REPLAY_COORD"), std::env::var("REPLAY_BASE")) else { return };
    let tk = super::Tickets::load(&coord, &base, std::env::var("REPLAY_PROJECT").ok().as_deref(), 1);
    println!("replay {coord}: {} problem lines", tk.errors.len());
    let m = &tk.m;
    // the era of names (docket_machine::ERA_NAMES): which names on the record's tickets it makes agents'
    let like: std::collections::BTreeSet<String> = m.tickets.keys().flat_map(|t| m.participants(*t)).filter(|a| m.named_like_a_ticket(a)).collect();
    println!("era {}; names like a ticket's number on its tickets: {like:?}; a start begins the era of names: {}", m.era, m.has_names_like_tickets());
    let words = |t: &Text| match m.text(t) {
        Some(Text::New(w)) => Some(w),
        _ => None,
    };
    // a2b67f5d's `passes_to`: one line beginning `@NAME`, NAME one the answer names
    let passes_to = |w: &str, names: &[String]| -> Option<String> {
        let w = w.trim();
        if w.contains('\n') {
            return None;
        }
        let first = w.strip_prefix('@')?.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).next()?;
        names.iter().find(|n| *n == first).cloned()
    };
    let (mut alls, mut changed) = (0, 0);
    for x in m.tickets.values() {
        for p in x.posts.iter().filter(|p| p.by == "owner") {
            let Some(now) = m.all_state(x.t, p.n) else { continue };
            alls += 1;
            // before: the answers to it in order; a turn-end reply (auto) answered nothing then
            let mut handed = p.told.clone();
            let mut claim: Option<(String, u32)> = None;
            for q in x.posts.iter().filter(|q| q.by != "owner" && !q.auto && q.answers == Some(p.n)) {
                match words(&q.text).and_then(|w| passes_to(&w, &q.names)) {
                    Some(to) if claim.is_none() && handed.contains(&q.by) => handed = vec![to],
                    _ if claim.is_none() => claim = Some((q.by.clone(), q.n)),
                    _ => {}
                }
            }
            let differs = now.handed != handed || now.claim != claim;
            changed += usize::from(differs);
            println!("{}.p{}\t{}\ttold {:?}\tnow {:?} {:?}\tbefore {:?} {:?}{}", m.id(x.t), p.n, p.at, p.told, now.handed, now.claim, handed, claim, if differs { "\tCHANGED" } else { "" });
        }
    }
    println!("{alls} @all posts, {changed} with another holder or claim than before");
}

/// An agent that has stopped is never handed it: ann, first on the thread and the only one out of a turn, has stopped,
/// so the first who can be woken is handed it, though it is in a turn.
#[test]
fn a_stopped_agent_first_on_the_thread_is_never_handed_an_all() {
    let mut c = Coord::new("stopped");
    c.stop("ann");
    for a in &AGENTS[1..] {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all who has the bench?");
    assert_eq!(c.told(n).0, vec!["bob".to_string()], "{:?}", c.decided());
}

/// The agent picked stops before the hand-over: the decision is asked again with the facts as they are then, and the
/// next who can be woken is handed it.
#[test]
fn an_all_is_asked_again_when_the_agent_picked_stops_before_the_hand_over() {
    let mut c = Coord::new("restop");
    let n = c.owner_says_then("@all who has the store?", |core| core.agents.get_mut("ann").unwrap().live.pid = None);
    assert_eq!(c.told(n).0, vec!["bob".to_string()], "{:?}", c.decided());
}

/// The mirror (docket-core3, N-807 post 91): the agent picked starts a turn before the hand-over; it can still be
/// woken, so it is handed it, as a turn's update.
#[test]
fn an_all_stays_with_the_agent_picked_when_it_starts_a_turn_before_the_hand_over() {
    let mut c = Coord::new("rebusy");
    let n = c.owner_says_then("@all who has the queue?", |core| core.begin_turn("ann"));
    assert_eq!(c.told(n).0, vec!["ann".to_string()], "{:?}", c.decided());
}

/// A told agent answers before the handed one: its answer is the claim, and the handed agent's delivery says who
/// answered it and that it owes nothing; an informed one's says whom it was handed to.
#[test]
fn a_told_agent_claims_before_the_handed_one_and_the_handed_ones_delivery_says_so() {
    let mut c = Coord::new("toldfirst");
    let n = c.owner_says("@all who can rerun the replay?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    let before = c.given("dee", n);
    assert!(before.contains("handed to ann") && before.contains("You owe nothing") && before.contains("claims it"), "{before}");
    let mine = c.given("ann", n);
    let id = c.core.tickets.m.id(c.t);
    assert!(mine.contains("it is handed to you") && mine.contains("If it is another agent's work, pass it on to them: the pass tool") && mine.contains(&format!("coord pass {id}.p{n} NAME")), "{mine}");
    // the hand-over's own foot may say how to split a last message (`#N-1`, `@NAME`); the @all's words ask no first word
    let all = mine.lines().find(|l| l.contains("This is the owner's @all")).unwrap_or_else(|| panic!("{mine}"));
    assert!(!all.contains("@NAME") && !all.contains("begins with"), "no first word asked: {all}");
    assert!(c.answer("cid", n, "I rerun it.").ok);
    let m = c.core.tickets.m.tickets[&c.t].posts.iter().find(|p| p.by == "cid" && p.answers == Some(n)).unwrap().n;
    let after = c.given("ann", n);
    assert!(after.contains(&format!("cid answered it in post {m}")) && after.contains("You owe nothing"), "{after}");
    assert!(!after.contains("it is handed to you"), "{after}");
    let r = c.answer("ann", n, "I can too.");
    assert!(!r.ok && r.out.contains(&format!("{id}.p{m}")), "{}", r.out);
}

/// His `@all` another agent claimed is settled for the agent it was handed to (consult-3 N-412.p2203, 1; p2210, 1):
/// not its to answer, for `taken` refuses that answer. (Ann first answers the thread's first post, his to her, so
/// what she is given after it holds no other post of his.) Put into the running turn of the agent it is handed to and
/// claimed by another, that turn ending with Nothing reads it: not due again, not owed. Given at the start of a turn
/// and claimed by another agent, that turn cut reads it too. One nobody claimed, its turn cut, comes again, as any post
/// of his.
#[test]
fn his_all_another_agent_claimed_is_read_at_the_handed_agents_turn_end_or_cut() {
    let mut c = Coord::new("allsettled");
    c.core.ticket_input(Input::Event { name: docket_machine::ERA.into(), value: json!({"era": docket_machine::ERA_PUT_IN}) }).unwrap();
    let has = |d: &docket_machine::Delivery, n: u32| matches!(&d.what, docket_machine::What::Posts { unread } if unread.iter().any(|p| p.n == n));
    let read = |c: &Coord, a: &str, d: &str| c.core.tickets.m.tickets[&c.t].given.iter().rev().find(|g| g.to == a && g.delivery == d).is_some_and(|g| g.read.is_some());
    c.turn_ends("ann", "On it.");
    assert!(c.core.tickets.m.owed_posts("ann").is_empty());
    // put into the running turn of the agent it is handed to, claimed by another; that turn ends with Nothing
    let n = c.owner_says("@all who can rerun the replay?");
    let k = c.told(n).0[0].clone();
    let other = AGENTS.iter().find(|a| **a != k).unwrap().to_string();
    let d = c.core.ticket_due(&k).into_iter().find(|d| has(d, n)).expect("handed");
    c.core.ticket_input(Input::GivenInto { t: c.t, to: k.clone(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
    assert!(c.answer(&other, n, "I rerun it.").ok);
    // its transcript, after any turn's end already in it
    let transcript = c.dir.join(format!("{k}.jsonl"));
    let mut all = std::fs::read_to_string(&transcript).unwrap_or_default();
    let offset = all.len() as u64;
    let said = format!("{}\n", json!({"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": "Nothing"}], "stop_reason": "end_turn"}}));
    all.push_str(&said);
    std::fs::write(&transcript, &all).unwrap();
    c.core.ticket_turn_ended(&k, docket_machine::Loc { file: transcript.display().to_string(), offset, len: said.len() as u64 });
    assert!(read(&c, &k, &d.id), "read at its turn's end: {:?}", c.core.tickets.m.tickets[&c.t].given);
    assert!(!c.core.ticket_due(&k).iter().any(|e| has(e, n)) && c.core.tickets.m.owed_posts(&k).is_empty(), "not due again, not owed");
    // given at the start of a turn of the agent it is handed to, claimed by another, that turn cut: read
    let n2 = c.owner_says("@all who has the stills?");
    let k = c.told(n2).0[0].clone();
    let other = AGENTS.iter().find(|a| **a != k).unwrap().to_string();
    let d2 = c.core.ticket_due(&k).into_iter().find(|d| has(d, n2)).expect("handed");
    c.core.ticket_input(Input::Given { t: c.t, to: k.clone(), delivery: d2.id.clone(), upto: d2.upto, skipped: Default::default(), place: None }).unwrap();
    assert!(c.answer(&other, n2, "I have them.").ok);
    c.core.ticket_input(Input::TurnCut { agent: k.clone() }).unwrap();
    assert!(read(&c, &k, &d2.id), "read at the cut: {:?}", c.core.tickets.m.tickets[&c.t].given);
    assert!(!c.core.ticket_due(&k).iter().any(|e| has(e, n2)) && c.core.tickets.m.owed_posts(&k).is_empty(), "not due again, not owed");
    // one nobody claimed, its turn cut: due again
    let n3 = c.owner_says("@all is the build green?");
    let k = c.told(n3).0[0].clone();
    let d3 = c.core.ticket_due(&k).into_iter().find(|d| has(d, n3)).expect("handed");
    c.core.ticket_input(Input::Given { t: c.t, to: k.clone(), delivery: d3.id.clone(), upto: d3.upto, skipped: Default::default(), place: None }).unwrap();
    c.core.ticket_input(Input::TurnCut { agent: k.clone() }).unwrap();
    assert!(c.core.ticket_due(&k).iter().any(|e| has(e, n3)), "unclaimed: due again");
}

/// Every answer is a claim, whatever its words (consult-lead N-412 post 1900): one line beginning `@bob` is the claim,
/// as is an answer of three lines naming two agents, and the next answer is refused.
#[test]
fn an_answer_naming_agents_is_a_claim_whatever_its_first_word() {
    let mut c = Coord::new("lines");
    let n = c.owner_says("@all who splits the work?");
    let r = c.answer("ann", n, "@bob yours");
    assert!(r.ok, "{}", r.out);
    assert_eq!(c.core.tickets.m.all_state(c.t, n).unwrap(), docket_machine::AllState { handed: vec!["ann".to_string()], claim: Some(("ann".to_string(), c.core.tickets.m.tickets[&c.t].posts.last().unwrap().n)), passed: Vec::new() });
    let r = c.answer("bob", n, "ok, the runner");
    assert!(!r.ok && r.out.contains("ann answered it"), "{}", r.out);
    let m = c.owner_says("@all and who splits the next one?");
    let who = c.told(m).0[0].clone();
    assert!(c.answer(&who, m, "@bob could take the runner\n@cid the window\nI take the rest.").ok);
    let r = c.answer("cid", m, "the window, then");
    assert!(!r.ok && r.out.contains("answered it"), "{}", r.out);
}

/// Passing it to an agent that has stopped is refused, and it stays with the one it was handed to.
#[test]
fn passing_an_all_to_a_stopped_agent_is_refused() {
    let mut c = Coord::new("passstop");
    c.stop("eve");
    let n = c.owner_says("@all who reads the log?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()]);
    let r = c.pass("ann", n, "eve", Some("eve wrote it"));
    assert!(!r.ok && r.out.contains("eve cannot be woken"), "{}", r.out);
    assert_eq!(c.handed(n), vec!["ann".to_string()]);
    let r = c.pass("ann", n, "nobody", None);
    assert!(!r.ok && r.out.contains("no agent named nobody"), "{}", r.out);
}

/// Two agents the words are about, bob on the thread and `ans` off it (docket-core3, N-807 post 91, note 1): the one on
/// the thread is handed it; one off it has no among(A, I), and must not win the tie for that.
#[test]
fn of_two_agents_the_words_are_about_the_one_on_the_thread_is_handed_it() {
    let mut c = Coord::new("abouttie");
    let mine = c.piece_for("bob", "the runner's cap");
    let other = c.piece_for("ans", "the window's tabs");
    for a in ["ann", "cid", "dee", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let n = c.owner_says(&format!("@all where are {other} and {mine}?"));
    assert_eq!(c.told(n).0, vec!["bob".to_string()], "{:?}", c.decided());
}

/// A project's own policy file gives `ans` the role "answerer" (`+role("ans", "answerer").`, paper-research N-412 post
/// 1518): with all six in a turn it is handed his `@all`, with no `coord role`.
#[test]
fn a_projects_policy_text_makes_the_answerer() {
    let mut c = Coord::with("projrole", Some("// the project's answerer\n+role(\"ans\", \"answerer\").\n"));
    for a in AGENTS {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all anyone?");
    assert_eq!(c.told(n).0, vec!["ans".to_string()], "{:?}", c.decided());
}

/// A restart replays the record (docket-core3, N-807 post 91): his `@all`, the `decided` line of `claims`, and a told
/// agent's claim come back as they were, from the record's lines alone. Nothing is decided again (the replayed coord
/// has no task channel and no running agent, so a choice made again would differ), and nothing is written.
#[test]
fn an_all_and_its_claim_replay_from_the_record_without_deciding_again() {
    let mut c = Coord::new("replay");
    for a in ["ann", "bob", "cid", "eve", "fay"] {
        c.core.begin_turn(a);
    }
    let n = c.owner_says("@all who has the build?");
    assert_eq!(c.told(n).0, vec!["dee".to_string()]);
    assert!(c.answer("cid", n, "Mine.").ok);
    let lines = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap().lines().count();
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let mut again = Core::load(&c.dir.display().to_string(), &base);
    again.load_tickets();
    assert_eq!(again.tickets.m.tickets[&c.t].posts, c.core.tickets.m.tickets[&c.t].posts, "the same posts, told, informed and answers");
    assert_eq!(again.tickets.m.all_state(c.t, n), c.core.tickets.m.all_state(c.t, n));
    assert_eq!(again.tickets.m.all_state(c.t, n).unwrap().claim.map(|(a, _)| a), Some("cid".to_string()));
    // the only line a start writes is the install's own `held_handed` (none was at this coord's first start, on an
    // empty record): nothing decided, told or posted again
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    let new: Vec<&str> = log.lines().skip(lines).filter(|l| !l.contains(&format!("\"{}\"", docket_machine::HELD_HANDED))).collect();
    assert!(new.is_empty(), "the replay wrote nothing: {new:?}");
    // after the restart a later answer is refused as before
    let id = again.tickets.m.id(c.t);
    let r = again.ticket_post(&Who::Agent("dee".into()), &id, "I have it too.".into(), None, Some(n), Said::default(), None, false);
    assert!(!r.ok && r.out.contains("cid answered it"), "{}", r.out);
}

/// The way back's after-replay step (proto-lead N-1446 p431, 2): a record from a later release whose handler of his
/// post holds a program this release cannot parse (a word no release reads, as 0.3's `gather` is to 38; `gather`
/// parses from E1b on), and no line told the post. The start after the replay sends it on, by one `untold` line: it
/// goes to this release's own `owner_post`, a handler of its own numbered past the record's, his thread not broken.
/// With no hold in the policy its route is due to the watcher at once; under `+lapse-after("gather", 15).` the line
/// carries the hold, as his typed post's does (proto-lead p819 (b)), and the holder is asked at the hold's end. The
/// next start finds it told and writes nothing for it.
#[test]
fn his_post_a_later_release_left_untold_goes_to_this_releases_handler_at_the_start() {
    for (tag, policy) in [("untold", None), ("untold-held", Some("+lapse-after(\"gather\", 15).\n"))] {
        let mut c = Coord::with(tag, policy);
        let t = c.t;
        c.core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("the lock again".into()), names: vec![], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
        let n = c.core.tickets.m.tickets[&t].last();
        let holds = move |core: &Core| core.tickets.m.tickets.values().find(|x| x.post_of.as_ref().is_some_and(|p| p.t == t && p.n == n)).map(|x| x.t);
        let old = holds(&c.core).expect("a handler holds it");
        let path = c.dir.join("tickets/log.jsonl");
        let mut lines: Vec<Value> = std::fs::read_to_string(&path).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let opened = lines.iter_mut().filter(|v| v["ev"] == "opened" && v["t"] == old).map(|v| v["program"] = Value::String(format!("p <- @@ a later release's word @@;\n{}", v["program"].as_str().unwrap()))).count();
        assert_eq!(opened, 1);
        std::fs::write(&path, lines.iter().map(|v| format!("{v}\n")).collect::<String>()).unwrap();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let start = || {
            let before = std::fs::read_to_string(&path).unwrap().lines().count();
            let mut core = Core::load(&c.dir.display().to_string(), &base);
            // its project, and so its policy, before the tickets, as main.rs loads them
            core.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
            core.load_tickets();
            let log = std::fs::read_to_string(&path).unwrap();
            let new: Vec<Value> = log.lines().skip(before).map(|l| serde_json::from_str::<Value>(l).unwrap()).filter(|v| v["name"] != docket_machine::HELD_HANDED).collect();
            (core, new)
        };
        let (mut core, new) = start();
        let value = if policy.is_some() { json!({"t": t, "n": n, "hold": 15}) } else { json!({"t": t, "n": n}) };
        assert_eq!(new.first().map(|v| (v["ev"].clone(), v["name"].clone(), v["value"].clone())), Some((json!("event"), json!(docket_machine::UNTOLD), value)), "{tag}: {new:#?}");
        let x = &core.tickets.m.tickets[&t];
        assert_eq!(x.broken, None);
        let h = holds(&core).expect("this release's handler holds it");
        assert!(h > old && !core.tickets.m.tickets.contains_key(&old), "{h} {old}");
        if policy.is_some() {
            // held as his typed post is: the holder is asked at the hold's end
            let docket_machine::Status::Awaits { agent, owed, since, .. } = &core.tickets.m.tickets[&h].status else { panic!("{:?}", core.tickets.m.tickets[&h].status) };
            assert_eq!((agent.as_str(), owed.call.as_str()), ("owner", docket_machine::syntax::GATHER), "{tag}");
            let since = chrono::DateTime::parse_from_rfc3339(since).unwrap().to_utc();
            core.lapse_round(since + chrono::Duration::seconds(15));
        }
        assert!(matches!(&core.tickets.m.tickets[&h].status, docket_machine::Status::Awaits { agent, owed, .. } if agent == docket_machine::WATCHER && owed.call == "route"), "{tag}: {:?}", core.tickets.m.tickets[&h].status);
        assert!(core.tickets.m.untold_posts().is_empty());
        // the next start replays those lines as they ran, and sends nothing on
        let (core, new) = start();
        assert!(new.is_empty(), "{tag}: {new:#?}");
        assert_eq!(holds(&core), Some(h));
        assert!(matches!(&core.tickets.m.tickets[&h].status, docket_machine::Status::Awaits { agent, .. } if agent == docket_machine::WATCHER), "{tag}: {:?}", core.tickets.m.tickets[&h].status);
    }
}

/// Nobody can be woken (consult-lead N-807 post 98, change 2): it is told to the first on the ticket, where it waits for
/// that agent's start, and his side shows that: the post's `told` names ann in the ticket's view, and ann's own view says
/// it is stopped, which the window draws under his post (`ann · not running`).
#[test]
fn an_all_with_every_agent_stopped_waits_for_the_first_and_the_view_says_it_is_not_running() {
    let mut c = Coord::new("allstopped");
    for a in AGENTS {
        c.stop(a);
    }
    let n = c.owner_says("@all is anyone there?");
    assert_eq!(c.told(n).0, vec!["ann".to_string()], "{:?}", c.decided());
    let view = c.core.tickets.m.view(c.t).unwrap();
    assert!(view.posts.iter().any(|p| p.n == n && p.told == vec!["ann".to_string()]), "{:?}", view.posts.iter().find(|p| p.n == n));
    assert_eq!(c.core.view("ann").state, "stopped");
}
