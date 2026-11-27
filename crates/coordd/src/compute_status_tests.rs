use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("compute-status-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The worker list as NativeLink 1.9.0's workers' listener gives it (made-up ids).
fn list() -> Value {
    json!([
        {"id": "tower-bwrap1f1e9843-41be-69c9-af3b-95bf0f263c0e", "running_actions": 2, "is_paused": false, "is_draining": false, "platform_properties": {"sandbox": "bwrap"}},
        {"id": "tower-podman2a2e9843-41be-69c9-af3b-95bf0f263c0e", "running_actions": 0, "is_paused": false, "is_draining": false, "platform_properties": {"sandbox": "podman"}},
        {"id": "old", "running_actions": 1, "is_paused": true, "platform_properties": {}}
    ])
}

#[test]
fn workers_are_named_without_their_uuid_and_busy_idle_or_offline() {
    let w = workers(&list());
    assert_eq!(w[0], json!({"name": "tower-bwrap", "state": "busy", "sandbox": "bwrap", "running": 2}));
    assert_eq!(w[1], json!({"name": "tower-podman", "state": "idle", "sandbox": "podman", "running": 0}));
    assert_eq!((w[2]["name"].clone(), w[2]["state"].clone()), (json!("old"), json!("offline")));
    assert!(workers(&json!({"not": "a list"})).is_empty());
}

#[test]
fn the_file_names_a_loopback_listener_only_and_workers_go_on_its_machine() {
    let d = scratch("named");
    assert_eq!(named(&d), None, "no file, nothing changes");
    std::fs::write(d.join("compute.json"), r#"{"workers": "http://127.0.0.1:50061", "cas": "/x/cas", "cap_bytes": 10, "machine": "yaaihh-build-1"}"#).unwrap();
    let s = named(&d).unwrap();
    assert_eq!((s.port, s.cap_bytes, s.machine.as_deref()), (Some(50061), Some(10), Some("yaaihh-build-1")));
    std::fs::write(d.join("compute.json"), r#"{"workers": "http://10.0.0.5:50061"}"#).unwrap();
    assert_eq!(named(&d).unwrap().port, None, "only 127.0.0.1 is asked");
    let devices = json!({"devices": [{"name": "WSL", "here": true}, {"name": "yaaihh-build-1"}]}).to_string();
    let a = on_devices(json!({"text": devices, "age_secs": 3}), &s, workers(&list()));
    let t: Value = serde_json::from_str(a["text"].as_str().unwrap()).unwrap();
    assert_eq!((t["devices"][0].get("workers"), t["devices"][1]["workers"].as_array().map(Vec::len)), (None, Some(3)));
    assert_eq!(a["age_secs"], 3);
    let here = on_devices(json!({"text": devices}), &Status::default(), workers(&list()));
    assert!(serde_json::from_str::<Value>(here["text"].as_str().unwrap()).unwrap()["devices"][0]["workers"].is_array(), "no machine named: the engine's own");
    assert_eq!(on_devices(json!({"text": null, "why": "w"}), &s, vec![]), json!({"text": null, "why": "w"}));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_cas_is_one_more_store_row_with_its_bytes_and_blobs() {
    let d = scratch("cas");
    std::fs::create_dir_all(d.join("cas/content_path-cas")).unwrap();
    std::fs::write(d.join("cas/content_path-cas/a"), [0u8; 100]).unwrap();
    std::fs::write(d.join("cas/content_path-cas/b"), [0u8; 23]).unwrap();
    let s = Status { cas: Some(d.join("cas").display().to_string()), cap_bytes: Some(1000), machine: Some("WSL".into()), ..Status::default() };
    let line = on_store(r#"{"rows": [{"id": "crates"}]}"#, &s);
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["rows"][1], json!({"id": "cas", "what": "Compute cache", "device": "WSL", "held_bytes": 123, "cap_bytes": 1000, "count": 2, "count_of": "blobs"}));
    assert_eq!(on_store("not json", &s), "not json");
    let _ = std::fs::remove_dir_all(&d);
}

/// By hand against a live NativeLink: the Machines and Store answers coordd gives with COMPUTE_FOLDER's compute.json,
/// over the made-up answers DEVICES_IN and STORE_IN, written to COMPUTE_OUT as {"devices", "store"}.
#[test]
#[ignore]
fn answered_live() {
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let (Some(f), Some(dev), Some(store), Some(out)) = (var("COMPUTE_FOLDER"), var("DEVICES_IN"), var("STORE_IN"), var("COMPUTE_OUT")) else { return };
    let s = named(Path::new(&f)).expect("compute.json");
    let w = workers(&listed(s.port.expect("its listener")).expect("the worker list"));
    let devices = on_devices(serde_json::from_str(&std::fs::read_to_string(dev).unwrap()).unwrap(), &s, w);
    let store = on_store(&std::fs::read_to_string(store).unwrap(), &s);
    std::fs::write(out, json!({"devices": devices, "store": serde_json::from_str::<Value>(&store).unwrap()}).to_string()).unwrap();
}
