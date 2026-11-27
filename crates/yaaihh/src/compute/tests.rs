use super::*;
use crate::App;
use rune::runtime::Runtime;
use serde_json::json;

fn settle(rt: &mut Runtime<App>) {
    rt.invalidate();
    for _ in 0..3 {
        rt.frame(0.016);
    }
}

fn texts(rt: &Runtime<App>) -> Vec<String> {
    rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
}

/// The devices test's reading with two made-up workers on the build machine and one gone on the second.
fn devices_answer() -> String {
    let mut a: serde_json::Value = serde_json::from_str(&crate::devices::tests::answer()).unwrap();
    let mut t: serde_json::Value = serde_json::from_str(a["text"].as_str().unwrap()).unwrap();
    t["devices"][1]["workers"] = json!([{"name": "w1", "state": "busy", "sandbox": "bwrap", "running": 2}, {"name": "w2", "state": "idle", "sandbox": "podman", "running": 0}]);
    t["devices"][2]["workers"] = json!([{"name": "w3", "state": "offline", "sandbox": "bwrap", "running": 1}, "not a worker"]);
    a["text"] = json!(t.to_string());
    a.to_string()
}

/// The store test's rows and the compute cache's row, made-up figures.
fn store_answer() -> String {
    let mut a: serde_json::Value = serde_json::from_str(&crate::store::tests::answer()).unwrap();
    a["rows"].as_array_mut().unwrap().push(json!({"id": "cas", "what": "Compute cache", "device": "yaaihh-build-1", "held_bytes": 1_288_490_189u64,
        "cap_bytes": 10_737_418_240u64, "count": 3412, "count_of": "blobs", "today": {"jobs": 40, "compiles": 40, "from_store": 25}}));
    a.to_string()
}

#[test]
fn a_workers_words_say_its_state_its_sandbox_and_what_it_runs() {
    let w = |v: serde_json::Value| serde_json::from_value::<Worker>(v).unwrap();
    assert_eq!(words(&w(json!({"name": "w1", "state": "busy", "sandbox": "bwrap", "running": 2}))), "compute w1 · busy · bwrap · 2 running");
    assert_eq!(words(&w(json!({"name": "w2", "state": "idle", "sandbox": "podman", "running": 0}))), "compute w2 · idle · podman");
    assert_eq!(words(&w(json!({"name": "w3", "state": "offline", "running": 1}))), "compute w3 · offline", "an offline worker runs nothing");
    assert_eq!(words(&w(json!({"state": 4}))), "compute worker", "a field of another type left out");
    let ws: Workers = serde_json::from_value(json!([{"name": "a"}, 7])).unwrap();
    assert_eq!(ws.0.len(), 1, "an item that does not read left out");
    assert_eq!(serde_json::from_value::<Workers>(json!("x")).unwrap(), Workers::default());
}

/// Home (N-1838): each worker on its machine under its host, offline quieter; the compute cache a Store row like the others.
#[test]
fn home_shows_each_worker_on_its_machines_card_and_the_cache_as_a_store_row() {
    let (_held, mut s) = crate::home::tests::made();
    s.sel = crate::Sel::View(crate::global::ID.into());
    s.dark = Some(true);
    crate::devices::came(&mut s, Ok(devices_answer()));
    crate::store::came(&mut s, Ok(store_answer()));
    let mut rt = crate::view_cost::window(s);
    settle(&mut rt);
    crate::home::tests::keyed(&mut rt, "host-more-Tower");
    let t = texts(&rt);
    assert!(t.iter().any(|x| x == "no runner jobs") && !t.iter().any(|x| x == "no jobs"), "beside a worker the runner's jobs are its own: {t:?}");
    for said in ["compute w1 · busy · bwrap · 2 running", "compute w2 · idle · podman", "compute w3 · offline · bwrap", "Compute cache", "1.2 GB of 10.0 GB", " · 3,412 blobs · today 40 jobs, 63% from the store"] {
        assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
    }
    let els = rt.elements();
    let card = |name: &str| els.iter().find(|e| e.key.as_deref() == Some(&format!("device-{name}"))).map(|e| e.rect).unwrap();
    let inside = |key: &str, r: [f32; 4]| els.iter().find(|e| e.key.as_deref() == Some(key)).is_some_and(|e| e.rect[0] >= r[0] && e.rect[1] >= r[1] && e.rect[0] + e.rect[2] <= r[0] + r[2] + 0.5 && e.rect[1] + e.rect[3] <= r[1] + r[3] + 0.5);
    assert!(inside("device-worker-yaaihh-build-1-0", card("yaaihh-build-1")) && inside("device-worker-yaaihh-build-1-1", card("yaaihh-build-1")), "on the build machine's card");
    assert!(inside("device-worker-yaaihh-build-2-0", card("yaaihh-build-2")), "on the second machine's card");
    let color = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.text_color);
    let k = rune_kit::Kit::neutral(true);
    assert_eq!((color("device-worker-yaaihh-build-1-0"), color("device-worker-yaaihh-build-2-0")), (Some(k.c.text_muted), Some(k.c.text_faint)));
    assert!(els.iter().any(|e| e.key.as_deref() == Some("store-bar-cas")), "the cache's bar, as the other rows'");
}

/// Home with the compute workers and the cache row, made-up figures, 1400 wide:
/// `STILL_OUT=DIR … compute::tests::compute_still --ignored --exact`.
#[test]
#[ignore = "a still: STILL_OUT"]
fn compute_still() {
    let Ok(out) = std::env::var("STILL_OUT") else { return };
    let out = std::path::PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    fn me() -> Option<crate::devices::Device> {
        Some(crate::devices::tests::me())
    }
    fn init() -> App {
        let (_held, mut s) = crate::home::tests::made();
        s.sel = crate::Sel::View(crate::global::ID.into());
        s.devices.open.insert("Tower".into());
        // COMPUTE_ANSWERS: coordd's Machines and Store answers given live ({"devices", "store"}), else made-up ones
        let live: Option<serde_json::Value> = std::env::var("COMPUTE_ANSWERS").ok().and_then(|f| serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok());
        match live {
            Some(v) => {
                crate::devices::came(&mut s, Ok(v["devices"].to_string()));
                crate::store::came(&mut s, Ok(v["store"].to_string()));
            }
            None => {
                crate::devices::came(&mut s, Ok(devices_answer()));
                crate::store::came(&mut s, Ok(store_answer()));
            }
        }
        s.devices.reader = me;
        s.devices.this_pc = me();
        s.notice = None;
        s
    }
    let args = vec!["--snap".to_string(), out.join(std::env::var("STILL_NAME").unwrap_or_else(|_| "home-compute.png".into())).display().to_string(), "--size".into(), "1400x1100".into(), "--at".into(), "1".into()];
    rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 1100.0), init, crate::view) }).args(args).try_run().unwrap();
}
