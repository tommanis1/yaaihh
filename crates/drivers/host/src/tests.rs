//! The host driver: times, commits, sizes and proofs as Events; Git-Head and Sleep; its names in the texts.

use super::*;
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("driver-host-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&d));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn named(event: &str, from: &str) -> Named {
    Named { event: event.into(), from: from.into() }
}

fn ev(ty: &str, args: Vec<Value>) -> Instance {
    Instance { ty: ty.into(), args }
}

/// Nothing live, and nothing holds.
fn blind() -> Seen {
    Seen { live: Box::new(Vec::new), holds: Box::new(|_, _| false) }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let base = ["-c", "user.name=t", "-c", "user.email=t@example.invalid", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", "-C"];
    let out = std::process::Command::new("git").args(base).arg(repo).args(args).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_time_key_names_its_utc_minute_and_others_name_none() {
    assert_eq!(clock::minute_of("1970-01-01"), Some(0));
    assert_eq!(clock::minute_of("2000-03-01T00:01Z"), Some(11_017 * 1440 + 1));
    assert_eq!(clock::minute_of("2026-11-27T09:30:15Z"), clock::minute_of("2026-11-27").map(|m| m + 9 * 60 + 30));
    for bad in ["soon", "2026-13-01", "2026-11-27T09:30", "N-1832.p9"] {
        assert_eq!(clock::minute_of(bad), None, "{bad}");
    }
}

#[test]
fn each_minute_ticks_and_a_listed_time_passes_once_when_it_is_past() {
    let d = dir("clock");
    let times = d.join("times");
    std::fs::write(&times, "2026-11-26T10:00Z\n2026-11-26T12:00Z\nnot a time\n").unwrap();
    let now = clock::minute_of("2026-11-26T11:00Z").unwrap();
    let mut c = clock::Clock::default();
    let t = named("p.Passed", &times.display().to_string());
    let passed = |k: &str| ev("p.Passed", vec![Value::text(k)]);
    assert_eq!(c.tick(now, Some(&t)), [ev(clock::TICK, vec![Value::Int(now)]), passed("2026-11-26T10:00Z")]);
    assert_eq!(c.tick(now + 1, Some(&t)), [ev(clock::TICK, vec![Value::Int(now + 1)])]);
    assert_eq!(c.tick(now + 60, Some(&t)), [ev(clock::TICK, vec![Value::Int(now + 60)]), passed("2026-11-26T12:00Z")]);
    assert_eq!(c.tick(now + 61, None), [ev(clock::TICK, vec![Value::Int(now + 61)])]);
    assert_eq!(c.tick(now + 62, Some(&named("p.Passed", "/nonexistent/times"))), [ev(clock::TICK, vec![Value::Int(now + 62)])]);
    drop(std::fs::remove_dir_all(&d));
}

#[test]
fn a_new_commit_is_committed_by_its_agents_and_changes_its_items_and_head_names_it() {
    let d = dir("git");
    git(&d, &["init", "-q", "-b", "main"]);
    git(&d, &["commit", "-q", "--allow-empty", "-m", "before the first look\n\nAgent: early"]);
    let mut commits = git::Commits::default();
    let committed = named("p.Committed", "Agent");
    let said = git::Said { committed: Some(&committed), changed_by: Some("Item") };
    assert_eq!(commits.look(std::slice::from_ref(&d), &said), [], "the first look only notes the tips");
    git(&d, &["commit", "-q", "--allow-empty", "-m", "one\n\nAgent: docket-core\nItem: I-1, N-2.p3"]);
    let one = git(&d, &["rev-parse", "HEAD"]);
    git(&d, &["switch", "-q", "-c", "side"]);
    git(&d, &["commit", "-q", "--allow-empty", "-m", "two, with no trailers"]);
    let two = git(&d, &["rev-parse", "HEAD"]);
    assert_eq!(
        commits.look(std::slice::from_ref(&d), &said),
        [
            ev("p.Committed", vec![Value::text("docket-core"), Value::text(&one)]),
            ev(git::ARTIFACT_CHANGED, vec![Value::text("I-1")]),
            ev(git::ARTIFACT_CHANGED, vec![Value::text("N-2.p3")]),
        ]
    );
    assert_eq!(commits.look(std::slice::from_ref(&d), &said), [], "nothing new");
    let mut h = Host::new(Config { head: Some(d.clone()), ..Config::default() }, Duration::from_secs(3600), blind()).unwrap();
    assert_eq!(h.carry(GIT_HEAD, &[], 1), Reply::Answer { value: Value::text(&two), costs: Vec::new(), events: Vec::new() });
    let mut none = Host::new(Config::default(), Duration::from_secs(3600), blind()).unwrap();
    assert!(matches!(none.carry(GIT_HEAD, &[], 1), Reply::Failed(_)));
    drop(std::fs::remove_dir_all(&d));
}

#[test]
fn a_kept_folder_is_sized_in_whole_megabytes_and_a_proof_file_is_captured_once() {
    let d = dir("store");
    let kept = d.join("kept");
    std::fs::create_dir_all(kept.join("in")).unwrap();
    std::fs::write(kept.join("in/a"), vec![0u8; 1_500_000]).unwrap();
    let key = kept.display().to_string();
    assert_eq!(store::sized(&[kept.clone(), d.join("gone")], SystemTime::now()), [ev(store::SIZED, vec![Value::text(&key), Value::Int(2), Value::Int(0)])]);
    let proofs = d.join("proofs");
    std::fs::create_dir_all(&proofs).unwrap();
    std::fs::write(proofs.join("a.proof"), "I-1\nlive_screenshot_verified\nstills/a.png\n0.4.1\n").unwrap();
    std::fs::write(proofs.join("b.proof"), "only two\nlines\n").unwrap();
    let args = vec![Value::text("I-1"), Value::text("live_screenshot_verified"), Value::text("stills/a.png"), Value::text("0.4.1")];
    assert_eq!(store::proofs(&proofs, "p.Captured"), [ev("p.Captured", args)]);
    assert!(proofs.join("a.taken").exists() && proofs.join("b.bad").exists());
    assert_eq!(store::proofs(&proofs, "p.Captured"), []);
    drop(std::fs::remove_dir_all(&d));
}

/// What `ready` gave, until `until` holds of it or three seconds pass.
fn outs(h: &mut Host, until: impl Fn(&[(u64, Reply)], &[Instance]) -> bool) -> (Vec<(u64, Reply)>, Vec<Instance>) {
    let (mut answers, mut events) = (Vec::new(), Vec::new());
    for _ in 0..300 {
        let (a, e) = h.ready();
        answers.extend(a);
        events.extend(e);
        if until(&answers, &events) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    (answers, events)
}

fn unit() -> Reply {
    Reply::Answer { value: Value::Unit, costs: Vec::new(), events: Vec::new() }
}

#[test]
fn the_watch_ticks_and_a_sleep_is_answered_by_its_line_unless_dropped() {
    let mut h = Host::new(Config::default(), Duration::from_millis(100), blind()).unwrap();
    let (_, ticks) = outs(&mut h, |_, e| e.len() >= 2);
    assert!(ticks.len() >= 2 && ticks.iter().all(|e| e.ty == clock::TICK), "{ticks:?}");
    assert_eq!(h.carry(SLEEP, &[Value::Int(0)], 4), unit());
    assert_eq!(h.carry(SLEEP, &[Value::Int(1)], 5), Reply::Later);
    assert_eq!(h.carry(SLEEP, &[Value::Int(1)], 6), Reply::Later);
    h.dropped(6);
    let (mut answers, _) = outs(&mut h, |a, _| !a.is_empty());
    std::thread::sleep(Duration::from_millis(300));
    answers.extend(h.ready().0);
    assert_eq!(answers, [(5, unit())]);
    assert!(h.sleeping.is_empty());
    assert!(matches!(h.carry(SLEEP, &[Value::text("1")], 7), Reply::Failed(_)));
}

/// Each name the driver gives with the project's host file is declared, as that kind, in the shipped texts.
#[test]
fn every_event_and_effect_it_names_with_the_projects_file_is_declared_in_the_shipped_texts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docket");
    let mut declared = Vec::new();
    for sub in ["std", "defaults"] {
        for f in std::fs::read_dir(root.join(sub)).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "docket")) {
            let text = std::fs::read_to_string(&f).unwrap();
            let module = text.lines().find_map(|l| l.strip_prefix("module ")).unwrap().trim().to_string();
            for l in text.lines() {
                for kind in ["Event ", "Effect "] {
                    if let Some(rest) = l.strip_prefix(kind) {
                        declared.push(format!("{kind}{module}.{}", rest.split('(').next().unwrap()));
                    }
                }
            }
        }
    }
    let file = std::fs::read_to_string(root.join("defaults/host.json")).unwrap();
    let h = Host::new(Config::read(&file, &dir("names")).unwrap(), Duration::from_secs(3600), blind()).unwrap();
    assert_eq!(h.events().len(), 6, "Tick, Sized, Artifact-Changed and the project's three");
    let named = h.events().into_iter().map(|e| format!("Event {e}")).chain(h.effects().into_iter().map(|(e, _)| format!("Effect {e}")));
    for n in named {
        assert!(declared.contains(&n), "{n} is not declared");
    }
}

#[test]
fn a_pick_is_the_first_live_party_holding_the_role_and_fails_when_none_does() {
    let seen = Seen {
        live: Box::new(|| vec!["w1".into(), "w2".into(), "w3".into()]),
        holds: Box::new(|m, q| m == "std.comms" && ["Role(\"w2\", \"looker\")", "Role(\"w3\", \"looker\")", "Role(\"gone\", \"keeper\")"].contains(&q)),
    };
    let mut h = Host::new(Config::default(), Duration::from_secs(3600), seen).unwrap();
    for (line, role) in [(1, "looker"), (2, "keeper"), (3, "clerk")] {
        assert_eq!(h.carry(PICK, &[Value::text(role)], line), Reply::Later);
    }
    h.dropped(3);
    let (answers, _) = outs(&mut h, |a, _| a.len() >= 2);
    assert_eq!(answers[0], (1, Reply::Answer { value: Value::text("w2"), costs: Vec::new(), events: Vec::new() }));
    assert_eq!(answers[1], (2, Reply::Failed("no party with a live session holds the role \"keeper\"".into())));
    assert_eq!(answers.len(), 2, "a dropped pick gets no answer");
}

#[test]
fn a_pick_of_another_family_passes_over_a_looker_of_a_writers_family() {
    let facts = ["Role(\"lk1\", \"looker\")", "Role(\"lk2\", \"looker\")", "Other-Family(\"lk1\", \"w-codex\")", "Other-Family(\"lk2\", \"w-claude\")"];
    let seen = Seen { live: Box::new(|| vec!["w-claude".into(), "lk1".into(), "lk2".into()]), holds: Box::new(move |_, q| facts.contains(&q)) };
    let mut h = Host::new(Config::default(), Duration::from_secs(3600), seen).unwrap();
    let asks = [
        (1, vec![Value::text("w-claude")]),
        (2, vec![Value::text("w-codex")]),
        (3, vec![Value::text("w-claude"), Value::text("w-codex")]),
        (4, vec![]),
    ];
    for (line, than) in asks {
        assert_eq!(h.carry(PICK_OTHER, &[Value::text("looker"), Value::List(than)], line), Reply::Later);
    }
    let one_field = Value::Inst(Instance { ty: "std.hooks.Party".into(), args: vec![Value::text("w-codex")] });
    for (line, other) in [(5, Value::Int(1)), (6, one_field)] {
        assert!(matches!(h.carry(PICK_OTHER, &[Value::text("looker"), Value::List(vec![other])], line), Reply::Failed(w) if w.contains("no name")), "a party is its text alone");
    }
    let (answers, _) = outs(&mut h, |a, _| a.len() >= 4);
    let picked = |r: &Reply| if let Reply::Answer { value, .. } = r { value.clone() } else { panic!("{r:?}") };
    assert_eq!((picked(&answers[0].1), picked(&answers[1].1)), (Value::text("lk2"), Value::text("lk1")), "each writer's other family");
    assert!(matches!(&answers[2].1, Reply::Failed(w) if w.ends_with("of another family than w-claude, w-codex")), "{:?}", answers[2]);
    assert_eq!(picked(&answers[3].1), Value::text("lk1"), "no writer: the first looker");
}

#[test]
fn a_restart_from_the_state_file_neither_repeats_a_time_nor_loses_a_commit() {
    let d = dir("restart");
    let repo = d.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "before"]);
    std::fs::write(d.join("times"), "2026-11-26T10:00Z\n").unwrap();
    let file = r#"{"repos": ["repo"], "committed": {"event": "p.Committed", "from": "Agent"}, "times": {"event": "p.Passed", "from": "times"}, "state": "state.json"}"#;
    let c = Config::read(file, &d).unwrap();
    let now = SystemTime::now();
    let kinds = |evs: Vec<Instance>| evs.into_iter().map(|e| e.ty).filter(|t| t != clock::TICK && t != store::SIZED).collect::<Vec<_>>();
    let mut w = watch::Watch::load(c.state.as_deref()).unwrap();
    assert_eq!(kinds(w.look(&c, now)), ["p.Passed"]);
    w.save(c.state.as_deref().unwrap()).unwrap();
    drop(w);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "while down\n\nAgent: a1"]);
    let mut again = watch::Watch::load(c.state.as_deref()).unwrap();
    assert_eq!(kinds(again.look(&c, now)), ["p.Committed"], "the time is not said again, and the commit made while down comes");
    std::fs::write(c.state.as_deref().unwrap(), "not json").unwrap();
    assert!(watch::Watch::load(c.state.as_deref()).is_err(), "a state file that cannot be read stops the start");
    drop(std::fs::remove_dir_all(&d));
}
