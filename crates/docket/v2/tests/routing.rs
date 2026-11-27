//! std.routing and std.harness_bump on the normative core: a fork keeps its parent's warm account while that is above
//! its floor, a fresh agent takes the most quota that would go unused spread by load, a native-only model never takes
//! a relay, and a pin moves only by a bumper, on a fresh probe of that launcher, and never back without the owner.

use docket_v2::check::load;
use docket_v2::norm::Core;
use docket_v2::run::norms::{Norms, Why};
use docket_v2::run::value::{Instance, Value};
use docket_v2::syntax::read;
use std::collections::BTreeSet;

/// The accounts the tests place on, ranked h28, seat-b, seat-g; tom places and owns, ann and bob bump.
const WORLD: &str = r#"
module t
import std.routing as R
import std.harness_bump as P
import std.comms as C
+C.Role("tom", "owner"). +P.Bump-Owed-To("owner"). +R.Placer("tom").
+C.Role("ann", "builder"). +C.Role("bob", "builder"). +P.Bumps("builder").
+R.Account-Rank("h28", 1). +R.Account-Rank("seat-b", 2). +R.Account-Rank("seat-g", 3).
+R.Of-Provider("h28", "anthropic"). +R.Cache-Mode("h28", "1h").
+R.Of-Provider("seat-b", "anthropic"). +R.Cache-Mode("seat-b", "5m").
+R.Of-Provider("seat-g", "google"). +R.Cache-Mode("seat-g", "explicit").
+R.Offers("h28", "opus"). +R.Offers("seat-b", "opus"). +R.Offers("seat-g", "neon").
+R.Native-Only("neon").
"#;

fn std_text(name: &str) -> String {
    let p = format!("{}/../../../docket/std/{name}.docket", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

/// The two modules with what they import and the world, checked whole (14) and loaded into the core.
fn core() -> Core {
    let texts = ["hooks", "comms", "budgets", "harness-bump", "routing"].map(std_text);
    let mut ms: Vec<_> = texts.iter().map(|t| read(t).unwrap_or_else(|e| panic!("{e}"))).collect();
    ms.push(read(WORLD).unwrap());
    let l = load(ms).unwrap_or_else(|e| panic!("{e}"));
    let mut core = Core::default();
    core.load(&l.norms).unwrap();
    core
}

fn t(s: &str) -> Value {
    Value::text(s)
}

fn n(i: i64) -> Value {
    Value::Int(i)
}

fn r(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: format!("std.routing.{ty}"), args }
}

fn p(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: format!("std.harness_bump.{ty}"), args }
}

/// Performs I as BY (None for a driver's line), committing it when it is enabled.
fn act(core: &mut Core, i: Instance, by: Option<&str>) -> Result<(), Why> {
    let s = core.step(&i, by.map(t).as_ref())?;
    core.commit(s);
    Ok(())
}

fn refused() -> Result<(), Why> {
    Err(Why::Policy("not enabled".into()))
}

/// The clock driver's minute, the only time placement reads.
fn tick(core: &mut Core, minute: i64) {
    act(core, Instance { ty: "std.budgets.Tick".into(), args: vec![n(minute)] }, None).unwrap();
}

fn quota(core: &mut Core, account: &str, left: i64, resets_at: i64) {
    act(core, r("Quota-Read", vec![t(account), n(left), n(resets_at)]), None).unwrap();
}

fn fresh_by(core: &mut Core, by: &str, agent: &str, account: &str, version: &str, model: &str) -> Result<(), Why> {
    let harness = if model == "neon" { "agy" } else { "claude-code" };
    let via = if version.starts_with("t3code") { "t3code" } else { harness };
    act(core, r("Place-Fresh", vec![t(by), t(agent), t(account), t(harness), t(via), t(version), t(model)]), Some(by))
}

fn fresh(core: &mut Core, agent: &str, account: &str, version: &str, model: &str) -> Result<(), Why> {
    fresh_by(core, "tom", agent, account, version, model)
}

fn moved(core: &mut Core, by: &str, agent: &str, account: &str) -> Result<(), Why> {
    act(core, r("Move", vec![t(by), t(agent), t(account)]), Some(by))
}

/// The session driver's line: AGENT runs HARNESS at VERSION.
fn started_on(core: &mut Core, agent: &str, harness: &str, version: &str) {
    act(core, Instance { ty: "std.hooks.Session-Started".into(), args: vec![t(agent), t(harness), t(version)] }, None).unwrap();
}

/// AGENT runs Claude Code at the pinned version.
fn started(core: &mut Core, agent: &str) {
    started_on(core, agent, "claude-code", "2.1.332");
}

/// (a) Within the 1h lifetime, by the clock's minute, a fork goes on its parent's account and may not be placed
/// fresh elsewhere, and the parent may not move; no act takes a time, so no actor can say the cache has gone. Once
/// the lifetime has passed both are free.
#[test]
fn a_fork_stays_on_its_parents_warm_account_within_the_ttl() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 990, 10_000);
    quota(&mut core, "seat-b", 30, 1_200);
    fresh(&mut core, "ann", "h28", "2.1.332", "opus").unwrap();
    started(&mut core, "ann");
    tick(&mut core, 2);
    act(&mut core, r("Cache-Used", vec![t("ann"), t("h28")]), None).unwrap();
    assert!(core.holds(&r("Warm", vec![t("ann"), t("h28"), n(62)])), "1h is 60 minutes after the last use");
    act(&mut core, r("Forked", vec![t("bob"), t("ann")]), None).unwrap();
    quota(&mut core, "seat-b", 1000, 200);

    tick(&mut core, 61);
    assert_eq!(fresh(&mut core, "bob", "seat-b", "2.1.332", "opus"), refused(), "a warm parent's fork is no fresh agent");
    assert_eq!(act(&mut core, r("Place-Fork", vec![t("tom"), t("bob"), t("seat-b")]), Some("tom")), refused());
    assert_eq!(moved(&mut core, "tom", "ann", "seat-b"), refused(), "never cut a warm cache");
    act(&mut core, r("Place-Fork", vec![t("tom"), t("bob"), t("h28")]), Some("tom")).unwrap();
    assert!(core.holds(&r("On-Account", vec![t("bob"), t("h28")])));
    assert!(core.holds(&r("Warm", vec![t("bob"), t("h28"), n(62)])), "the fork shares the parent's prefix and its lifetime");

    tick(&mut core, 62);
    act(&mut core, r("Forked", vec![t("cy"), t("ann")]), None).unwrap();
    assert_eq!(act(&mut core, r("Place-Fork", vec![t("tom"), t("cy"), t("h28")]), Some("tom")), refused(), "cold at its end");
    fresh(&mut core, "cy", "seat-b", "2.1.332", "opus").unwrap();
    moved(&mut core, "tom", "ann", "seat-b").unwrap();
}

/// (b) Of two fitting accounts the one with more quota above the floor left per minute to its reset is taken: h28 at
/// 99% for 150 h over a seat at 8% for 20 h; a seat at 40% for 10 h beats h28 in turn (h28 now holds ann), so the
/// rate decides and not the headroom.
#[test]
fn a_fresh_spawn_takes_the_most_quota_that_would_go_unused() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 990, 150 * 60);
    quota(&mut core, "seat-b", 80, 20 * 60);
    assert_eq!(fresh(&mut core, "ann", "seat-b", "2.1.332", "opus"), refused());
    fresh(&mut core, "ann", "h28", "2.1.332", "opus").unwrap();

    quota(&mut core, "seat-b", 400, 10 * 60);
    assert_eq!(fresh(&mut core, "bob", "h28", "2.1.332", "opus"), refused());
    fresh(&mut core, "bob", "seat-b", "2.1.332", "opus").unwrap();
    tick(&mut core, 10 * 60);
    assert_eq!(fresh(&mut core, "cy", "seat-b", "2.1.332", "opus"), refused(), "a reading past its reset is no rate");
}

/// An account at 0.1% a minute before its reset is below the floor and takes nobody, though its rate is the highest;
/// a reading whose reset lies past the horizon is refused, so no score can overflow.
#[test]
fn a_dying_account_takes_no_new_agent() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 1000, 7 * 24 * 60);
    quota(&mut core, "seat-b", 1, 1);
    assert_eq!(fresh(&mut core, "ann", "seat-b", "2.1.332", "opus"), refused());
    fresh(&mut core, "ann", "h28", "2.1.332", "opus").unwrap();
    assert_eq!(act(&mut core, r("Quota-Read", vec![t("seat-b"), n(900), n(i64::MAX)]), None), refused());
    assert_eq!(act(&mut core, r("Quota-Read", vec![t("seat-b"), n(900), n(i64::MIN)]), None), refused());
}

/// Two accounts alike: the first spawn goes to the lower rank, and only there; the next, with one agent already on it,
/// goes to the other, so spawns at one moment do not pile onto one account.
#[test]
fn equal_accounts_break_by_rank_and_spread_by_load() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 600, 600);
    quota(&mut core, "seat-b", 600, 600);
    assert_eq!(fresh(&mut core, "ann", "seat-b", "2.1.332", "opus"), refused(), "one Best, by rank");
    fresh(&mut core, "ann", "h28", "2.1.332", "opus").unwrap();
    assert_eq!(fresh(&mut core, "bob", "h28", "2.1.332", "opus"), refused(), "h28 carries ann");
    fresh(&mut core, "bob", "seat-b", "2.1.332", "opus").unwrap();
}

/// A warm parent on an account fallen below its floor holds no fork there: the fork is placed fresh on a healthy one.
/// A kept cache on such an account may be left, and its warmth there goes with the move.
#[test]
fn an_exhausted_warm_account_lets_go_of_its_agents_and_forks() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 990, 10_000);
    quota(&mut core, "seat-b", 300, 10_000);
    fresh(&mut core, "ann", "h28", "2.1.332", "opus").unwrap();
    started(&mut core, "ann");
    act(&mut core, r("Cache-Used", vec![t("ann"), t("h28")]), None).unwrap();
    act(&mut core, r("Forked", vec![t("bob"), t("ann")]), None).unwrap();
    quota(&mut core, "h28", 20, 10_000);
    assert_eq!(act(&mut core, r("Place-Fork", vec![t("tom"), t("bob"), t("h28")]), Some("tom")), refused(), "below its floor");
    fresh(&mut core, "bob", "seat-b", "2.1.332", "opus").unwrap();
    moved(&mut core, "tom", "ann", "seat-b").unwrap();
    assert!(!core.holds(&r("Warm", vec![t("ann"), t("h28"), n(60)])), "its old warmth is dropped with the move");
    act(&mut core, r("Session-Ended", vec![t("ann")]), None).unwrap();
    assert!(!core.holds(&r("Fork-Of", vec![t("bob"), t("ann")])), "an ended parent is nobody's parent");
}

/// The way an agent is placed to run is kept; started relayed after a native placement, its duty is violated.
#[test]
fn an_agent_started_otherwise_than_placed_shows_it() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "seat-g", 500, 600);
    fresh(&mut core, "ann", "seat-g", "agy-1.5.18", "neon").unwrap();
    assert!(core.holds(&r("Placed-With", vec![t("ann"), t("agy"), t("agy"), t("agy-1.5.18")])));
    let duty = r("Runs-As-Placed", vec![t("ann"), t("tom")]);
    started_on(&mut core, "ann", "agy", "agy-1.5.18");
    assert!(core.holds(&duty) && !core.holds(&r("Off-Placement", vec![t("ann")])));
    started_on(&mut core, "ann", "agy", "t3code-0.1.11");
    assert!(core.holds(&duty) && core.holds(&r("Off-Placement", vec![t("ann")])), "native placed, relayed started: the duty's Violated when");
}

/// (c) A native-only model is refused on agy through t3code, which relays agy, and taken on agy itself; no harness
/// runs at a version that is not pinned.
#[test]
fn a_native_only_model_cannot_route_through_a_relay() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "seat-g", 500, 60);
    assert!(core.holds(&p("Relayed", vec![t("agy"), t("t3code"), t("t3code-0.1.11")])));
    assert_eq!(fresh(&mut core, "ann", "seat-g", "t3code-0.1.11", "neon"), refused());
    assert_eq!(fresh(&mut core, "ann", "seat-g", "agy-1.6.0", "neon"), refused(), "an unpinned version");
    fresh(&mut core, "ann", "seat-g", "agy-1.5.18", "neon").unwrap();
}

/// Only a party the project names as a placer places or moves an agent; one agent cannot move another.
#[test]
fn only_a_placer_places_or_moves() {
    let mut core = core();
    tick(&mut core, 0);
    quota(&mut core, "h28", 990, 9_000);
    quota(&mut core, "seat-b", 1000, 100);
    assert_eq!(fresh_by(&mut core, "eve", "ann", "seat-b", "2.1.332", "opus"), refused());
    fresh(&mut core, "ann", "seat-b", "2.1.332", "opus").unwrap();
    started(&mut core, "ann");
    quota(&mut core, "h28", 1000, 50);
    assert_eq!(moved(&mut core, "eve", "ann", "h28"), refused());
    moved(&mut core, "tom", "ann", "h28").unwrap();
}

/// (d) A bump is refused while Verify-Intent holds: before staging, with a check missing, and with one failed (the
/// duty violated); a new staging starts the checks over, and with every check passed the probe's lines close the
/// duty and the pin moves. Only a bumper proposes or commits.
#[test]
fn an_unverified_harness_bump_is_refused() {
    let mut core = core();
    let (h, via) = (t("claude-code"), t("claude-code"));
    let commit = |core: &mut Core, v: &str| act(core, p("Commit-Pin", vec![t("ann"), h.clone(), via.clone(), t(v)]), Some("ann"));
    let line = |core: &mut Core, ev: &str, v: &str, c: &str| act(core, p(ev, vec![h.clone(), via.clone(), t(v), t(c)]), None).unwrap();
    let staged = |core: &mut Core, v: &str| act(core, p("Candidate-Staged", vec![h.clone(), via.clone(), t(v)]), None).unwrap();
    let duty = |v: &str| p("Verify-Intent", vec![t("ann"), t("tom"), h.clone(), via.clone(), t(v)]);

    assert_eq!(act(&mut core, p("Propose-Bump", vec![t("eve"), h.clone(), via.clone(), t("2.1.400")]), Some("eve")), refused(), "eve bumps nothing");
    assert_eq!(commit(&mut core, "2.1.400"), refused(), "nothing asked");
    act(&mut core, p("Propose-Bump", vec![t("ann"), h.clone(), via.clone(), t("2.1.400")]), Some("ann")).unwrap();
    assert!(core.holds(&duty("2.1.400")));
    assert_eq!(commit(&mut core, "2.1.400"), refused(), "not staged");
    staged(&mut core, "2.1.400");
    for c in ["hook_moments", "system_prompt", "model_flags"] {
        line(&mut core, "Check-Passed", "2.1.400", c);
    }
    assert_eq!(commit(&mut core, "2.1.400"), refused(), "trust_popup not probed");
    line(&mut core, "Check-Failed", "2.1.400", "trust_popup");
    let failed = p("Failed", vec![h.clone(), via.clone(), t("2.1.400"), t("trust_popup")]);
    assert!(core.holds(&duty("2.1.400")) && core.holds(&failed), "a failed check leaves the duty open, and violated");
    assert_eq!(commit(&mut core, "2.1.400"), refused(), "a failed check");

    staged(&mut core, "2.1.400");
    assert!(!core.holds(&failed), "a new staging clears the flake");
    for c in ["hook_moments", "system_prompt", "trust_popup", "model_flags"] {
        line(&mut core, "Check-Passed", "2.1.400", c);
    }
    assert!(!core.holds(&duty("2.1.400")), "the probe's lines closed it");
    commit(&mut core, "2.1.400").unwrap();
    assert!(core.holds(&p("Pinned", vec![h.clone(), via.clone(), t("2.1.400")])));
    assert!(!core.holds(&p("Pinned", vec![h.clone(), via.clone(), t("2.1.332")])));
}

/// The pin never goes back to a version it has left on an old probe: a builder may not propose it, the owner may, and
/// his proposal is probed afresh. A probe of one launcher is no probe of another of the same harness.
#[test]
fn a_bump_never_replays_an_old_probe() {
    let mut core = core();
    let probe = |core: &mut Core, harness: &str, via: &str, v: &str| {
        act(core, p("Candidate-Staged", vec![t(harness), t(via), t(v)]), None).unwrap();
        for c in ["hook_moments", "system_prompt", "trust_popup", "model_flags"] {
            act(core, p("Check-Passed", vec![t(harness), t(via), t(v), t(c)]), None).unwrap();
        }
    };
    let propose = |core: &mut Core, by: &str, harness: &str, via: &str, v: &str| act(core, p("Propose-Bump", vec![t(by), t(harness), t(via), t(v)]), Some(by));
    let commit = |core: &mut Core, by: &str, harness: &str, via: &str, v: &str| act(core, p("Commit-Pin", vec![t(by), t(harness), t(via), t(v)]), Some(by));

    propose(&mut core, "ann", "claude-code", "claude-code", "2.1.400").unwrap();
    probe(&mut core, "claude-code", "claude-code", "2.1.400");
    commit(&mut core, "ann", "claude-code", "claude-code", "2.1.400").unwrap();
    assert_eq!(propose(&mut core, "bob", "claude-code", "claude-code", "2.1.332"), refused(), "back to a left pin");
    propose(&mut core, "tom", "claude-code", "claude-code", "2.1.332").unwrap();
    assert_eq!(commit(&mut core, "tom", "claude-code", "claude-code", "2.1.332"), refused(), "his rollback is probed too");

    propose(&mut core, "ann", "claude-code", "claude-code", "2.1.401").unwrap();
    probe(&mut core, "claude-code", "claude-code", "2.1.401");
    propose(&mut core, "bob", "claude-code", "claude-code", "2.1.401").unwrap();
    assert_eq!(commit(&mut core, "bob", "claude-code", "claude-code", "2.1.401"), refused(), "a proposal starts the probe over");

    propose(&mut core, "ann", "agy", "t3code", "t3code-0.2.0").unwrap();
    probe(&mut core, "agy", "agy", "t3code-0.2.0");
    assert_eq!(commit(&mut core, "ann", "agy", "t3code", "t3code-0.2.0"), refused(), "the native launcher's probe");
    probe(&mut core, "agy", "t3code", "t3code-0.2.0");
    commit(&mut core, "ann", "agy", "t3code", "t3code-0.2.0").unwrap();
}

/// The pairs of fields AT a text states for the facts NAMES, from its own `+` lines.
fn stated(text: &str, names: &[&str], at: [usize; 2]) -> BTreeSet<(String, String)> {
    use docket_v2::syntax::tree::{DeclKind, E, Expr};
    let text_at = |xs: &[Expr], i: usize| match docket_v2::check::names::literal(&xs[i]) {
        Some(Value::Text(s)) => s,
        other => panic!("{other:?}"),
    };
    let m = read(text).unwrap();
    m.decls.iter().filter_map(|d| match &d.what {
        DeclKind::Holds(Expr { e: E::Apply(f, xs), .. }) if names.contains(&f.to_string().as_str()) => Some((text_at(xs, at[0]), text_at(xs, at[1]))),
        _ => None,
    }).collect()
}

/// The pins are the versions the bridges were read at, each one, and no other: a bump that moves one copy fails here.
#[test]
fn every_pin_is_a_version_a_bridge_states() {
    let pins = stated(&std_text("harness-bump"), &["Pin-Stated"], [0, 2]);
    let mut read_at = BTreeSet::new();
    for b in ["claude-code", "codex", "opencode", "agy"] {
        read_at.extend(stated(&std_text(&format!("bridges/{b}")), &["H.Reaches", "H.Reaches-In", "H.Lets", "H.Waits"], [0, 1]));
    }
    assert_eq!(pins.len(), 5, "claude-code, codex, opencode, and agy both ways");
    assert_eq!(pins, read_at);
}
