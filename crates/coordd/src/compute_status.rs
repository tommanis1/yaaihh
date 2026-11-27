//! Home's compute figures: NativeLink's workers on their machine's device, its CAS as a Store row.
//! Named by `~/agent-coord/machines/compute.json` (`workers`, `cas`, `cap_bytes`, `machine`); without it nothing changes.

use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

/// The most bytes read of the file and of the workers' answer.
const AT_MOST: u64 = 1 << 20;

/// What the file names: the workers' listener (`http://127.0.0.1:PORT`), the CAS folder, its cap and its machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub port: Option<u16>,
    pub cas: Option<String>,
    pub cap_bytes: Option<u64>,
    pub machine: Option<String>,
}

/// FOLDER's `compute.json`, a plain file only; None when it is not there or does not read.
pub fn named(folder: &Path) -> Option<Status> {
    let p = folder.join("compute.json");
    std::fs::symlink_metadata(&p).ok().filter(|m| m.is_file() && m.len() <= AT_MOST)?;
    let v: Value = serde_json::from_str(&std::fs::read_to_string(&p).ok()?).ok()?;
    let port = v["workers"].as_str().and_then(|u| u.strip_prefix("http://127.0.0.1:")).and_then(|p| p.trim_end_matches('/').parse().ok());
    Some(Status { port, cas: v["cas"].as_str().map(str::to_string), cap_bytes: v["cap_bytes"].as_u64(), machine: v["machine"].as_str().map(str::to_string) })
}

/// The scheduler's worker list on 127.0.0.1:PORT (`GET /admin/scheduler/MAIN/workers`), within a second.
pub fn listed(port: u16) -> Result<Value, String> {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(1)).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(1))).map_err(|e| e.to_string())?;
    write!(s, "GET /admin/scheduler/MAIN/workers HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n").map_err(|e| e.to_string())?;
    let mut text = String::new();
    s.take(AT_MOST).read_to_string(&mut text).map_err(|e| e.to_string())?;
    let body = text.split_once("\r\n\r\n").map(|(_, b)| b).ok_or("no answer")?;
    serde_json::from_str(body).map_err(|e| e.to_string())
}

/// LIST's workers as Home says them: the name without its uuid, idle or busy (offline when paused or draining).
pub fn workers(list: &Value) -> Vec<Value> {
    let Some(ws) = list.as_array() else { return Vec::new() };
    ws.iter()
        .map(|w| {
            let id = w["id"].as_str().unwrap_or("");
            let name = if id.len() > 36 && id.is_char_boundary(id.len() - 36) { &id[..id.len() - 36] } else { id };
            let running = w["running_actions"].as_u64().unwrap_or(0);
            let off = w["is_paused"].as_bool() == Some(true) || w["is_draining"].as_bool() == Some(true);
            let state = if off { "offline" } else if running > 0 { "busy" } else { "idle" };
            json!({"name": name, "state": state, "sandbox": w["platform_properties"]["sandbox"], "running": running})
        })
        .collect()
}

/// The Machines answer ANSWER with WORKERS on the device S names (else the engine's own); its text is left as it was
/// when it does not read as devices.
pub fn on_devices(answer: Value, s: &Status, workers: Vec<Value>) -> Value {
    let Some(text) = answer["text"].as_str() else { return answer };
    let Ok(mut t) = serde_json::from_str::<Value>(text) else { return answer };
    let Some(devs) = t["devices"].as_array_mut() else { return answer };
    let at = devs.iter().position(|d| s.machine.is_some() && d["name"].as_str() == s.machine.as_deref()).or_else(|| devs.iter().position(|d| d["here"] == json!(true)));
    match at {
        Some(i) => devs[i]["workers"] = json!(workers),
        None => devs.push(json!({"name": s.machine.clone().unwrap_or_else(|| "compute".into()), "workers": workers})),
    }
    let mut a = answer;
    a["text"] = json!(t.to_string());
    a
}

/// The CAS folder's bytes and files, walked without following links.
pub fn held(dir: &Path) -> (u64, u64) {
    let (mut bytes, mut files, mut todo) = (0, 0, vec![dir.to_path_buf()]);
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => todo.push(e.path()),
                Ok(t) if t.is_file() => {
                    bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
                    files += 1;
                }
                _ => {}
            }
        }
    }
    (bytes, files)
}

/// The Store row of S's CAS: held against its cap, its blobs, on its machine.
pub fn cas_row(s: &Status) -> Option<Value> {
    let (bytes, blobs) = held(Path::new(s.cas.as_deref()?));
    Some(json!({"id": "cas", "what": "Compute cache", "device": s.machine, "held_bytes": bytes, "cap_bytes": s.cap_bytes, "count": blobs, "count_of": "blobs"}))
}

/// The Store answer's line LINE with S's CAS row after the runner's rows.
pub fn on_store(line: &str, s: &Status) -> String {
    let (Ok(mut v), Some(row)) = (serde_json::from_str::<Value>(line), cas_row(s)) else { return line.to_string() };
    match v["rows"].as_array_mut() {
        Some(rows) => rows.push(row),
        None => return line.to_string(),
    }
    v.to_string()
}

#[cfg(test)]
#[path = "compute_status_tests.rs"]
mod tests;
