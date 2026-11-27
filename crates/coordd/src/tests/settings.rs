use super::*;

/// A coord COORD under D/coords and the places its settings are written from.
fn coord(d: &Path) -> (PathBuf, Places, String) {
    let _ = std::fs::remove_dir_all(d);
    let coord = d.join("coords/walk");
    std::fs::create_dir_all(&coord).unwrap();
    let at = Places { home: d.display().to_string(), coord: coord.display().to_string(), ..Default::default() };
    let c = coord.display().to_string();
    (coord, at, c)
}

/// Each key at each level goes in the file its reader reads, his comments kept; "" unsets; a value of the wrong kind,
/// an unknown key or level, or a file in a git repository outside the coord refused, nothing written.
#[test]
fn each_setting_goes_in_its_readers_file_at_each_level() {
    let d = std::env::temp_dir().join(format!("coordd-settings-{}", std::process::id()));
    let (coord, at, c) = coord(&d);
    let load = || Project::load(&c).unwrap();
    let read_now = || -> Value { serde_json::from_str(&read(&c, load().as_ref(), None)).unwrap() };
    let all_null: serde_json::Map<String, Value> = keys().into_iter().map(|k| (k, Value::Null)).collect();
    assert_eq!(read_now(), json!({"yaaihh": all_null, "project": all_null, "said": null}));
    for (level, key, value, why) in [
        ("project", "agent_mode", "auto", "not a mode"),
        ("project", "safety_check", "maybe", "true or false"),
        ("project", "boxed", "1", "true or false"),
        ("project", "owner_speakers", "docket core", "not an agent's name"),
        ("project", "models.claude-gateway", "claude opus", "not a model's id"),
        ("project", "models.gemini", "x", "no such setting"),
        ("machine", "agent_mode", "yolo", "the level is"),
        ("yaaihh", "safety_check", "false", "in no flavor"),
    ] {
        assert!(set(&at, None, level, key, value).unwrap_err().contains(why), "{key} = {value}");
    }
    assert!(!coord.join("project").exists(), "nothing written");
    // the project file (a new one of the coord's own) and yaaihh.toml, his comment kept
    assert!(set(&at, None, "project", "agent_mode", "yolo").unwrap().contains("in a new project file"));
    std::fs::write(d.join("coords/yaaihh.toml"), "# his\nagent_mode = \"safe\"\n").unwrap();
    assert!(set(&at, None, "yaaihh", "agent_mode", "yolo").unwrap().contains("yaaihh.toml.previous is the file as it was"));
    set(&at, None, "yaaihh", "models.agy-personal", "agy/gemini-3-pro").unwrap();
    set(&at, None, "yaaihh", "boxed", "true").unwrap();
    let wide = std::fs::read_to_string(d.join("coords/yaaihh.toml")).unwrap();
    assert!(wide.starts_with("# his\nagent_mode = \"yolo\"\n"), "his comment kept: {wide}");
    let t: toml::Table = wide.parse().unwrap();
    assert_eq!((t["box"].as_bool(), t["models"]["agy-personal"].as_str()), (Some(true), Some("agy/gemini-3-pro")));
    set(&at, load().as_ref(), "project", "owner_speakers", "docket-core, paper-research").unwrap();
    set(&at, load().as_ref(), "project", "models.claude-gateway", "claude-opus-5-5").unwrap();
    set(&at, load().as_ref(), "project", "boxed", "false").unwrap();
    let r = read_now();
    assert_eq!((&r["project"]["owner_speakers"], &r["project"]["models.claude-gateway"], &r["project"]["boxed"]), (&json!("docket-core,paper-research"), &json!("claude-opus-5-5"), &json!(false)));
    assert_eq!((&r["yaaihh"]["models.agy-personal"], &r["yaaihh"]["boxed"], &r["yaaihh"]["agent_mode"]), (&json!("agy/gemini-3-pro"), &json!(true), &json!("yolo")));
    // the safety check: the project's own folder's harness.toml
    assert!(set(&at, load().as_ref(), "project", "safety_check", "false").unwrap().contains("definition/harness.toml"));
    assert_eq!(read_now()["project"]["safety_check"], false);
    for k in ["safety_check", "agent_mode", "boxed", "owner_speakers", "models.claude-gateway"] {
        set(&at, load().as_ref(), "project", k, "").unwrap();
    }
    assert_eq!(read_now()["project"], json!(all_null), "unset");
    // the flavor's root once the project is in one, and refused once that flavor is a git repository
    let flavor = d.join("flavor");
    std::fs::create_dir_all(flavor.join("projects/walk")).unwrap();
    std::fs::write(flavor.join("projects/walk/project.toml"), "[project]\n").unwrap();
    std::fs::write(coord.join("project"), format!("{}\n", flavor.join("projects/walk/project.toml").display())).unwrap();
    assert!(set(&at, load().as_ref(), "yaaihh", "safety_check", "true").unwrap().contains("flavor/harness.toml"));
    assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(&flavor).status().unwrap().success());
    assert!(set(&at, load().as_ref(), "yaaihh", "safety_check", "false").unwrap_err().contains("a git repository outside this coord"));
    assert_eq!(read_now()["yaaihh"]["safety_check"], true);
    let _ = std::fs::remove_dir_all(&d);
}

/// Who may speak to the owner and the box default: the project's, else every project's, else anyone and no box.
#[test]
fn the_speakers_and_the_box_are_the_projects_else_every_projects() {
    let d = std::env::temp_dir().join(format!("coordd-speakers-{}", std::process::id()));
    let (_coord, at, c) = coord(&d);
    let load = || Project::load(&c).unwrap();
    assert_eq!((speakers(&c, None), wide_box(&c)), (Speakers::Any, None));
    set(&at, None, "yaaihh", "owner_speakers", "leads").unwrap();
    set(&at, None, "yaaihh", "boxed", "true").unwrap();
    assert_eq!((speakers(&c, None), wide_box(&c)), (Speakers::Leads, Some(true)));
    set(&at, None, "project", "owner_speakers", "w1,w2").unwrap();
    assert_eq!(speakers(&c, load().as_ref()), Speakers::Named(vec!["w1".into(), "w2".into()]));
    assert!(load().unwrap().project.boxed, "no box of its own: every project's");
    set(&at, load().as_ref(), "project", "boxed", "false").unwrap();
    assert!(!load().unwrap().project.boxed, "its own wins");
    let _ = std::fs::remove_dir_all(&d);
}
