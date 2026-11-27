use super::*;
use super::serve::serve;
use std::process::Command;

/// A coord folder under D with a git repository `repo`, one worktree `wt` of it holding a build folder, and Claude
/// Code's projects folder with two transcripts; `yaaihh.toml` beside the coord as TOML gives it.
fn setup(tag: &str, toml: &str) -> (PathBuf, String, Places) {
    let d = std::env::temp_dir().join(format!("cold-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let coord = d.join("coords/research");
    std::fs::create_dir_all(coord.join("claude-home/projects/-work")).unwrap();
    std::fs::create_dir_all(d.join("cold")).unwrap();
    let git = |args: &[&str], at: &Path| assert!(Command::new("git").args(args).current_dir(at).output().unwrap().status.success(), "git {args:?}");
    std::fs::create_dir_all(d.join("repo")).unwrap();
    std::fs::write(d.join("repo/a.rs"), "fn a() {}").unwrap();
    git(&["init", "-q"], &d.join("repo"));
    git(&["add", "."], &d.join("repo"));
    git(&["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "one"], &d.join("repo"));
    git(&["worktree", "add", "-q", d.join("wt").to_str().unwrap()], &d.join("repo"));
    std::fs::create_dir_all(d.join("wt/target/quick")).unwrap();
    std::fs::write(d.join("wt/target/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55").unwrap();
    std::fs::write(d.join("wt/target/quick/coordd"), vec![1u8; 4096]).unwrap();
    for t in ["old", "live"] {
        std::fs::write(coord.join(format!("claude-home/projects/-work/{t}.jsonl")), "{\"type\":\"user\"}\n").unwrap();
    }
    std::fs::write(d.join("coords/yaaihh.toml"), toml.replace("COLD", &d.join("cold").display().to_string())).unwrap();
    let places = Places {
        repos: vec![d.join("repo").display().to_string()],
        transcripts: vec![coord.join("claude-home/projects").display().to_string()],
        live: vec![coord.join("claude-home/projects/-work/live.jsonl").display().to_string()],
    };
    (d.clone(), coord.display().to_string(), places)
}

const RULES: &str = "# the owner's own words\n[[cold.target]]\nname = \"disk\"\nlocal = \"COLD\"\n\n[[cold.rule]]\nwhat = \"worktrees\"\nidle_days = 30\nto = \"disk\"\n\n[[cold.rule]]\nwhat = \"transcripts\"\nidle_days = 90\nto = \"disk\"\n";

fn later(days: u64) -> SystemTime {
    SystemTime::now() + Duration::from_secs(days * 86_400 + 60)
}

#[test]
fn the_dry_run_counts_only_what_is_idle_long_enough_and_moves_nothing() {
    let (d, coord, places) = setup("dry", RULES);
    let cfg = load(&coord).unwrap();
    assert_eq!((cfg.target.len(), cfg.rule.len()), (1, 2));
    assert!(plan(&cfg, &places, SystemTime::now()).is_empty(), "nothing is idle yet");
    let p = plan(&cfg, &places, later(31));
    assert_eq!(p.iter().map(|(i, it)| (*i, it.what.as_str())).collect::<Vec<_>>(), vec![(0, "worktrees")], "the transcripts' 90 days are not up");
    assert!(p[0].1.bytes >= 4096 + 9, "a worktree's bytes count its build folder too: {}", p[0].1.bytes);
    let p = plan(&cfg, &places, later(91));
    let paths: Vec<&str> = p.iter().map(|(_, it)| it.path.as_str()).collect();
    assert!(paths.iter().any(|x| x.ends_with("old.jsonl")) && !paths.iter().any(|x| x.ends_with("live.jsonl")), "a live agent's transcript is never moved: {paths:?}");
    let a: Value = serde_json::from_str(&answer(&coord, &cfg, &p, None)).unwrap();
    assert_eq!(a["plan"]["count"], 2);
    assert_eq!(a["rules"][0]["count"], 1);
    assert_eq!(a["targets"][0]["ok"], true);
    assert!(d.join("wt/a.rs").is_file(), "a dry run moves nothing");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_eviction_leaves_a_pointer_and_a_restore_brings_back_the_same_tree() {
    let (d, coord, places) = setup("evict", RULES);
    let cfg = load(&coord).unwrap();
    let before = Manifest::of(&d.join("wt")).unwrap();
    let p = plan(&cfg, &places, later(31));
    let (done, failed) = evict_all(&coord, &cfg, &p, |_| true);
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(done.len(), 1);
    assert!(!d.join("wt").exists() && pointer_of(&d.join("wt").display().to_string()).is_file(), "the worktree is gone, its pointer in its place");
    let list = String::from_utf8(Command::new("git").args(["worktree", "list", "--porcelain"]).current_dir(d.join("repo")).output().unwrap().stdout).unwrap();
    assert!(list.contains("locked in cold storage"), "git keeps its record, locked: {list}");
    assert_eq!(held(&coord).len(), 1);
    assert!(plan(&cfg, &places, later(31)).is_empty(), "what is in cold storage is not planned again");
    let back = restore(&coord, &cfg, &d.join("wt").display().to_string()).unwrap();
    assert_eq!(back.manifest, before);
    assert_eq!(Manifest::of(&d.join("wt")).unwrap(), before, "the tree came back as it was");
    assert!(!pointer_of(&d.join("wt").display().to_string()).exists() && held(&coord).is_empty());
    assert!(d.join("cold/worktrees").read_dir().unwrap().next().is_some(), "the cold copy is kept after a restore");
    let lines = std::fs::read_to_string(d.join("coords/cold.jsonl")).unwrap();
    assert_eq!(lines.lines().count(), 2, "{lines}");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn nothing_is_removed_when_the_target_cannot_take_it() {
    let (d, coord, places) = setup("down", &RULES.replace("COLD", "/no/such/cold/disk"));
    let cfg = load(&coord).unwrap();
    let p = plan(&cfg, &places, later(31));
    let (done, failed) = evict_all(&coord, &cfg, &p, |_| true);
    assert!(done.is_empty() && failed.len() == 1 && failed[0].contains("not there"), "{failed:?}");
    assert!(d.join("wt/a.rs").is_file() && !pointer_of(&d.join("wt").display().to_string()).exists());
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn builds_are_found_in_the_repository_and_its_worktrees() {
    let (d, coord, places) = setup("builds", &format!("{RULES}\n[[cold.rule]]\nwhat = \"builds\"\nidle_days = 7\nto = \"disk\"\n"));
    let cfg = load(&coord).unwrap();
    let r = &cfg.rule[2];
    let found = candidates(r, &places, later(8));
    assert_eq!(found.iter().map(|it| it.path.clone()).collect::<Vec<_>>(), vec![d.join("wt/target").display().to_string()]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn auto_is_set_in_the_file_and_its_other_text_kept() {
    let (d, coord, places) = setup("auto", RULES);
    assert!(serve(&coord, &places, false, Some("auto"), None, Some(1), Some(true)).is_err(), "the owner's alone");
    let a: Value = serde_json::from_str(&serve(&coord, &places, true, Some("auto"), None, Some(1), Some(true)).unwrap()).unwrap();
    assert_eq!(a["rules"][1]["auto"], true);
    assert_eq!(a["said"], "rule 2 is automatic now");
    let text = std::fs::read_to_string(d.join("coords/yaaihh.toml")).unwrap();
    assert!(text.starts_with("# the owner's own words") && text.contains("auto = true"), "{text}");
    assert!(serve(&coord, &places, true, Some("auto"), None, Some(9), Some(true)).unwrap_err().contains("no cold rule 10"));
    let _ = std::fs::remove_dir_all(&d);
}
