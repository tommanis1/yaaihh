//! std.delivery and the bridges' three ways to get it to a session, on the Machine: what each harness is given, in
//! what order and how many times, with a driver that writes down every Effect it carries (paper-research N-1978).
//! The same flows, step by step and fact by fact, are docket/scenarios/s11 to s13 on the booted daemon, each step
//! checked whole there (`Core::checked_guided`); here the core is the plain one, as the daemon runs it (the checked
//! core takes over 5 min for these runs even with post-v041-ways-rc2: 6ce0401dd's run).

use docket_v2::check::names::Sym;
use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::norms::{Moved, NormDecl, Norms, Step, Why};
use docket_v2::run::record::Line;
use docket_v2::run::value::{Env, Instance, Value};
use docket_v2::syntax::tree::Expr;
use std::cell::RefCell;
use std::rc::Rc;

const TEXTS: [&str; 9] = [
    "std/hooks.docket", "std/comms.docket", "std/rails.docket", "std/budgets.docket", "std/delivery.docket",
    "std/bridges/claude-code.docket", "std/bridges/codex.docket", "std/bridges/agy.docket", "scenarios/s11-delivery-world.docket",
];
const DELIVERED: [&str; 5] = ["std.bridges.claude_code.Push-Channel", "std.bridges.claude_code.Push-Channel-Now", "std.bridges.codex.Codex-Start-Turn", "std.bridges.codex.Codex-Steer", "std.bridges.agy.Agy-Kick"];

type Carried = Rc<RefCell<Vec<(String, String, String)>>>;

/// Carries the five delivery Effects, answering each at once and writing down (Effect, agent, words); names every
/// other Effect of the texts so they load, and fails it; performs the clock's, agy-rpc's and the session driver's
/// Events.
struct Sessions(Vec<String>, Carried);

impl Driver for Sessions {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("sessions")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.0.iter().map(|e| (e.clone(), vec![])).collect()
    }
    fn events(&self) -> Vec<String> {
        ["std.budgets.Tick", "std.bridges.agy.Session-Kicked-Off", "std.hooks.Session-Started"].map(str::to_string).to_vec()
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        if !DELIVERED.contains(&op) {
            return Reply::Failed(format!("{op} is not carried in this test"));
        }
        let [Value::Text(to), Value::Text(words)] = args else { return Reply::Failed(format!("{op}: {args:?}")) };
        self.1.borrow_mut().push((op.rsplit('.').next().unwrap().to_string(), to.clone(), words.clone()));
        Reply::Answer { value: Value::Unit, costs: vec![], events: vec![] }
    }
    fn dropped(&mut self, _: u64) {}
}

/// DELIVERY_CHECKED set: on `Core::checked()`, for its time (consult-lead-c N-1978 p10); else the plain core.
fn machine() -> (Machine<Core>, Carried) {
    machine_on(if std::env::var_os("DELIVERY_CHECKED").is_some() { Core::checked() } else { Core::default() })
}

/// The texts on the core N, with the Sessions driver.
fn machine_on<N: Norms>(n: N) -> (Machine<N>, Carried) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docket");
    let read = |f: &str| docket_v2::syntax::read(&std::fs::read_to_string(root.join(f)).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
    let loaded = docket_v2::check::load(TEXTS.map(read).to_vec()).unwrap_or_else(|e| panic!("{e}"));
    let effects = loaded.syms.iter().filter(|(_, s)| matches!(s, Sym::Effect { does: None, .. })).map(|(n, _)| n.clone()).collect();
    let carried = Carried::default();
    (Machine::new(loaded, n, vec![Box::new(Sessions(effects, carried.clone()))]).unwrap(), carried)
}

/// A field as a script writes it: digits an Int, true/false a Bool, else a text.
fn val(a: &str) -> Value {
    match a {
        "true" | "false" => Value::Bool(a == "true"),
        _ => a.parse().map_or_else(|_| Value::text(a), Value::Int),
    }
}

fn inst(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: ty.into(), args: args.iter().map(|a| val(a)).collect() }
}

/// TY(ARGS) asked for by BY, run until it rests or ends; its run's state.
fn act<N: Norms>(m: &mut Machine<N>, by: &str, ty: &str, args: &[&str]) -> State {
    let r = m.start(Value::text(by), Some(0), "", inst(ty, args)).unwrap_or_else(|e| panic!("{ty}: {e}"));
    m.run();
    m.state(r)
}

fn ok<N: Norms>(m: &mut Machine<N>, by: &str, ty: &str, args: &[&str]) {
    let s = act(m, by, ty, args);
    assert_eq!(s, State::Done, "{ty}{args:?}: {:?}", m.record.iter().rev().take(6).collect::<Vec<_>>());
}

fn event<N: Norms>(m: &mut Machine<N>, ty: &str, args: &[&str]) {
    m.event(0, inst(ty, args));
    m.run();
}

fn holds<N: Norms>(m: &mut Machine<N>, ty: &str, args: &[&str]) -> bool {
    m.norms.holds(&inst(ty, args))
}

/// The Effects carried since the last look, as (Effect, agent), and the words of each.
fn taken(c: &Carried) -> (Vec<(String, String)>, Vec<String>) {
    let all: Vec<_> = c.borrow_mut().drain(..).collect();
    (all.iter().map(|(e, a, _)| (e.clone(), a.clone())).collect(), all.into_iter().map(|(_, _, w)| w).collect())
}

fn one(e: &str, a: &str) -> Vec<(String, String)> {
    vec![(e.to_string(), a.to_string())]
}

const POST: &str = "std.delivery.Post";
const REACH: &str = "std.hooks.Reach";

#[test]
fn claude_code_is_pushed_once_when_idle_and_once_at_a_turns_end_whatever_came_during_it_and_interrupted_at_once() {
    let (mut m, c) = machine();
    ok(&mut m, "o1", POST, &["o1", "w1", "m1", "one", "Normal"]);
    let (e, w) = taken(&c);
    assert_eq!((e, w[0].contains("[m1 from o1] one")), (one("Push-Channel", "w1"), true), "idle: pushed at once");
    // five words while woken and in its turn: nothing pushed
    ok(&mut m, "o1", POST, &["o1", "w1", "m2", "two", "Normal"]);
    ok(&mut m, "w1", REACH, &["w1", "turn_begins"]);
    for (id, text) in [("m3", "three"), ("m4", "four"), ("m5", "five")] {
        ok(&mut m, "o1", POST, &["o1", "w1", id, text, "Normal"]);
    }
    ok(&mut m, "brain", "std.delivery.Direct", &["brain", "w1", "first things first", "9"]);
    assert_eq!(taken(&c).0, vec![], "nothing typed into a turn");
    // a director's interrupting word goes in at once; another party's is refused
    ok(&mut m, "o1", POST, &["o1", "w1", "m6", "stop", "Interrupt"]);
    assert_eq!(taken(&c).0, one("Push-Channel-Now", "w1"));
    assert_eq!(act(&mut m, "w2", POST, &["w2", "w1", "m7", "x", "Interrupt"]), State::Raised);
    // the turn's end: one push with all four and the directive first; the two handed before are read
    ok(&mut m, "w1", REACH, &["w1", "turn_end"]);
    let (e, w) = taken(&c);
    assert_eq!(e, one("Push-Channel", "w1"), "one push for the whole burst");
    assert!(w[0].starts_with("[coord delivery]\nFirst: first things first\n") && ["m2", "m3", "m4", "m5"].iter().all(|m| w[0].contains(&format!("[{m} from o1]"))), "{}", w[0]);
    assert!(!holds(&mut m, "std.delivery.Unread", &["w1", "m1", "Normal"]) && !holds(&mut m, "std.delivery.Unread", &["w1", "m6", "Interrupt"]));
    assert!(holds(&mut m, "std.delivery.In-Flight", &["w1", "m5"]));
    // an agent not delivered to is pushed nothing
    ok(&mut m, "o1", POST, &["o1", "w2", "z1", "hello", "Normal"]);
    assert_eq!(taken(&c).0, vec![]);
}

#[test]
fn codex_is_given_one_turn_for_a_burst_and_no_word_while_at_its_own_approval_question() {
    let (mut m, c) = machine();
    ok(&mut m, "o1", POST, &["o1", "x1", "n1", "a", "Normal"]);
    let (e, w) = taken(&c);
    assert_eq!(e, one("Codex-Start-Turn", "x1"));
    assert!(w[0].starts_with("CONTEXT\n") && w[0].contains("\nWRITE\n[n1 from o1] a\nRECONCILE\n"), "{}", w[0]);
    ok(&mut m, "x1", REACH, &["x1", "turn_begins"]);
    for k in 2..=9 {
        ok(&mut m, "o1", POST, &["o1", "x1", &format!("n{k}"), "more", "Normal"]);
    }
    ok(&mut m, "x1", REACH, &["x1", "permission_asked"]);
    ok(&mut m, "o1", POST, &["o1", "x1", "q1", "now", "Interrupt"]);
    assert_eq!(taken(&c).0, vec![], "eight words and an interrupt at its question: no turn, no steer");
    // past the question the word held at it is steered in then (N-1978 p5, point 3); a later one at once
    ok(&mut m, "x1", REACH, &["x1", "tool_result"]);
    let (e, w) = taken(&c);
    assert_eq!((e, w[0].contains("[q1 from o1] now")), (one("Codex-Steer", "x1"), true), "the held interrupt, at the question's end");
    ok(&mut m, "o1", POST, &["o1", "x1", "q2", "now", "Interrupt"]);
    assert_eq!(taken(&c).0, one("Codex-Steer", "x1"), "past the question, steered");
    ok(&mut m, "x1", REACH, &["x1", "turn_end"]);
    let (e, w) = taken(&c);
    assert_eq!(e, one("Codex-Start-Turn", "x1"), "one turn for the eight waiting");
    assert_eq!(w[0].matches(" from o1] ").count(), 8, "{}", w[0]);
    ok(&mut m, "x1", REACH, &["x1", "turn_begins"]);
    ok(&mut m, "x1", REACH, &["x1", "turn_end"]);
    assert_eq!(taken(&c).0, vec![], "a turn's end with nothing new starts none");
    assert!(holds(&mut m, "std.delivery.Idle", &["x1"]));
}

#[test]
fn agy_is_never_given_anything_pulls_its_briefing_and_a_late_check_in_is_told_once() {
    let (mut m, c) = machine();
    event(&mut m, "std.budgets.Tick", &["50"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k1", "read the spec", "Interrupt"]);
    ok(&mut m, "o1", "std.rails.Assign", &["o1", "g1", "legend"]);
    event(&mut m, "std.hooks.Session-Started", &["g1", "agy", "agy-1.5.18"]);
    assert!(holds(&mut m, "std.bridges.agy.Kicked-Off", &["g1", "false"]) && holds(&mut m, "std.bridges.agy.Next-Expected-Pull", &["g1", "55"]));
    let r = m.start(Value::text("g1"), Some(0), "", inst("std.bridges.agy.Pull-Brain", &["g1", "first"])).unwrap();
    m.run();
    assert_eq!(m.state(r), State::Done);
    let brief = m.record.iter().find_map(|l| match l {
        Line::Ended { run, value: Value::Record { fields, .. } } if *run == m.run_name(r) => Some(fields.clone()),
        _ => None,
    });
    let brief = brief.expect("the pull ends with its briefing");
    let field = |k: &str| brief.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap();
    assert_eq!(field("mode"), Value::text("work"));
    assert_eq!(field("unread_messages"), Value::List(vec![Value::text("[k1 from o1] read the spec")]));
    assert_eq!(field("assigned_items"), Value::List(vec![Value::text("legend")]));
    assert_eq!(field("suggested_wake_mins"), Value::Int(10));
    // late: told once at the first minute past the wake and the grace, and again only after a later pull is missed
    for minute in 60..=66 {
        event(&mut m, "std.budgets.Tick", &[&minute.to_string()]);
    }
    let told = |m: &Machine<Core>| m.record.iter().filter(|l| matches!(l, Line::Performed { act, .. } if act.ty == "std.bridges.agy.Pulse-Anomaly")).count();
    assert_eq!(told(&m), 1);
    assert!(holds(&mut m, "std.bridges.agy.Anomaly", &["agy_missed_checkin", "g1", "65"]));
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "late"]);
    for minute in 67..=90 {
        event(&mut m, "std.budgets.Tick", &[&minute.to_string()]);
    }
    assert_eq!(told(&m), 2, "the pull at 66 was due by 66 + 10 + 5");
    ok(&mut m, "g1", "std.delivery.Report-Work", &["g1", "done", "c1", ""]);
    assert_eq!(taken(&c).0, vec![], "nothing ever reached the agy session");
}

/// N-1978 p5, point 2: an agent delivered to whose session is not running is typed nothing, an interrupting word
/// neither, and what waited is given when its session starts: pushed on Claude Code, one turn on Codex.
#[test]
fn an_agent_not_running_is_given_nothing_until_its_session_starts_and_then_what_waited() {
    let (mut m, c) = machine();
    ok(&mut m, "o1", POST, &["o1", "w3", "u1", "start with the map", "Interrupt"]);
    ok(&mut m, "o1", POST, &["o1", "x3", "u2", "the build first", "Interrupt"]);
    ok(&mut m, "brain", "std.delivery.Direct", &["brain", "x3", "tests first", "2"]);
    assert_eq!(taken(&c).0, vec![], "no session: nothing typed");
    event(&mut m, "std.hooks.Session-Started", &["w3", "claude-code", "2.1.332"]);
    let (e, w) = taken(&c);
    assert_eq!((e, w[0].contains("[u1 from o1] start with the map")), (one("Push-Channel", "w3"), true));
    event(&mut m, "std.hooks.Session-Started", &["x3", "codex", "0.160.0"]);
    let (e, w) = taken(&c);
    assert_eq!(e, one("Codex-Start-Turn", "x3"));
    assert!(w[0].contains("First: tests first\n") && w[0].contains("[u2 from o1] the build first"), "{}", w[0]);
    // a session started again: what the one before was handed and never read is given again
    event(&mut m, "std.hooks.Session-Started", &["w3", "claude-code", "2.1.332"]);
    let (e, w) = taken(&c);
    assert_eq!((e, w[0].contains("[u1 from o1]")), (one("Push-Channel", "w3"), true));
}

/// N-1978 p5, point 4: a paused agy pull hands nothing, and an unpause gives its next pull the grace from then.
#[test]
fn a_paused_agy_pull_hands_nothing_and_an_unpause_starts_its_wake_over() {
    let (mut m, c) = machine();
    event(&mut m, "std.budgets.Tick", &["10"]);
    event(&mut m, "std.hooks.Session-Started", &["g1", "agy", "agy-1.5.18"]);
    ok(&mut m, "o1", "std.bridges.agy.Pause", &["o1", "g1"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k1", "the release is tonight", "Normal"]);
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "paused"]);
    assert!(!holds(&mut m, "std.delivery.In-Flight", &["g1", "k1"]), "nothing handed while paused");
    for minute in 11..=60 {
        event(&mut m, "std.budgets.Tick", &[&minute.to_string()]);
    }
    ok(&mut m, "o1", "std.bridges.agy.Unpause", &["o1", "g1"]);
    assert!(holds(&mut m, "std.bridges.agy.Next-Expected-Pull", &["g1", "65"]));
    for minute in 61..=65 {
        event(&mut m, "std.budgets.Tick", &[&minute.to_string()]);
    }
    let told = m.record.iter().filter(|l| matches!(l, Line::Performed { act, .. } if act.ty == "std.bridges.agy.Pulse-Anomaly")).count();
    assert_eq!(told, 0, "a pause is no missed check-in");
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "back"]);
    assert!(holds(&mut m, "std.delivery.In-Flight", &["g1", "k1"]), "told now");
    assert_eq!(taken(&c).0, vec![]);
}

/// N-1998 p3, point 1: the owner's own word to an agy agent goes in at once as a turn of its own, once it has pulled
/// since its kick-off and while no turn runs, one word to a turn; before that pull, in a turn, paused, or a
/// director's word that is not his, it waits for a pull. Typed in, it is not handed: the pull tells it. v1 does the
/// same since docket-edge4's cfb0ed2e9.
#[test]
fn the_owners_word_kicks_an_agy_agent_at_once_after_its_first_pull_and_never_into_a_turn() {
    let (mut m, c) = machine();
    event(&mut m, "std.budgets.Tick", &["10"]);
    event(&mut m, "std.hooks.Session-Started", &["g1", "agy", "agy-1.5.18"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k1", "before the pull", "Normal"]);
    assert_eq!(taken(&c).0, vec![], "before its first pull: the kick-off carries it");
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "first"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k2", "the gate is red", "Normal"]);
    let (e, w) = taken(&c);
    assert_eq!((e, w[0].as_str()), (one("Agy-Kick", "g1"), "[coord delivery, from the owner] [k2 from o1] the gate is red"));
    assert!(!holds(&mut m, "std.delivery.In-Flight", &["g1", "k2"]), "typed in, not handed: the turn's pull tells it (N-1998 p7, point 1)");
    ok(&mut m, "o1", POST, &["o1", "g1", "k3", "and the notes", "Normal"]);
    assert_eq!(taken(&c).0, vec![], "one word typed in is one turn");
    ok(&mut m, "g1", REACH, &["g1", "turn_begins"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k4", "stop", "Interrupt"]);
    assert_eq!(taken(&c).0, vec![], "never into a turn");
    ok(&mut m, "g1", REACH, &["g1", "turn_end"]);
    ok(&mut m, "brain", POST, &["brain", "g1", "k5", "from the brain", "Normal"]);
    assert_eq!(taken(&c).0, vec![], "a director's word that is not his waits");
    ok(&mut m, "o1", "std.bridges.agy.Pause", &["o1", "g1"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k6", "while paused", "Normal"]);
    assert_eq!(taken(&c).0, vec![], "paused, his word waits");
    ok(&mut m, "o1", "std.bridges.agy.Unpause", &["o1", "g1"]);
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "back"]);
    for k in ["k2", "k3", "k4", "k5", "k6"] {
        assert!(holds(&mut m, "std.delivery.In-Flight", &["g1", k]), "{k} told at the pull");
    }
    // its session kicked off again: before its next pull, his word waits once more
    event(&mut m, "std.hooks.Session-Started", &["g1", "agy", "agy-1.5.18"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k7", "after the restart", "Normal"]);
    assert_eq!(taken(&c).0, vec![]);
}

/// The daemon's core, every binding its `each`es try counted ([`Core::tried`]), as tests/cost.rs counts them.
/// Each `each`: its condition's written form and the bindings it tried.
type Eaches = Rc<RefCell<Vec<(String, u64)>>>;
struct Counted(Core, Eaches);

impl Norms for Counted {
    fn load(&mut self, decls: &[NormDecl]) -> Result<(), String> {
        self.0.load(decls)
    }
    fn holds(&mut self, i: &Instance) -> bool {
        self.0.holds(i)
    }
    fn cond(&mut self, e: &Expr, env: &Env) -> Result<bool, String> {
        self.0.cond(e, env)
    }
    fn each(&mut self, e: &Expr, env: &Env) -> Result<Vec<Env>, String> {
        let before = self.0.tried();
        let rows = self.0.each(e, env)?;
        self.1.borrow_mut().push((docket_v2::syntax::form::expr(e).to_string(), self.0.tried() - before));
        Ok(rows)
    }
    fn step(&mut self, act: &Instance, party: Option<&Value>) -> Result<Step, Why> {
        self.0.step(act, party)
    }
    fn commit(&mut self, step: Step) {
        self.0.commit(step)
    }
    fn view(&mut self, k: Option<usize>) {
        self.0.view(k)
    }
    fn reads(&mut self) -> [u8; 32] {
        self.0.reads()
    }
    fn moved(&mut self) -> Moved {
        self.0.moved()
    }
    fn unwatch(&mut self) {
        self.0.unwatch()
    }
    fn cap(&mut self, steps: Option<u64>) {
        self.0.cap(steps)
    }
}

/// The bindings tried by one Codex `tool_result` that steers a held word in, and by one agy pull, after N messages
/// posted to an agent nobody delivers to.
fn tried_with(n: usize) -> (Vec<(String, u64)>, Vec<(String, u64)>) {
    let tried: Eaches = Default::default();
    let (mut m, c) = machine_on(Counted(Core::default(), tried.clone()));
    for k in 0..n {
        ok(&mut m, "o1", POST, &["o1", "w2", &format!("z{k}"), "for someone else", "Normal"]);
    }
    ok(&mut m, "x1", REACH, &["x1", "turn_begins"]);
    ok(&mut m, "x1", REACH, &["x1", "permission_asked"]);
    ok(&mut m, "o1", POST, &["o1", "x1", "q1", "stop", "Interrupt"]);
    event(&mut m, "std.hooks.Session-Started", &["g1", "agy", "agy-1.5.18"]);
    ok(&mut m, "o1", POST, &["o1", "g1", "k1", "read the spec", "Normal"]);
    taken(&c);
    tried.borrow_mut().clear();
    ok(&mut m, "x1", REACH, &["x1", "tool_result"]);
    assert_eq!(taken(&c).0, one("Codex-Steer", "x1"), "the held word steered in");
    let steer = std::mem::take(&mut *tried.borrow_mut());
    ok(&mut m, "g1", "std.bridges.agy.Pull-Brain", &["g1", "first"]);
    assert!(holds(&mut m, "std.delivery.In-Flight", &["g1", "k1"]));
    let pull = tried.take();
    (steer, pull)
}

/// consult-lead-c N-1978 p8, p10: what waits for an agent is joined from its own unread messages, so a Codex
/// `tool_result` and an agy pull try as many bindings at 10,000 messages to others as at 10: no walk over every
/// message ever posted.
#[test]
fn a_steer_and_a_pull_try_as_many_bindings_at_ten_thousand_messages_to_others_as_at_ten() {
    let (small, large) = (tried_with(10), tried_with(10_000));
    let sum = |es: &[(String, u64)]| es.iter().map(|(_, n)| n).sum::<u64>();
    // the conditions whose cost grew, each with what it tried at 10 and at 10,000
    let grew = |a: &[(String, u64)], b: &[(String, u64)]| -> Vec<(String, u64, u64)> { a.iter().zip(b).filter(|(x, y)| y.1 > x.1 + 2).map(|(x, y)| (x.0.clone(), x.1, y.1)).collect() };
    for (what, a, b) in [("tool_result", &small.0, &large.0), ("pull", &small.1, &large.1)] {
        assert_eq!(a.len(), b.len(), "{what}: as many eaches at both sizes");
        assert!(sum(b) <= sum(a) + 2, "{what}: tried {} at 10, {} at 10,000; grew: {:?}", sum(a), sum(b), grew(a, b));
    }
}
