//! What is due, kept between reads (a2h-speed, paper-research N-560 p37): the follow round asks it each second of
//! every agent, the clerk, the watcher and the far names; it is read from the machine once and given again only while
//! the machine is as it was, so every answer is the one a fresh read gives.

use super::Core;
use docket_machine::{Input, Text};
use serde_json::json;

fn core(tag: &str) -> (Core, std::path::PathBuf) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-due-kept-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"w\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    let reg: String = ["w2", "w3"].iter().enumerate().map(|(i, n)| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}}\n")).collect();
    std::fs::write(coord.join("registry.jsonl"), reg).unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    (core, coord)
}

#[test]
fn what_is_due_is_read_once_while_the_machine_stands_and_afresh_when_it_moves() {
    let (mut core, coord) = core("kept");
    let names: Vec<String> = ["w2", "w3", "clerk", docket_machine::WATCHER].iter().map(|s| s.to_string()).collect();
    let ids = |core: &Core, n: &str| -> Vec<String> { core.tickets.due(n).into_iter().map(|d| d.id).collect() };
    let fresh = |core: &Core, n: &str| -> Vec<String> { core.tickets.m.due(n).into_iter().filter(|d| !core.tickets.handed.contains_key(&(n.to_string(), d.id.clone()))).map(|d| d.id).collect() };
    let same = |core: &Core| {
        let all = core.tickets.due_all(&names);
        for n in &names {
            assert_eq!(ids(core, n), fresh(core, n), "{n}");
            assert_eq!(all[n].iter().map(|d| d.id.clone()).collect::<Vec<_>>(), fresh(core, n), "{n}");
        }
    };
    same(&core);
    let before = core.tickets.m.changes();
    // the owner's thread to w2: due to w2 at once, nothing to w3
    let r = core.ticket_input(Input::Open { kind: "thread".into(), under: None, opener: "owner".into(), form: json!({"to": "w2", "typed": [], "names": ["w2"], "text": "w2, look", "addressed": {"head": "Named"}}), text: Some(Text::New("w2, look".into())), names: vec!["w2".into()], told: vec![] });
    let t = match r {
        Ok(r) => docket_machine::parse_ticket(r.trim_start_matches("Opened ").trim_end_matches('.')).unwrap(),
        Err(e) => panic!("{e}"),
    };
    assert!(core.tickets.m.changes() > before);
    assert_eq!(ids(&core, "w2").len(), 1, "{:?}", ids(&core, "w2"));
    assert!(ids(&core, "w3").is_empty());
    same(&core);
    // asked again with nothing changed: the same, from what was kept
    let kept = core.tickets.m.changes();
    same(&core);
    assert_eq!(core.tickets.m.changes(), kept);
    // a post naming w3 moves the machine: w3's is there at the next read (its words say @w3: with w2 on the thread, words
    // naming nobody go to the watcher's route, base.kind; w2 was left off it while a name like a ticket's number read as
    // a ticket, docket_machine::ERA_NAMES)
    core.ticket_input(Input::Post { t, by: "owner".into(), text: Text::New("@w3 too".into()), names: vec!["w3".into()], told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    assert_eq!(ids(&core, "w3").len(), 1);
    same(&core);
    // handed and taken: gone from what is due, kept or not
    let d = core.tickets.due("w2").remove(0);
    core.tickets.handed.insert(("w2".into(), d.id.clone()), (d.t, d.upto, Default::default()));
    assert!(ids(&core, "w2").is_empty());
    same(&core);
    core.tickets.handed.clear();
    core.ticket_input(Input::Given { t: d.t, to: "w2".into(), delivery: d.id.clone(), upto: d.upto, skipped: Default::default(), place: None }).unwrap();
    assert!(ids(&core, "w2").is_empty());
    same(&core);
    // a post put in by hand, as a test may (no input): the write goes through `edit`, which counts it, so it is read
    // afresh; a write that does not go through it does not compile (`docket_machine::Counted`)
    let kept = core.tickets.m.changes();
    let x = core.tickets.m.tickets.edit().get_mut(&t).unwrap();
    let mut p = x.posts.last().unwrap().clone();
    p.n += 1;
    p.names = vec!["w2".into()];
    p.text = Text::New("by hand".into());
    x.posts.push(p);
    assert!(core.tickets.m.changes() > kept);
    same(&core);
    // so does the scope of slow, set by hand
    let kept = core.tickets.m.changes();
    core.tickets.m.slowed.edit().insert("N-1".into());
    assert!(core.tickets.m.changes() > kept);
    same(&core);
    let _ = std::fs::remove_dir_all(&coord);
}
