//! A Safe agent's read outside its folders through coordd's requests: the hook's call held on its card, his answer
//! letting it through or not, and an allowed folder kept in the project file for the next read and the next start.

use super::*;
use coord_types::ReadAnswer;

fn coord(tag: &str) -> (std::path::PathBuf, Shared) {
    let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
    let coord = std::env::temp_dir().join(format!("read-ask-flow-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&coord);
    for d in ["definition", "work", "notes/old", "elsewhere"] {
        std::fs::create_dir_all(coord.join(d)).unwrap();
    }
    for f in ["notes/old/paper.md", "notes/old/more.md", "elsewhere/x.md"] {
        std::fs::write(coord.join(f), "x").unwrap();
    }
    std::fs::write(coord.join("definition/project.toml"), "[project]\nhost_relay = true\n").unwrap();
    std::fs::write(coord.join("project"), format!("{}\n", coord.join("definition/project.toml").display())).unwrap();
    std::fs::write(coord.join("registry.jsonl"), "{\"ev\":\"add\",\"id\":\"AGT-001\",\"name\":\"clerk\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"m\",\"at\":\"x\"}\n").unwrap();
    let core = Core::load(&coord.display().to_string(), &base);
    (std::fs::canonicalize(&coord).unwrap(), Shared::new(core))
}

fn read(p: &std::path::Path) -> Request {
    let input = serde_json::json!({"hook_event_name": "PermissionRequest", "permission_mode": "default", "tool_name": "Read", "tool_input": {"file_path": p.display().to_string()}});
    Request::Hook { event: "permission".into(), input }
}

fn decision(r: &Reply) -> (String, String) {
    let v: Value = serde_json::from_str(&r.out).unwrap_or_else(|e| panic!("{e}: {:?}", r.out));
    let d = &v["hookSpecificOutput"]["decision"];
    (d["behavior"].as_str().unwrap_or("").into(), d["message"].as_str().unwrap_or("").into())
}

/// The read's call, sent as the agent's hook, once its card is up.
async fn asking(shared: &Shared, p: &std::path::Path) -> (tokio::task::JoinHandle<Reply>, u64) {
    let call = tokio::spawn({
        let (shared, req) = (shared.clone(), read(p));
        async move { handle(&shared, &Caller::Agent("clerk".into()), 0, req).await }
    });
    for _ in 0..400 {
        if let Some(k) = shared.lock().await.view("clerk").read_asks.into_iter().find(|k| k.path == p.display().to_string()) {
            return (call, k.id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("no card for {}", p.display());
}

fn answer(id: u64, a: ReadAnswer) -> Request {
    Request::ReadAnswer { name: "clerk".into(), id, answer: a }
}

#[tokio::test]
async fn a_read_waits_on_his_card_and_a_folder_he_allows_is_kept_for_the_next_read() {
    let (coord, shared) = coord("folder");
    let paper = coord.join("notes/old/paper.md");
    let (call, id) = asking(&shared, &paper).await;
    assert!(!call.is_finished(), "the call waits on him");
    assert_eq!(shared.lock().await.view("clerk").read_asks[0].folder, Some(coord.join("notes/old").display().to_string()));
    let by_agent = handle(&shared, &Caller::Agent("clerk".into()), 0, answer(id, ReadAnswer::Folder)).await;
    assert!(!by_agent.ok && !call.is_finished(), "his alone: {}", by_agent.out);
    let r = handle(&shared, &Caller::Owner, 0, answer(id, ReadAnswer::Folder)).await;
    assert!(r.ok, "{}", r.out);
    assert_eq!(decision(&call.await.unwrap()).0, "allow");
    assert!(shared.lock().await.view("clerk").read_asks.is_empty(), "the card goes");
    let file = std::fs::read_to_string(coord.join("definition/project.toml")).unwrap();
    assert!(file.contains("[read.clerk]") && file.contains(&coord.join("notes/old").display().to_string()), "{file}");
    // the next read in that folder goes through at once, with no card
    let r = handle(&shared, &Caller::Agent("clerk".into()), 0, read(&coord.join("notes/old/more.md"))).await;
    assert_eq!(decision(&r).0, "allow");
    let _ = std::fs::remove_dir_all(&coord);
}

#[tokio::test]
async fn once_lets_the_one_call_through_and_no_is_said_to_it() {
    let (coord, shared) = coord("once");
    let x = coord.join("elsewhere/x.md");
    let (call, id) = asking(&shared, &x).await;
    assert!(handle(&shared, &Caller::Owner, 0, answer(id, ReadAnswer::Once)).await.ok);
    assert_eq!(decision(&call.await.unwrap()).0, "allow");
    assert!(!std::fs::read_to_string(coord.join("definition/project.toml")).unwrap().contains("[read"), "once is kept nowhere");
    let (call, id) = asking(&shared, &x).await;
    assert!(handle(&shared, &Caller::Owner, 0, answer(id, ReadAnswer::Deny)).await.ok);
    let (behavior, why) = decision(&call.await.unwrap());
    assert!(behavior == "deny" && why.contains("said no"), "{why}");
    let r = handle(&shared, &Caller::Agent("clerk".into()), 0, read(&x)).await;
    assert!(decision(&r).1.contains("said no") && shared.lock().await.view("clerk").read_asks.is_empty(), "no card again");
    let _ = std::fs::remove_dir_all(&coord);
}

#[tokio::test]
async fn a_tool_that_does_not_read_is_left_to_the_hooks_own_refusal() {
    let (coord, shared) = coord("pass");
    let input = serde_json::json!({"permission_mode": "default", "tool_name": "Write", "tool_input": {"file_path": coord.join("elsewhere/x.md").display().to_string()}});
    let r = handle(&shared, &Caller::Agent("clerk".into()), 0, Request::Hook { event: "permission".into(), input }).await;
    assert!(r.ok && r.out.is_empty(), "{}", r.out);
    let _ = std::fs::remove_dir_all(&coord);
}
