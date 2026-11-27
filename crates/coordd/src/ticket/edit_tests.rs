//! UI editing's two engine parts (paper-research N-101 post 145, 8; docket-edge's proposal on N-101, 15 Nov): the
//! owner's once-edit of a call's template, checked in the call's slots and sent as the step's prompt; and the reload
//! of kinds and calls, taken only when all pass and nothing the log stands on moved.

use super::Core;
use crate::core::Who;
use docket_machine::{Input, Line};
use serde_json::json;

const CALL: &str = "call ask(q: Text) -> Said(text: Text)\n  task t\n  template\n    Q: {{ q }}\n";
pub(super) const KIND: &str = "kind e\n  letter E\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    b <- step w2 ask(q);\n    return q\n";

/// A coord with a project of KIND and CALL, w2 registered; the core loaded, and its folder.
pub(super) fn core(tag: &str) -> (Core, std::path::PathBuf) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("docket-edit-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    let project = coord.join("files");
    std::fs::create_dir_all(project.join("kinds")).unwrap();
    std::fs::create_dir_all(project.join("calls")).unwrap();
    std::fs::write(project.join("calls/a.call"), CALL).unwrap();
    std::fs::write(project.join("kinds/e.kind"), KIND).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"e\"\n").unwrap();
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"w2\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let mut core = Core::load(&coord.display().to_string(), &base);
    core.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
    core.load_tickets();
    assert!(core.tickets.errors.is_empty(), "{:?}", core.tickets.errors);
    (core, coord)
}

/// The prompt w2 would be sent for what is due to it on ticket T.
pub(super) fn prompt(core: &Core, t: u64) -> String {
    let d = core.ticket_due("w2").into_iter().find(|d| d.t == t).expect("a step is due");
    let v = core.tickets.m.view(t).unwrap();
    crate::render::delivery(&core.places(), &d, &v, true, true, false, false, None).unwrap().0
}

pub(super) fn log(coord: &std::path::Path) -> Vec<Line> {
    std::fs::read_to_string(coord.join("tickets/log.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

#[test]
fn the_owners_template_is_checked_in_the_calls_slots_and_sent_as_the_steps_prompt() {
    let (mut core, coord) = core("template");
    core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": "why so"}), text: None, names: vec![], told: vec![] }).unwrap();
    let t = core.tickets.m.tickets.values().find(|x| x.kind == "e").map(|x| x.t).unwrap();
    let id = core.tickets.m.id(t);
    assert!(prompt(&core, t).contains("Q: why so"), "the call's own template first");
    // a slot the call does not have is refused here, before the machine sees it; an agent may not edit at all
    let r = core.ticket_template(&Who::Owner, &id, "ask".into(), "step".into(), "Say {{ nosuch }}".into(), "w".into());
    assert!(!r.ok && r.out.contains("refused"), "{}", r.out);
    let r = core.ticket_template(&Who::Agent("w2".into()), &id, "ask".into(), "step".into(), "Say {{ q }}".into(), "w".into());
    assert!(!r.ok);
    assert!(!log(&coord).iter().any(|l| matches!(l, Line::Template { .. })));
    // the owner's text, in the call's slots, is what goes, for this step only
    let r = core.ticket_template(&Who::Owner, &id, "ask".into(), "step".into(), "Edited: {{ q }}!".into(), "clearer".into());
    assert!(r.ok, "{}", r.out);
    let p = prompt(&core, t);
    assert!(p.contains("Edited: why so!") && !p.contains("Q: why so"), "{p}");
    assert!(log(&coord).iter().any(|l| matches!(l, Line::Template { text, by, why, .. } if text == "Edited: {{ q }}!" && by == "owner" && why == "clearer")));
    // a restart replays it: the step still carries the owner's text
    drop(core);
    let (core2, _) = (|| {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let mut c = Core::load(&coord.display().to_string(), &base);
        c.project = crate::project::Project::load(&coord.display().to_string()).unwrap();
        c.load_tickets();
        (c, ())
    })();
    assert!(core2.tickets.errors.is_empty(), "{:?}", core2.tickets.errors);
    assert!(prompt(&core2, t).contains("Edited: why so!"));
    let mut core = core2;
    core.ticket_input(Input::Answer { t, by: "w2".into(), head: json!({"head": "Said", "text": "x"}), text: docket_machine::Text::New("x".into()) }).unwrap();
    assert!(prompt(&core, t).contains("Q: why so"), "the next step has the call's template again");
    let _ = std::fs::remove_dir_all(&coord);
}

#[test]
fn a_reload_takes_new_kinds_and_refuses_what_the_log_stands_on() {
    let (mut core, coord) = core("reload");
    let files = coord.join("files");
    core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": "q"}), text: None, names: vec![], told: vec![] }).unwrap();
    // a new kind and a call's changed template: taken, and recorded with what changed
    std::fs::write(files.join("kinds/f.kind"), "kind f\n  letter F\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    return q\n").unwrap();
    std::fs::write(files.join("calls/a.call"), CALL.replace("Q: {{ q }}", "Question: {{ q }}")).unwrap();
    assert!(!core.ticket_reload(&Who::Agent("w2".into()), None, "w".into()).ok, "the owner's only");
    let r = core.ticket_reload(&Who::Owner, Some("abc123".into()), "new kind f".into());
    assert!(r.ok, "{}", r.out);
    assert!(r.out.contains("kinds f") && r.out.contains("calls ask"), "{}", r.out);
    assert!(core.tickets.m.defs.kinds.contains_key("f"));
    assert!(log(&coord).iter().any(|l| matches!(l, Line::Reload { commit: Some(c), kinds, calls, .. } if c == "abc123" && *kinds == vec!["f".to_string()] && *calls == vec!["ask".to_string()])));
    let r = core.ticket_open(&Who::Owner, "f".into(), None, json!({"q": "x"}), None, Default::default());
    assert!(r.ok, "a ticket of the new kind opens: {}", r.out);
    // refused, the old ones kept: a call's parameters changed (the tickets recorded it, so what refuses it is the kinds
    // that no longer check against it: live edits, N-101 p424), a kind gone, a kind's letter changed, a bad kind
    for (file, text, why) in [
        ("calls/a.call", CALL.replace("ask(q: Text)", "ask(q: Text, r: Text)"), "call ask takes 2"),
        ("kinds/f.kind", String::new(), "the kind f is gone"),
        ("kinds/f.kind", "kind f\n  letter G\n  form q: Text\n  result Text\n  pattern\n    a <- step w2 ask(q);\n    return q\n".into(), "letter changed"),
        ("kinds/g.kind", "kind g\n  pattern\n    nonsense(\n".into(), "g.kind"),
    ] {
        let path = files.join(file);
        let before = std::fs::read_to_string(&path).ok();
        if text.is_empty() {
            std::fs::remove_file(&path).unwrap();
        } else {
            std::fs::write(&path, &text).unwrap();
        }
        let r = core.ticket_reload(&Who::Owner, None, "w".into());
        assert!(!r.ok && r.out.contains(why) && r.out.contains("nothing is read again"), "{why}: {}", r.out);
        assert!(core.tickets.m.defs.kinds.contains_key("f") && !core.tickets.m.defs.kinds.contains_key("g"));
        match before {
            Some(b) => std::fs::write(&path, b).unwrap(),
            None => std::fs::remove_file(&path).unwrap(),
        }
    }
    assert_eq!(log(&coord).iter().filter(|l| matches!(l, Line::Reload { .. })).count(), 1, "a refused reload writes nothing");
    let _ = std::fs::remove_dir_all(&coord);
}

/// The owner's dry run of a prompt (`Request::DryRun`, H2; his N-216.p1, docket-edge8 N-1446 p1824): a draft of the
/// call file renders the step a ticket waits on with its arguments, and once the draft is saved that step is sent
/// exactly so; with no ticket asked, the newest open ticket waiting on a step of the call fills it; his prompt for one
/// thread renders that thread's step alone; his own prompt for a thread stands over a draft of the program; with none
/// waiting the answer names no ticket. Only the owner, only a call file of a calls folder, and nothing is recorded.
#[test]
fn a_dry_run_shows_the_waiting_steps_prompt_as_it_is_then_sent() {
    let (mut core, coord) = core("dry");
    let open = |core: &mut Core, q: &str| -> u64 {
        core.ticket_input(Input::Open { kind: "e".into(), under: None, opener: "owner".into(), form: json!({"q": q}), text: None, names: vec![], told: vec![] }).unwrap();
        *core.tickets.m.tickets.range(..docket_machine::HIDDEN).next_back().unwrap().0
    };
    let (a, b) = (open(&mut core, "why so"), open(&mut core, "how so"));
    let (ida, idb) = (core.tickets.m.id(a), core.tickets.m.id(b));
    let file = coord.join("files/calls/a.call");
    let path = file.display().to_string();
    let draft = CALL.replace("    Q: {{ q }}\n", "    Q: {{ q }}\n    Answer in one line.\n");
    let lines = log(&coord).len();
    let dry = |core: &Core, on: Option<&str>, path: Option<&str>, text: &str| -> Result<coord_types::DryRunAnswer, String> {
        match core.ticket_dry_from(&Who::Owner, "ask", on, path, text) {
            Ok(d) => crate::render::dry_run(d),
            Err(r) => Err(r.out),
        }
    };
    // a draft of the program, from A's page: A's waiting step, with A's arguments
    let r = dry(&core, Some(&ida), Some(&path), &draft).unwrap();
    assert_eq!((r.on.as_deref(), r.step, r.by.as_str(), r.asked, r.own), (Some(ida.as_str()), 1, "w2", true, false), "{r:?}");
    assert!(r.text.starts_with("Q: why so\nAnswer in one line.\n") && r.text.contains("This step gives:\n  Said"), "{}", r.text);
    // from no ticket (the Docket, the Map): the newest open ticket waiting on a step of ask
    let r = dry(&core, None, Some(&path), &draft).unwrap();
    assert_eq!((r.on.as_deref(), r.asked), (Some(idb.as_str()), false), "{r:?}");
    assert!(r.text.starts_with("Q: how so\nAnswer in one line.\n"), "{}", r.text);
    // his prompt for B's thread alone: B's step, his text in the call's slots; never another ticket's
    let r = dry(&core, Some(&idb), None, "For B: {{ q }}").unwrap();
    assert!(r.asked && r.on.as_deref() == Some(idb.as_str()) && r.text.starts_with("For B: how so\n"), "{r:?}");
    // a prompt that would not be sent: why, in the renderer's words
    let e = dry(&core, Some(&ida), None, "Say {{ nosuch }}").unwrap_err();
    assert!(e.starts_with("the owner's prompt for ask: the template: "), "{e}");
    // nothing recorded, the file as it was, and A is still sent the call's own words
    assert_eq!(log(&coord).len(), lines);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), CALL);
    assert!(!prompt(&core, a).contains("Answer in one line."));
    // saved: what A is sent is what the dry run showed
    let shown = dry(&core, Some(&ida), Some(&path), &draft).unwrap().text;
    std::fs::write(&file, &draft).unwrap();
    assert!(core.ticket_reload(&Who::Owner, None, "one line".into()).ok);
    assert_eq!(prompt(&core, a), shown);
    // his own prompt for B's thread stands over a draft of the program, as it does when B's step is sent
    assert!(core.ticket_template(&Who::Owner, &idb, "ask".into(), "step".into(), "Mine: {{ q }}".into(), "mine".into()).ok);
    let r = dry(&core, Some(&idb), Some(&path), &draft.replace("Q:", "Question:")).unwrap();
    assert!(r.own && r.text.starts_with("Mine: how so\n"), "{r:?}");
    assert_eq!(r.text, prompt(&core, b));
    // past every step of ask: none waits, and the answer names no ticket
    for t in [a, b] {
        for _ in 0..2 {
            core.ticket_input(Input::Answer { t, by: "w2".into(), head: json!({"head": "Said", "text": "x"}), text: docket_machine::Text::New("x".into()) }).unwrap();
        }
    }
    assert_eq!(dry(&core, Some(&ida), Some(&path), &draft).unwrap(), coord_types::DryRunAnswer::default());
    assert_eq!(dry(&core, None, Some(&path), &draft).unwrap(), coord_types::DryRunAnswer::default());
    // refused: an agent, a kind's file, a file outside the calls folders
    assert!(core.ticket_dry_from(&Who::Agent("w2".into()), "ask", None, Some(&path), &draft).is_err());
    for p in [coord.join("files/kinds/e.kind"), coord.join("elsewhere/a.call")] {
        let e = core.ticket_dry_from(&Who::Owner, "ask", None, Some(&p.display().to_string()), &draft).err().expect("refused");
        assert!(e.out.contains("is no call file of a folder the calls are read from"), "{}", e.out);
    }
    let _ = std::fs::remove_dir_all(&coord);
}

/// The dry run's example as docket-edge8 posted it for him to read (N-1446 p1824), word for word: a made-up call, a
/// ticket waiting on its step with made-up arguments, and his line added to the call's template; through the agent's
/// own `step.tmpl` and `gives`. The call is `judge` here: the post's `check` is agent-base's runner check
/// (claim.call), which a project's call of other fields does not replace.
#[test]
fn the_dry_runs_posted_example_renders_word_for_word() {
    let (mut core, coord) = core("dry-example");
    let call = "call judge(change: Text, asked: Text) -> Fits | Misses(why: Text)\n  task review\n  template\n    Read this change and say whether it does what was asked.\n    Asked: {{ asked }}\n    The change:\n    {{ change }}\n";
    let file = coord.join("files/calls/review.call");
    std::fs::write(&file, call).unwrap();
    std::fs::write(coord.join("files/kinds/review.kind"), "kind review\n  letter V\n  form change: Text, asked: Text\n  result Text\n  pattern\n    v <- step w2 judge(change, asked);\n    return change\n").unwrap();
    let r = core.ticket_reload(&Who::Owner, None, "the example's call and kind".into());
    assert!(r.ok, "{}", r.out);
    let form = json!({"change": "edit.rs: the button reads \"Save for the program\".", "asked": "The Save button says what it saves."});
    core.ticket_input(Input::Open { kind: "review".into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
    let t = core.tickets.m.tickets.values().find(|x| x.kind == "review").map(|x| x.t).unwrap();
    let id = core.tickets.m.id(t);
    let draft = call.replace("    {{ change }}\n", "    {{ change }}\n    Answer Misses if anything asked is left out.\n");
    let dry = core.ticket_dry_from(&Who::Owner, "judge", Some(&id), Some(&file.display().to_string()), &draft).ok().expect("taken");
    let r = crate::render::dry_run(dry).unwrap();
    assert_eq!((r.on.as_deref(), r.step, r.asked, r.own), (Some(id.as_str()), 1, true, false), "{r:?}");
    let want = "Read this change and say whether it does what was asked.\nAsked: The Save button says what it saves.\nThe change:\nedit.rs: the button reads \"Save for the program\".\nAnswer Misses if anything asked is left out.\n\nThis step gives one of:\n  Fits\n  Misses\n    why";
    assert_eq!(r.text, want, "\n{}", r.text);
    let _ = std::fs::remove_dir_all(&coord);
}
