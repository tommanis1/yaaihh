use super::*;

fn home(tag: &str) -> PathBuf {
    let h = std::env::temp_dir().join(format!("read-ask-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&h);
    for d in [".gemini/antigravity-cli", ".claude/projects/p/memory", ".ssh", "work", "notes/old"] {
        std::fs::create_dir_all(h.join(d)).unwrap();
    }
    for f in [".gemini/antigravity-cli/paper.md", ".gemini/oauth_creds.json", ".claude/projects/p/memory/m.md", ".ssh/id_ed25519", "notes/old/a.md"] {
        std::fs::write(h.join(f), "x").unwrap();
    }
    std::fs::canonicalize(&h).unwrap()
}

fn read(p: &Path) -> Value {
    json!({"hook_event_name": "PermissionRequest", "permission_mode": "default", "tool_name": "Read", "tool_input": {"file_path": p.display().to_string()}})
}

#[test]
fn only_a_whole_path_of_a_reading_tool_is_judged_and_a_bypassing_session_never() {
    assert_eq!(asked(&read(Path::new("/a/b.md"))), Some(("/a/b.md".into(), false)));
    let glob = json!({"permission_mode": "acceptEdits", "tool_name": "Glob", "tool_input": {"pattern": "/a/notes/**/*.md"}});
    assert_eq!(asked(&glob), Some(("/a/notes".into(), true)), "a Glob's folder is its pattern's whole part");
    let grep = json!({"permission_mode": "default", "tool_name": "Grep", "tool_input": {"pattern": "x", "path": "/a"}});
    assert_eq!(asked(&grep), Some(("/a".into(), true)));
    for v in [json!({"permission_mode": "bypassPermissions", "tool_name": "Read", "tool_input": {"file_path": "/a"}}), json!({"tool_name": "Bash", "tool_input": {"command": "cat /a"}}), json!({"tool_name": "Write", "tool_input": {"file_path": "/a"}}), json!({"tool_name": "Read", "tool_input": {"file_path": "a/b"}})] {
        assert_eq!(asked(&v), None, "{v}");
    }
}

#[test]
fn a_path_is_judged_as_the_machine_has_it_and_a_place_no_agent_reads_stays_closed() {
    let h = home("closed");
    assert_eq!(resolve(&format!("{}/work/../.ssh/./id_ed25519", h.display())), Some(h.join(".ssh/id_ed25519")));
    std::os::unix::fs::symlink(h.join(".ssh"), h.join("notes/keys")).unwrap();
    assert_eq!(resolve(&format!("{}/notes/keys/id_ed25519", h.display())), Some(h.join(".ssh/id_ed25519")), "a link is followed");
    assert_eq!(resolve(&format!("{}/notes/keys/none/yet", h.display())), Some(h.join(".ssh/none/yet")), "and a path not there yet with it");
    for p in [".ssh/id_ed25519", ".gemini/oauth_creds.json", ".claude/projects/p/memory/m.md", "agent-coord/accounts/a/x", "work/.git/config"] {
        assert!(closed(&h, &h.join(p)).is_some(), "{p}");
    }
    assert_eq!(closed(&h, &h.join(".gemini/antigravity-cli/paper.md")), None);
    assert_eq!(offered(&h, &h.join(".gemini/antigravity-cli/paper.md"), false), Some(h.join(".gemini/antigravity-cli")));
    assert_eq!(offered(&h, &h.join("notes/old"), true), Some(h.join("notes/old")), "a folder named is offered itself");
    assert_eq!(offered(&h, &h.join("x.md"), false), None, "the home is too wide");
    assert_eq!(offered(&h, Path::new("/etc/hosts"), false), None, "a folder at the machine's top too");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_read_waits_on_one_card_and_his_answer_is_the_calls() {
    let h = home("asks");
    let mut a = Asks::default();
    let paper = h.join(".gemini/antigravity-cli/paper.md");
    let folders = vec![h.join("notes").display().to_string()];
    assert!(matches!(a.judge("clerk", &read(&h.join("notes/old/a.md")), &h, &folders, "t0"), Judged::Allow), "inside its read folders");
    assert!(matches!(a.judge("clerk", &read(&h.join(".ssh/id_ed25519")), &h, &folders, "t0"), Judged::Refuse(w) if w.contains("no agent reads")));
    std::os::unix::fs::symlink(h.join(".ssh"), h.join("notes/keys")).unwrap();
    assert!(matches!(a.judge("clerk", &read(&h.join("notes/keys/id_ed25519")), &h, &folders, "t0"), Judged::Refuse(_)), "a link out of its folders to a closed place");
    assert!(matches!(a.judge("clerk", &read(Path::new("relative")), &h, &folders, "t0"), Judged::Pass));

    // one card, however many calls wait on it
    let Judged::Wait(id, mut first) = a.judge("clerk", &read(&paper), &h, &folders, "t1") else { panic!() };
    let Judged::Wait(same, mut second) = a.judge("clerk", &read(&paper), &h, &folders, "t2") else { panic!() };
    assert_eq!((id, a.of("clerk").len(), a.of("other").len()), (same, 1, 0));
    assert_eq!(a.of("clerk")[0], ReadAsk { id, path: paper.display().to_string(), folder: Some(h.join(".gemini/antigravity-cli").display().to_string()), at: "t1".into() });
    assert!(a.answer("other", id, ReadAnswer::Folder).is_err(), "an ask is its agent's");
    let (_, kept) = a.answer("clerk", id, ReadAnswer::Folder).unwrap();
    assert_eq!(kept, Some(h.join(".gemini/antigravity-cli").display().to_string()), "the folder to keep in the project file");
    assert_eq!((first.try_recv(), second.try_recv()), (Ok(Ok(())), Ok(Ok(()))));
    assert!(a.of("clerk").is_empty());

    // once: the waiting call goes through; with none waiting (it ran out), the next read of that path does, once
    let other = h.join("work/x.md");
    let Judged::Wait(id, mut rx) = a.judge("clerk", &read(&other), &h, &[], "t3") else { panic!() };
    a.answer("clerk", id, ReadAnswer::Once).unwrap();
    assert_eq!(rx.try_recv(), Ok(Ok(())));
    assert!(matches!(a.judge("clerk", &read(&other), &h, &[], "t4"), Judged::Wait(..)), "once is once");
    let id = a.of("clerk")[0].id;
    a.answer("clerk", id, ReadAnswer::Once).unwrap();
    assert!(matches!(a.judge("clerk", &read(&other), &h, &[], "t5"), Judged::Allow), "his once kept for the call that ran out");
    assert!(matches!(a.judge("clerk", &read(&other), &h, &[], "t6"), Judged::Wait(..)));

    // no: said to the call, and to every later read in that folder without a card
    let id = a.of("clerk")[0].id;
    let Judged::Wait(_, mut rx) = a.judge("clerk", &read(&other), &h, &[], "t7") else { panic!() };
    a.answer("clerk", id, ReadAnswer::Deny).unwrap();
    assert!(matches!(rx.try_recv(), Ok(Err(w)) if w.contains("said no")));
    assert!(matches!(a.judge("clerk", &read(&h.join("work/y.md")), &h, &[], "t8"), Judged::Refuse(w) if w.contains("said no")));
    assert!(a.of("clerk").is_empty());
    assert!(matches!(a.judge("other", &read(&h.join("work/y.md")), &h, &[], "t9"), Judged::Wait(..)), "his no was to that agent");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_path_whose_folder_is_too_wide_is_allowed_once_or_not_at_all() {
    let h = home("wide");
    let mut a = Asks::default();
    std::fs::write(h.join("top.md"), "x").unwrap();
    let Judged::Wait(id, _) = a.judge("clerk", &read(&h.join("top.md")), &h, &[], "t") else { panic!() };
    assert_eq!(a.of("clerk")[0].folder, None);
    assert!(a.answer("clerk", id, ReadAnswer::Folder).is_err());
    assert_eq!(a.of("clerk").len(), 1, "his card stays for Once or Deny");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn an_allowed_folder_is_one_line_added_to_the_project_file_and_reads_back_as_the_agents() {
    let text = "# his\n[project]\nrepo = \"/w\"\n\n[read.other]\nfolders = [\"/o\"]\n";
    let once = with_read(text, "clerk-paper-setup", "/h/.gemini/antigravity-cli").unwrap();
    assert_eq!(with_read(&once, "clerk-paper-setup", "/h/.gemini/antigravity-cli").unwrap(), once, "once");
    let twice = with_read(&once, "other", "/p").unwrap();
    assert!(twice.starts_with("# his\n[project]\nrepo = \"/w\"\n"), "{twice}");
    let p: crate::project::Project = toml::from_str(&twice).unwrap();
    assert_eq!(p.agent_read("clerk-paper-setup"), vec!["/h/.gemini/antigravity-cli".to_string()]);
    assert_eq!(p.agent_read("other"), vec!["/o".to_string(), "/p".to_string()]);
    assert_eq!(with_read("", "a", "/x").unwrap().trim(), "[read.a]\nfolders = [\"/x\"]");
}

#[test]
fn a_link_is_followed_before_the_dots_after_it_as_the_kernel_does() {
    let h = home("dots");
    std::fs::create_dir_all(h.join("secret_parent/sub")).unwrap();
    std::os::unix::fs::symlink(h.join("secret_parent/sub"), h.join("notes/link")).unwrap();
    let p = resolve(&format!("{}/notes/link/../target", h.display())).unwrap();
    assert_eq!(p, h.join("secret_parent/target"));
    assert!(!within(&p, &[h.join("notes").display().to_string()]));
    assert_eq!(resolve(&format!("{}/notes/none/../old", h.display())), Some(h.join("notes/old")), "a part not there is taken lexically");
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_tool_that_reads_a_folder_whole_is_refused_where_that_holds_a_closed_place() {
    let h = home("tree");
    for d in [".local/share", "agent-coord/p/work"] {
        std::fs::create_dir_all(h.join(d)).unwrap();
    }
    let grep = |p: &Path| json!({"permission_mode": "default", "tool_name": "Grep", "tool_input": {"pattern": "x", "path": p.display().to_string()}});
    let mut a = Asks::default();
    for p in [h.clone(), h.join(".gemini"), h.join("agent-coord"), h.join(".local"), PathBuf::from("/"), h.parent().unwrap().to_path_buf()] {
        assert!(matches!(a.judge("clerk", &grep(&p), &h, &[], "t"), Judged::Refuse(_)), "{}", p.display());
    }
    let inside = h.join(".gemini/antigravity-cli");
    let Judged::Wait(..) = a.judge("clerk", &grep(&inside), &h, &[], "t") else { panic!("inside") };
    assert_eq!(a.of("clerk")[0].folder, Some(inside.display().to_string()));
    let Judged::Wait(..) = a.judge("clerk", &grep(&h.join(".local/share")), &h, &[], "t") else { panic!("beside .local/opt") };
    assert!(a.of("clerk").iter().all(|k| k.folder.as_deref() != Some(h.join(".gemini").display().to_string().as_str())));
    let glob = |pat: String, path: Option<String>| json!({"permission_mode": "default", "tool_name": "Glob", "tool_input": {"pattern": pat, "path": path}});
    let up = [glob(format!("{}/*/../../.ssh/*", inside.display()), None), glob("../../.ssh/*".into(), Some(inside.display().to_string())), glob(format!("{}/**", h.display()), Some(inside.display().to_string()))];
    for g in up {
        assert!(matches!(a.judge("clerk", &g, &h, &[], "t"), Judged::Refuse(_)), "{g}");
    }
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_call_that_ran_out_leaves_nothing_on_its_card() {
    let h = home("closedtx");
    let mut a = Asks::default();
    let paper = h.join(".gemini/antigravity-cli/paper.md");
    for _ in 0..3 {
        let Judged::Wait(_, rx) = a.judge("clerk", &read(&paper), &h, &[], "t") else { panic!() };
        drop(rx);
    }
    let Judged::Wait(id, _rx) = a.judge("clerk", &read(&paper), &h, &[], "t") else { panic!() };
    assert_eq!(a.open[&id].2.len(), 1);
    let _ = std::fs::remove_dir_all(&h);
}

#[test]
fn a_repository_outside_is_offered_and_read_whole_but_its_git_folder_is_not() {
    let h = home("repo");
    std::fs::create_dir_all(h.join("other-repo/.git")).unwrap();
    std::fs::write(h.join("other-repo/.git/config"), "x").unwrap();
    std::fs::write(h.join("other-repo/paper.tex"), "x").unwrap();
    let mut a = Asks::default();
    let Judged::Wait(..) = a.judge("clerk", &read(&h.join("other-repo/paper.tex")), &h, &[], "t") else { panic!("read") };
    assert_eq!(a.of("clerk")[0].folder, Some(h.join("other-repo").display().to_string()));
    let grep = json!({"permission_mode": "default", "tool_name": "Grep", "tool_input": {"pattern": "x", "path": h.join("other-repo").display().to_string()}});
    assert!(matches!(a.judge("clerk", &grep, &h, &[], "t"), Judged::Wait(..)));
    assert!(matches!(a.judge("clerk", &read(&h.join("other-repo/.git/config")), &h, &[], "t"), Judged::Refuse(_)));
    std::fs::create_dir_all(h.join("agent-coord/p")).unwrap();
    std::fs::write(h.join("agent-coord/p/registry.jsonl"), "").unwrap();
    std::fs::create_dir_all(h.join("elsewhere")).unwrap();
    std::fs::write(h.join("elsewhere/.netrc"), "x").unwrap();
    for p in [h.join("agent-coord/p"), h.join("elsewhere")] {
        let g = json!({"permission_mode": "default", "tool_name": "LS", "tool_input": {"path": p.display().to_string()}});
        assert!(matches!(a.judge("clerk", &g, &h, &[], "t"), Judged::Refuse(_)), "{}", p.display());
    }
    let _ = std::fs::remove_dir_all(&h);
}
