//! std.attention on the machine and the eFLINT core: the owner's pause, the freeze, neglect, and soft signals that refuse nothing.

use docket_v2::norm::Core;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::machine::{Machine, State};
use docket_v2::run::norms::Norms;
use docket_v2::run::record::Line;
use docket_v2::run::value::{Instance, Value};

fn t(s: &str) -> Value {
    Value::text(s)
}

fn n(k: i64) -> Value {
    Value::Int(k)
}

fn i(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: if ty.contains('.') { ty.to_string() } else { format!("std.attention.{ty}") }, args }
}

/// The installation's driver: seats the owner, ticks the clock, and names the imported texts' Effects so their reactions load.
struct Host;

impl Driver for Host {
    fn name(&self) -> Vec<Value> {
        vec![t("host"), t("1")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let names = ["std.hooks.Start-Session", "std.hooks.Tool-Use", "std.hooks.Next-Moment", "std.hooks.Apply", "std.comms.Call-Text", "std.comms.Model", "std.comms.Read-As", "std.comms.Pick", "std.comms.TextOf", "std.budgets.Request-Budget", "std.budgets.Command", "std.budgets.Measure", "std.control.Await-Approval"];
        names.iter().map(|e| (e.to_string(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        vec!["std.attention.Tick".into(), "std.comms.Owner-Seated".into()]
    }
    fn carry(&mut self, op: &str, args: &[Value], _: u64) -> Reply {
        assert_eq!(op, "std.control.Await-Approval", "{op} was not expected");
        if args.get(1) == Some(&t("fail")) { Reply::Failed("the window went away".into()) } else { Reply::Later }
    }
    fn dropped(&mut self, _: u64) {}
}

struct World(Machine<Core>);

impl World {
    /// Olga the owner, leo the lead, bea the brain, at minute 0.
    fn new() -> World {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docket/std");
        let read = |f: &str| docket_v2::syntax::read(&std::fs::read_to_string(dir.join(f)).unwrap()).unwrap_or_else(|e| panic!("{f}: {e}"));
        let mut texts = ["attention.docket", "control.docket", "budgets.docket", "comms.docket", "hooks.docket"].map(read).to_vec();
        // a program of a project that asks a party's approval
        let ask = "module t\nimport std.hooks as H\nimport std.control as Ctrl\nAct Ask(by: H.Party, to: H.Party, what: String) Returns Ctrl.Approval Does { perform Ctrl.Await-Approval(to, what) }";
        texts.push(docket_v2::syntax::read(ask).unwrap());
        let mut l = docket_v2::check::names::load(texts).unwrap_or_else(|e| panic!("{e}"));
        l.rows = docket_v2::check::typing::check(&l).unwrap_or_else(|es| panic!("{}", es.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")));
        let mut w = World(Machine::new(l, Core::checked_guided(), vec![Box::new(Host)]).unwrap_or_else(|e| panic!("{e}")));
        w.event(i("std.comms.Owner-Seated", vec![t("olga")]));
        w.ok("olga", i("std.comms.Seat", vec![t("olga"), t("leo"), t("lead")]));
        w.ok("olga", i("std.comms.Seat", vec![t("olga"), t("bea"), t("brain")]));
        w.tick(0);
        w
    }
    fn event(&mut self, e: Instance) {
        self.0.event(0, e);
        self.0.run();
    }
    fn tick(&mut self, at: i64) {
        self.event(i("Tick", vec![n(at)]));
    }
    /// BY's run of act A, to its end: whether it was performed.
    fn act(&mut self, by: &str, a: Instance) -> bool {
        let r = self.0.start(t(by), Some(0), "", a).unwrap_or_else(|e| panic!("{e}"));
        self.0.run();
        self.0.state(r) == State::Done
    }
    fn ok(&mut self, by: &str, a: Instance) {
        let what = format!("{a}");
        assert!(self.act(by, a), "{what} by {by} was refused: {:?}", self.0.record.last());
    }
    fn refused(&mut self, by: &str, a: Instance) {
        let what = format!("{a}");
        assert!(!self.act(by, a), "{what} by {by} was let");
    }
    fn holds(&mut self, ty: &str, args: Vec<Value>) -> bool {
        self.0.norms.holds(&i(ty, args))
    }
    /// The module's own reactions were never refused.
    fn reactions_clean(&self) {
        let bad: Vec<&Line> = self.0.record.iter().filter(|l| matches!(l, Line::Refused { act, .. } if act.ty.starts_with("std.attention.Raise") || act.ty.starts_with("std.attention.Lower"))).collect();
        assert!(bad.is_empty(), "{bad:?}");
    }
    fn initiative(&mut self, id: &str, stale: i64) {
        self.ok("leo", i("Open-Initiative", vec![t("leo"), t(id), t("a title"), n(stale)]));
    }
    fn item(&mut self, id: &str, init: &str, stale: i64) {
        self.ok("leo", i("Open-Item", vec![t("leo"), t(id), t(init), t("a task"), n(stale)]));
    }
}

#[test]
fn a_non_owner_cannot_pause_drop_or_resume() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.refused("leo", i("Owner-Pause", vec![t("leo"), t("i1"), t("too much")]));
    w.refused("leo", i("Owner-Drop", vec![t("leo"), t("i1"), t("not needed")]));
    w.refused("bea", i("Owner-Pause", vec![t("bea"), t("i1"), t("too much")]));
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    assert!(w.holds("Initiative", vec![t("i1"), t("a title"), t("Parked")]));
    w.refused("leo", i("Owner-Resume", vec![t("leo"), t("i1")]));
    w.ok("olga", i("Owner-Resume", vec![t("olga"), t("i1")]));
    w.ok("olga", i("Owner-Drop", vec![t("olga"), t("i1"), t("done with it")]));
    assert!(w.holds("Initiative", vec![t("i1"), t("a title"), t("Done")]));
    w.reactions_clean();
}

#[test]
fn pausing_freezes_work_and_spend_under_the_initiative() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.item("w1", "i1", 600);
    w.ok("ann", i("Spend-For", vec![t("ann"), t("b1"), t("w1")]));
    w.ok("ann", i("Push", vec![t("ann"), t("w1")]));
    w.ok("ann", i("std.budgets.Spend", vec![t("ann"), t("b1"), t("tokens"), n(5)]));
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    assert!(w.holds("Frozen", vec![t("w1")]));
    w.refused("ann", i("Push", vec![t("ann"), t("w1")]));
    w.refused("ann", i("std.budgets.Spend", vec![t("ann"), t("b1"), t("tokens"), n(5)]));
    w.refused("leo", i("Open-Item", vec![t("leo"), t("w2"), t("i1"), t("more"), n(60)]));
    w.refused("ann", i("Open-Discussion", vec![t("ann"), t("d1"), t("w1")]));
    w.refused("ann", i("Finish-Item", vec![t("ann"), t("w1")]));
    w.ok("olga", i("Owner-Resume", vec![t("olga"), t("i1")]));
    w.ok("ann", i("Push", vec![t("ann"), t("w1")]));
    w.ok("ann", i("std.budgets.Spend", vec![t("ann"), t("b1"), t("tokens"), n(5)]));
    w.reactions_clean();
}

#[test]
fn a_pause_freezes_only_the_spend_for_what_is_under_it() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.initiative("i2", 600);
    w.item("w1", "i1", 600);
    w.item("w2", "i2", 600);
    w.ok("ann", i("Spend-For", vec![t("ann"), t("b1"), t("w1")]));
    w.ok("ann", i("Spend-For", vec![t("ann"), t("b2"), t("w2")]));
    w.refused("ann", i("Spend-For", vec![t("ann"), t("b1"), t("w2")]));
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    w.refused("ann", i("std.budgets.Spend", vec![t("ann"), t("b1"), t("tokens"), n(5)]));
    w.ok("ann", i("std.budgets.Spend", vec![t("ann"), t("b2"), t("tokens"), n(5)]));
    w.ok("ann", i("Push", vec![t("ann"), t("w2")]));
    w.refused("ann", i("Spend-For", vec![t("ann"), t("b3"), t("w1")]));
}

#[test]
fn the_name_the_reactions_run_as_is_seated_by_nobody() {
    let mut w = World::new();
    w.refused("olga", i("std.comms.Seat", vec![t("olga"), t("std.attention"), t("lead")]));
    w.event(i("std.comms.Owner-Seated", vec![t("std.attention")]));
    assert!(!w.holds("std.comms.Role", vec![t("std.attention"), t("owner")]));
}

#[test]
fn an_unpaused_stale_item_raises_surface_neglected_until_pushed_or_paused() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.item("w1", "i1", 30);
    w.tick(30);
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    w.tick(31);
    assert!(w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    assert!(w.holds("Needs-Owner", vec![t("neglected"), t("w1")]));
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("i1")]));
    w.ok("ann", i("Push", vec![t("ann"), t("w1")]));
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    w.tick(700);
    assert!(w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("i1")]));
    assert!(w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("i1")]));
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    w.reactions_clean();
}

#[test]
fn an_eighth_active_item_raises_a_soft_anomaly_and_trim_scope_without_refusing_it() {
    let mut w = World::new();
    w.initiative("i1", 600);
    for k in 1..=7 {
        w.item(&format!("w{k}"), "i1", 600);
    }
    assert!(w.holds("System-Pulse", vec![t("Healthy"), n(0)]));
    assert!(!w.holds("System-Pulse", vec![t("Anomalous"), n(0)]), "the pulse is one state, the one its count gives");
    w.item("w8", "i1", 600);
    assert!(w.holds("Pulse-Anomaly", vec![t("wip_bloat"), t("i1")]));
    assert!(w.holds("Trim-Scope", vec![t("leo"), t("olga"), t("i1")]));
    assert!(w.holds("System-Pulse", vec![t("Anomalous"), n(1)]));
    assert!(!w.holds("System-Pulse", vec![t("Healthy"), n(1)]) && !w.holds("System-Pulse", vec![t("Healthy"), n(0)]));
    w.ok("leo", i("Finish-Item", vec![t("leo"), t("w8")]));
    assert!(!w.holds("Trim-Scope", vec![t("leo"), t("olga"), t("i1")]), "the module lowers it, not the lead's own act");
    assert!(w.holds("System-Pulse", vec![t("Healthy"), n(0)]));
    assert!(!w.holds("System-Pulse", vec![t("Anomalous"), n(0)]), "the pulse is one state, the one its count gives");
    for k in 1..=3 {
        w.ok("ann", i("Open-Discussion", vec![t("ann"), t(&format!("d{k}")), t("w1")]));
    }
    assert!(w.holds("Resolve-Or-Escalate", vec![t("bea"), t("olga"), t("w1")]));
    w.ok("ann", i("Close-Discussion", vec![t("ann"), t("d3")]));
    assert!(!w.holds("Resolve-Or-Escalate", vec![t("bea"), t("olga"), t("w1")]));
    w.tick(121);
    assert!(w.holds("Pulse-Anomaly", vec![t("spinning_discussion"), t("w1")]), "open past 120 minutes");
    assert!(w.holds("Resolve-Or-Escalate", vec![t("bea"), t("olga"), t("w1")]));
    w.reactions_clean();
}

#[test]
fn an_untied_thread_opens_and_is_owed_filing_under_an_initiative() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.ok("ann", i("Open-Thread", vec![t("ann"), t("t1")]));
    assert!(w.holds("Unfiled", vec![t("t1")]));
    assert!(w.holds("File-Under-Initiative", vec![t("leo"), t("olga"), t("t1")]));
    assert!(!w.holds("Needs-Owner", vec![t("unfiled_overdue"), t("t1")]), "off the owner's head while it is fresh");
    w.tick(16);
    assert!(w.holds("Needs-Owner", vec![t("unfiled_overdue"), t("t1")]));
    w.ok("leo", i("File-Under", vec![t("leo"), t("t1"), t("i1")]));
    assert!(!w.holds("File-Under-Initiative", vec![t("leo"), t("olga"), t("t1")]));
    assert!(!w.holds("Needs-Owner", vec![t("unfiled_overdue"), t("t1")]));
    w.ok("olga", i("Open-Thread", vec![t("olga"), t("t2")]));
    assert!(!w.holds("Unfiled", vec![t("t2")]), "the owner's own thread is no agent's to file");
    w.reactions_clean();
}

/// The approval in Needs-Owner is the Await-Approval in flight: there while the owner's answer is awaited, gone once it comes.
#[test]
fn needs_owner_lists_an_approval_while_its_await_is_pending() {
    let mut w = World::new();
    w.refused("ann", i("Asked-Owner", vec![t("r0"), t("olga"), t("forged")]));
    let r = w.0.start(t("ann"), Some(0), "", i("t.Ask", vec![t("ann"), t("olga"), t("may I merge w9")])).unwrap();
    w.0.run();
    let State::Resting(line) = w.0.state(r) else { panic!("the ask waits for the owner: {:?}", w.0.record) };
    let run = Value::text(w.0.run_name(r));
    assert!(w.0.norms.holds(&i("Awaiting-Approval", vec![run.clone(), t("olga"), t("may I merge w9")])));
    assert!(w.0.norms.holds(&i("Needs-Owner", vec![t("approval"), run.clone()])));
    w.0.answer(line, Reply::Answer { value: Value::ctor("std.control.Lapsed", vec![]), costs: vec![], events: vec![] }).unwrap();
    w.0.run();
    assert_eq!(w.0.state(r), State::Done, "{:?}", w.0.record);
    assert!(!w.0.norms.holds(&i("Needs-Owner", vec![t("approval"), run])));
    let lines = w.0.record.iter().filter(|l| matches!(l, Line::Effect { op, .. } if op == "std.control.Await-Approval")).count();
    assert_eq!(lines, 1, "the root arm carries the one Effect on, it does not ask twice");
    let other = w.0.start(t("ann"), Some(0), "", i("t.Ask", vec![t("ann"), t("leo"), t("for leo")])).unwrap();
    w.0.run();
    assert!(!w.0.norms.holds(&i("Needs-Owner", vec![t("approval"), Value::text(w.0.run_name(other))])), "an ask of another party is not the owner's");
}

#[test]
fn an_approval_whose_answer_fails_leaves_needs_owner() {
    let mut w = World::new();
    let r = w.0.start(t("ann"), Some(0), "", i("t.Ask", vec![t("ann"), t("olga"), t("fail")])).unwrap();
    w.0.run();
    assert_eq!(w.0.state(r), State::Raised, "{:?}", w.0.record);
    let run = Value::text(w.0.run_name(r));
    assert!(!w.0.norms.holds(&i("Awaiting-Approval", vec![run.clone(), t("olga"), t("fail")])));
    assert!(!w.0.norms.holds(&i("Needs-Owner", vec![t("approval"), run])));
    assert!(w.0.record.iter().any(|l| matches!(l, Line::Performed { act, .. } if act.ty == "std.attention.Answered-Owner")));
}

/// The owner's audit (N-1895 p152, bug 1): a discussion left open under a paused or dropped initiative, or on a finished
/// item, is no spinning discussion; nothing is owed for it.
#[test]
fn an_open_discussion_on_frozen_or_finished_work_does_not_spin() {
    let mut w = World::new();
    for (init, item) in [("i1", "w1"), ("i2", "w2"), ("i3", "w3")] {
        w.initiative(init, 6000);
        w.item(item, init, 6000);
    }
    for k in 1..=3 {
        w.ok("ann", i("Open-Discussion", vec![t("ann"), t(&format!("d{k}")), t("w1")]));
    }
    w.ok("ann", i("Open-Discussion", vec![t("ann"), t("e1"), t("w2")]));
    w.ok("ann", i("Open-Discussion", vec![t("ann"), t("f1"), t("w3")]));
    assert!(w.holds("Resolve-Or-Escalate", vec![t("bea"), t("olga"), t("w1")]), "three on w1");
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    w.ok("olga", i("Owner-Drop", vec![t("olga"), t("i2"), t("not needed")]));
    w.ok("leo", i("Finish-Item", vec![t("leo"), t("w3")]));
    w.tick(121);
    for item in ["w1", "w2", "w3"] {
        assert!(!w.holds("Pulse-Anomaly", vec![t("spinning_discussion"), t(item)]), "{item}");
        assert!(!w.holds("Resolve-Or-Escalate", vec![t("bea"), t("olga"), t(item)]), "{item}");
    }
    assert!(w.holds("System-Pulse", vec![t("Healthy"), n(0)]));
    w.reactions_clean();
}

/// The owner's audit (N-1895 p152, bug 2): resuming an initiative restarts its active items' clocks too, so none is
/// neglected the moment it resumes; one finished before the pause keeps its own.
#[test]
fn resuming_an_initiative_restarts_its_items_clocks() {
    let mut w = World::new();
    w.initiative("i1", 600);
    w.item("w1", "i1", 30);
    w.item("w2", "i1", 30);
    w.ok("leo", i("Finish-Item", vec![t("leo"), t("w2")]));
    w.ok("olga", i("Owner-Pause", vec![t("olga"), t("i1"), t("later")]));
    w.tick(500);
    w.ok("olga", i("Owner-Resume", vec![t("olga"), t("i1")]));
    assert!(!w.holds("Neglected", vec![t("w1")]), "w1 neglected as it resumed");
    assert!(!w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]));
    assert!(w.holds("Progress", vec![t("w1"), n(500)]) && !w.holds("Progress", vec![t("w1"), n(0)]), "one clock on w1, from the resume");
    assert!(w.holds("Progress", vec![t("w2"), n(0)]), "the finished item keeps its own");
    w.tick(531);
    assert!(w.holds("Surface-Neglected", vec![t("std.attention"), t("olga"), t("w1")]), "and it runs again from there");
    w.reactions_clean();
}
