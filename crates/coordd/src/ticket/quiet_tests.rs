//! `waited` is how long an owed step has been silent, not its age (quiet.rs; N-159 posts 56, 59): a turn running for
//! its agent or a runner job running for its ticket holds it, however long; a quiet step still expires.

use super::{expired, quiet::Quiet, Core};
use chrono::{DateTime, Duration, Utc};
use docket_machine::{Input, Line};
use serde_json::json;

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn job(at: &str, name: &str, id: &str, on: u64) -> Line {
    Line::Event { at: at.into(), name: name.into(), value: json!({"event": name, "id": id, "ticket": format!("H-{on}")}), fired: vec![], counted: vec![], on: Some(on) }
}

/// The policy's `expired` asked of one step that has been silent MINUTES, the base policy (120).
fn expires(agent: &str, minutes: i64) -> bool {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let places = crate::policy::Places { base, coord: std::env::temp_dir().display().to_string(), ..Default::default() };
    !expired(&places, vec![], &[(1, 1, agent.to_string(), "H-1".to_string(), minutes)]).unwrap().is_empty()
}

#[test]
fn a_turn_running_past_silent_after_holds_the_step_and_its_end_starts_the_silence() {
    let mut q = Quiet::default();
    let since = "2026-11-15T20:00:00Z";
    // three hours on Agda in one turn: not silent
    assert_eq!(q.waited(1, "f1-2", since, true, t("2026-11-15T23:00:00Z")), 0);
    assert!(!expires("f1-2", 0));
    // the turn ended at 22:30: silent from then, not from the step's 20:00
    q.take(&Line::TurnEnded { at: "2026-11-15T22:30:00Z".into(), agent: "f1-2".into(), delivery: None, text: docket_machine::Loc { file: String::new(), offset: 0, len: 0 }, later: vec![], informs: vec![], stood: None, go: None, tools: Default::default(), model: None });
    assert_eq!(q.waited(1, "f1-2", since, false, t("2026-11-15T23:00:00Z")), 30);
    assert!(!expires("f1-2", 30));
    // and when it stays silent: withdrawn at 120
    assert_eq!(q.waited(1, "f1-2", since, false, t("2026-11-16T00:30:00Z")), 120);
    assert!(expires("f1-2", 120));
    // another agent's turn end does not count for it
    q.turn_ended("other", "2026-11-16T00:29:00Z");
    assert_eq!(q.waited(1, "f1-2", since, false, t("2026-11-16T00:30:00Z")), 120);
}

#[test]
fn a_job_running_past_silent_after_holds_the_runner_step_and_its_end_starts_the_silence() {
    let mut q = Quiet::default();
    let since = "2026-11-15T20:00:00Z";
    q.take(&job("2026-11-15T20:01:00Z", "job_start", "77", 1));
    // a check building for two hours and more: not silent; a job on another ticket does not hold this one
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-15T23:00:00Z")), 0);
    assert_eq!(q.waited(2, "runner", since, false, t("2026-11-15T23:00:00Z")), 180);
    // its end: silent from then
    q.take(&job("2026-11-15T22:10:00Z", "job_end", "77", 1));
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-15T23:00:00Z")), 50);
    assert!(!expires("runner", 50));
    assert!(expires("runner", q.waited(1, "runner", since, false, t("2026-11-16T00:10:00Z"))));
}

#[test]
fn a_quiet_step_still_expires() {
    let q = Quiet::default();
    let m = q.waited(1, "f1-2", "2026-11-15T20:00:00Z", false, t("2026-11-15T22:01:00Z"));
    assert_eq!(m, 121);
    assert!(expires("f1-2", m));
    // the owner's steps never do
    assert!(!expires("owner", 600));
}

/// Through coordd: a hand-in's runner step, its check's job started, coordd restarted mid-job; the job is rebuilt from
/// the log and holds the step past 120 minutes; its end lets the silence count, and the policy withdraws the step then.
/// An agent in a turn is read from its live state.
#[test]
fn a_restart_mid_job_rebuilds_the_open_job_from_the_log_and_it_still_holds_the_step() {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-quiet-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/h.call"), "call hand_in(commit: Ref) -> OnLine(top: Ref) | Red(errors: Text)\n  template\n    Check {{ commit }}.\n").unwrap();
    std::fs::write(project.join("kinds/h.kind"), "kind piece\n  letter H\n  form c: Ref\n  result Ref\n  pattern\n    v <- step runner hand_in(c);\n    match v {\n      OnLine(top) => return top\n      Red(e) => return c\n    }\n").unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"h\"\nrunner = \"true\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let load = || {
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core
    };
    let mut core = load();
    core.ticket_input(Input::Open { kind: "piece".into(), under: None, opener: "owner".into(), form: json!({"c": "abc123"}), text: None, names: vec![], told: vec![] }).unwrap();
    let k = core.tickets.m.tickets.values().find(|x| x.kind == "piece").map(|x| x.t).unwrap();
    let id = core.tickets.m.id(k);
    // what project_event's Task::Event becomes off the lock
    let event = |core: &mut Core, name: &str, phase: &str| core.ticket_input(Input::Event { name: name.into(), value: json!({"event": name, "id": "77", "job": format!("hand-{id}/check"), "ticket": id, "phase": phase}) }).unwrap();
    event(&mut core, "job_start", "start");
    let owed = |core: &Core, later: i64| {
        let (t, _, a, since) = core.tickets.m.awaiting().into_iter().find(|(t, ..)| *t == k).expect("the step is owed");
        core.silent_minutes(t, &a, &since, Utc::now() + Duration::minutes(later))
    };
    assert_eq!(owed(&core, 180), 0);
    // coordd restarts while the check builds
    drop(core);
    let mut core = load();
    assert!(core.tickets.quiet.job_running(k), "the open job is rebuilt from the log");
    assert_eq!(owed(&core, 180), 0, "and still holds the step three hours on");
    assert!(!expires("runner", owed(&core, 180)));
    event(&mut core, "job_end", "end");
    assert!(!core.tickets.quiet.job_running(k));
    let m = owed(&core, 180);
    assert!((179..=180).contains(&m), "{m}");
    assert!(expires("runner", m), "silent since the job's end, and that long: withdrawn");
    // an agent in a turn, from its live state; its turn's end is when its silence starts
    let a = core.agents.get_mut("w2").unwrap();
    (a.live.state, a.live.turn, a.ended_turn) = ("working".into(), 1, 0);
    let early = "2026-11-15T00:00:00Z";
    assert_eq!(core.silent_minutes(k, "w2", early, Utc::now()), 0);
    core.end_turn("w2");
    core.agents.get_mut("w2").unwrap().live.state = "idle".into();
    assert!(core.silent_minutes(k, "w2", early, Utc::now() + Duration::minutes(30)) <= 30);
    let _ = std::fs::remove_dir_all(&coord);
}

/// A step given to the runner (`accepted`, waiting for a build cut up to 120 minutes later and running on the build's
/// own ticket) is not silent while the runner is seen alive; from when it is seen gone it is (paper-research N-159
/// post 109). An agent's step is as before.
#[test]
fn a_runner_step_waits_while_the_runner_lives_and_is_silent_once_it_is_gone() {
    let mut q = Quiet::default();
    let since = "2026-11-15T20:00:00Z";
    // nothing heard of the runner: its step goes by the rule for any step
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-15T22:01:00Z")), 121);
    // seen alive (the source's runner_seen at coordd's start, or runner_up): 500 minutes and not silent
    q.runner_seen(true, "2026-11-15T19:00:00Z");
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-16T04:20:00Z")), 0);
    assert!(!expires("runner", q.waited(1, "runner", since, false, t("2026-11-16T04:20:00Z"))));
    // an agent's step is not held by the runner's life
    assert_eq!(q.waited(2, "f1-2", since, false, t("2026-11-15T22:01:00Z")), 121);
    assert!(expires("f1-2", 121));
    // seen gone at 04:00: silent from then, withdrawn once that is longer than silent-after
    q.runner_seen(false, "2026-11-16T04:00:00Z");
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-16T05:00:00Z")), 60);
    assert!(!expires("runner", 60));
    assert!(expires("runner", q.waited(1, "runner", since, false, t("2026-11-16T06:01:00Z"))));
    // back up: waits again
    q.runner_seen(true, "2026-11-16T06:30:00Z");
    assert_eq!(q.waited(1, "runner", since, false, t("2026-11-16T09:00:00Z")), 0);
}

/// Through coordd: the source's events set what the runner is seen as.
#[test]
fn the_sources_runner_events_say_whether_the_runner_lives() {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-quiet-runner-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    std::fs::create_dir_all(&coord).unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.load_tickets();
    let since = "2026-11-15T00:00:00Z";
    let rt = tokio::runtime::Runtime::new().unwrap();
    let later = Utc::now() + Duration::minutes(500);
    assert!(core.silent_minutes(1, "runner", since, later) > 120, "nothing heard: by the step's age");
    rt.block_on(core.project_event(json!({"event": "runner_seen", "up": true})));
    assert_eq!(core.silent_minutes(1, "runner", since, later), 0);
    rt.block_on(core.project_event(json!({"event": "runner_down", "why": "no process holds the runner's lock"})));
    let m = core.silent_minutes(1, "runner", since, later);
    assert!((499..=500).contains(&m), "{m}: silent from when it was seen gone");
    rt.block_on(core.project_event(json!({"event": "runner_up"})));
    assert_eq!(core.silent_minutes(1, "runner", since, later), 0);
    rt.block_on(core.project_event(json!({"event": "runner_seen", "up": false})));
    assert!(core.silent_minutes(1, "runner", since, later) >= 499);
    assert!(core.silent_minutes(1, "f1-2", since, later) > 120, "an agent's step is not held by it");
    let _ = std::fs::remove_dir_all(&coord);
}

/// Time with no coordd on the record is not silence (paper-research N-101 post 186): a step owed by an idle agent, the
/// record set aside longer than `silent-after`, coordd started on it again; the step is not withdrawn at the next check,
/// and is once `silent-after` of running time has passed. The time down is kept over a further restart.
#[test]
fn time_with_no_coordd_on_the_record_is_not_silence() {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-quiet-down-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/a.call"), "call ask(q: Text) -> Said(a: Text)\n  template\n    {{ q }}\n").unwrap();
    std::fs::write(project.join("kinds/a.kind"), "kind e\n  letter E\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    return q\n").unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"d\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let load = || {
        let mut core = Core::load(&coord.display().to_string(), &base);
        core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        core.load_tickets();
        assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
        core
    };
    let t0 = Utc::now();
    let mut core = load();
    core.came_up(t0).unwrap();
    core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": "why?"}), text: None, names: vec![], told: vec![] }).unwrap();
    let owed = |core: &Core, later: i64| {
        let (t, _, a, since) = core.tickets.m.awaiting().into_iter().find(|(_, _, a, _)| a == "w2").expect("the step is owed");
        core.silent_minutes(t, &a, &since, t0 + Duration::minutes(later))
    };
    assert!(owed(&core, 1) <= 1);
    // the last mark of the running coordd, then the record set aside for 200 minutes
    core.tickets.quiet.alive(&core.alive_file(), t0 + Duration::minutes(1)).unwrap();
    drop(core);
    let mut core = load();
    core.came_up(t0 + Duration::minutes(201)).unwrap();
    // the next check, a minute on: about two minutes of silence, not 202
    let m = owed(&core, 202);
    assert!(m <= 2, "{m}");
    assert!(!expires("w2", m), "not withdrawn at the next check");
    // silent-after of running time since: withdrawn
    let m = owed(&core, 201 + 120);
    assert!((120..=121).contains(&m), "{m}");
    assert!(expires("w2", m));
    // another restart at once: the time down is read back from the record
    core.tickets.quiet.alive(&core.alive_file(), t0 + Duration::minutes(321)).unwrap();
    drop(core);
    let mut core = load();
    core.came_up(t0 + Duration::minutes(321)).unwrap();
    assert!((120..=121).contains(&owed(&core, 321)));
    // a record with no mark: its log's last line is taken as the last time alive
    std::fs::remove_file(core.alive_file()).unwrap();
    drop(core);
    let mut core = load();
    core.came_up(t0 + Duration::minutes(500)).unwrap();
    assert!(owed(&core, 501) <= 2, "{}", owed(&core, 501));
    let _ = std::fs::remove_dir_all(&coord);
}
