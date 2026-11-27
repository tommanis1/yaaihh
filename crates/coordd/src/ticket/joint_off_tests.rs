//! `off` written into the coord's `run-acts` stops the joint's thread without a restart (paper-research N-788): coordd
//! stats the switch's file at each send, and once it has changed and no longer says `both` it drops the sender.

use super::Core;
use docket_machine::Input;
use serde_json::json;

#[test]
fn off_written_into_the_switch_stops_the_joint_at_the_next_input() {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-joint-off-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/a.call"), "call ask(q: Text) -> Said(a: Text)\n  template\n    {{ q }}\n").unwrap();
    std::fs::write(project.join("kinds/a.kind"), "kind e\n  letter E\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    return q\n").unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"d\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let switch = coord.join(crate::joint::SWITCH);
    // the file's time moved by hand, so the test does not rest on the filesystem's clock grain
    let write = |words: &str, secs: u64| {
        std::fs::write(&switch, words).unwrap();
        let f = std::fs::File::options().write(true).open(&switch).unwrap();
        f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000 + secs)).unwrap();
    };
    write("both\n", 0);
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    assert!(core.tickets.joint.is_some(), "run-acts both starts the joint");
    let open = |core: &mut Core| core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": "why?"}), text: None, names: vec![], told: vec![] }).unwrap();
    open(&mut core);
    assert!(core.tickets.joint.is_some());
    // the file written again, still `both`: the joint goes on
    write("both\n", 60);
    open(&mut core);
    assert!(core.tickets.joint.is_some(), "a switch rewritten as both keeps the joint");
    // `off`: the next input drops the sender, and the thread's loop ends
    write("off\n", 120);
    open(&mut core);
    assert!(core.tickets.joint.is_none(), "off stops the joint at the next input");
    // `both` again starts nothing until coordd's next start; the machine went on throughout
    write("both\n", 180);
    open(&mut core);
    assert!(core.tickets.joint.is_none());
    assert_eq!(core.tickets.m.tickets.len(), 4);
    // a switch's file taken away is off as well
    write("both\n", 240);
    core.load_tickets();
    assert!(core.tickets.joint.is_some());
    std::fs::remove_file(&switch).unwrap();
    open(&mut core);
    assert!(core.tickets.joint.is_none(), "no file is off");
    std::fs::remove_dir_all(&coord).unwrap();
}
