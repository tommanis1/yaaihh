//! docket-core3 (N-101): a project on a library, `library = "PATH"` in its file; kinds and calls read from agent-base,
//! the library and the project, in that order, a flat folder as well as one with kinds/ and calls/.

use crate::ticket::Tickets;

/// A coord and a project in a fresh folder: the project file names LIBRARY; the project's files are FILES (name,
/// text), flat. Gives (coord, project folder, the folder to remove).
fn scratch(tag: &str, library: &str, files: &[(&str, &str)]) -> (String, String, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("docket-library-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (coord, project) = (root.join("coord"), root.join("project"));
    std::fs::create_dir_all(&coord).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("project.toml"), format!("[project]\nlibrary = {library:?}\n")).unwrap();
    for (n, t) in files {
        std::fs::write(project.join(n), t).unwrap();
    }
    std::fs::write(coord.join("project"), project.join("project.toml").display().to_string()).unwrap();
    (coord.display().to_string(), project.display().to_string(), root)
}

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

/// The library's kind runs on the project's call that replaces the library's with more parts; the library's own call
/// that the project leaves is rendered from the library's folder.
#[test]
fn a_project_stands_on_its_library() {
    let lib = std::env::temp_dir().join(format!("docket-library-lib-{}", std::process::id()));
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("lib.call"), "call design(words: Text) -> Design(asked: Text, today: Text) | Ask(question: Text)\n  task design\n  template\n    Design this: {{ words }}\ncall review(d: Text) -> Fine | Not(why: Text)\n  task review\n  template\n    Review: {{ d }}\ncall assemble(d: Text) -> Text\n  blind\n  task assemble\n  template\n    Assemble: {{ d }}\n").unwrap();
    std::fs::write(lib.join("lib.kind"), "kind change\n  form words: Text\n  result Text\n  pattern\n    d <- step fresh design(words);\n    match d {\n      Design(asked, today) => return today\n      Ask(q) => return q\n    }\n").unwrap();
    // the call that stands says the model of the fresh agent its step makes (the model classes, models.rs)
    let own = "call design(words: Text) where model >= medium -> Design(asked: Text, today: Text, meaning: Text) | Ask(question: Text)\n  task design\n  template\n    Our design: {{ words }}\n";
    let (coord, project, root) = scratch("stands", &lib.display().to_string(), &[("own.call", own)]);
    let t = Tickets::load(&coord, &base(), Some(&project), 1);
    assert!(t.errors.is_empty(), "{:#?}", t.errors);
    assert!(t.m.defs.kinds.contains_key("change"));
    assert!(t.m.defs.calls["design"].head.to_string().contains("meaning"));
    let places = crate::policy::Places { base: base(), project: Some(project.clone()), coord: coord.clone(), ..Default::default() };
    let calls = crate::policy::call::Calls::load(&places.call_dirs(), &places.overlay).unwrap();
    assert_eq!(calls.render_step("review", &serde_json::json!({"d": "x"})).as_deref().map(str::trim), Some("Review: x"));
    assert_eq!(calls.render_step("design", &serde_json::json!({"words": "w"})).as_deref().map(str::trim), Some("Our design: w"));
    // a step's call in the policy's types too (`-> Text`): rendered all the same
    assert_eq!(calls.render_step("assemble", &serde_json::json!({"d": "x"})).as_deref().map(str::trim), Some("Assemble: x"));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&lib);
}

/// The library and a project on it (its policy/ with it) load with agent-base's, every failing line printed, nothing refused
/// (`DOCKET_LIBRARY=DIR DOCKET_PROJECT=DIR cargo test -p coordd loads_with_a_library -- --ignored --nocapture`; each
/// folder flat or with kinds/ and calls/; tickets-patterns' plan/library and plan/tovrin-program/on-library).
#[test]
#[ignore]
fn loads_with_a_library() {
    let library = std::env::var("DOCKET_LIBRARY").expect("DOCKET_LIBRARY names the library's folder");
    let from = std::env::var("DOCKET_PROJECT").expect("DOCKET_PROJECT names the project's folder");
    let read = |d: &str| -> Vec<(String, String)> {
        let mut fs: Vec<_> = std::fs::read_dir(d).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "kind" || e == "call")).collect();
        fs.sort();
        fs.iter().map(|p| (p.file_name().unwrap().to_string_lossy().to_string(), std::fs::read_to_string(p).unwrap())).collect()
    };
    let mut files = read(&from);
    for sub in ["kinds", "calls"] {
        if std::path::Path::new(&format!("{from}/{sub}")).is_dir() {
            files.extend(read(&format!("{from}/{sub}")));
        }
    }
    let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let (coord, project, root) = scratch("load", &library, &files);
    // its policy too: a duties file lets a step raise Violated (ticket/violated.rs)
    if let Ok(ps) = std::fs::read_dir(format!("{from}/policy")) {
        std::fs::create_dir_all(format!("{project}/policy")).unwrap();
        for p in ps.flatten().map(|e| e.path()).filter(|p| p.is_file()) {
            std::fs::copy(&p, format!("{project}/policy/{}", p.file_name().unwrap().to_string_lossy())).unwrap();
        }
    }
    let t = Tickets::load(&coord, &base(), Some(&project), 1);
    let _ = std::fs::remove_dir_all(&root);
    let shown: Vec<String> = t.errors.iter().map(|e| e.replace(&format!("{project}/"), &format!("{from}/"))).collect();
    for e in &shown {
        println!("{e}");
    }
    println!("{} failing; kinds loaded: {:?}", shown.len(), t.m.defs.kinds.keys().collect::<Vec<_>>());
    // the lists whose entries name another list's elements (`Defs.names`), as the forming reads them
    for ((k, c), rs) in &t.m.defs.names {
        println!("names: {k} {c}: {rs:?}");
    }
    assert!(shown.is_empty(), "{} lines fail", shown.len());
}

/// What a project's policy decides (N-159: the draft's cut-build): DOCKET_PROJECT's policy/ asked decision
/// DOCKET_DECIDE once per case of DOCKET_CASES, cases split by `;`, each `FIELD=VALUE,...` (empty: no fields), as
/// coordd's clock asks a `when` binding (machine.rs when_asks). Prints each case and the option chosen, none when the
/// rules leave none or several.
/// (`DOCKET_PROJECT=DIR DOCKET_DECIDE=cut-build DOCKET_CASES='open=0,closed-piece=1;open=1' cargo test -p coordd
/// decides_with_the_projects_policy -- --ignored --nocapture`)
#[test]
#[ignore]
fn decides_with_the_projects_policy() {
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    let from = std::env::var("DOCKET_PROJECT").expect("DOCKET_PROJECT names the project's folder");
    let decision = std::env::var("DOCKET_DECIDE").expect("DOCKET_DECIDE names the decision");
    let cases = std::env::var("DOCKET_CASES").unwrap_or_default();
    // a decision is made with the proxy's models: a stand-in that serves none, as ticket/mod.rs's test of `when`
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let body = r#"{"data": []}"#;
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
    let places = crate::policy::Places { base: base(), project: Some(from.clone()), coord: from.clone(), ..Default::default() };
    let engine = crate::policy::Engine::start(places);
    for case in cases.split(';') {
        let fields: BTreeMap<String, String> =
            case.split(',').filter(|f| !f.trim().is_empty()).map(|f| f.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).expect("FIELD=VALUE")).collect();
        match engine.decide(&decision, fields) {
            Ok(d) => println!("{case:<45} -> {} (optimal {:?}{})", d.chosen.as_deref().unwrap_or("none"), d.optimal, d.fault.as_ref().map(|f| format!(", fault {f}")).unwrap_or_default()),
            Err(e) => println!("{case:<45} -> error: {e}"),
        }
    }
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}
