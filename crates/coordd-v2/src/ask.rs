//! `coordd-v2 ask SOCKET NAME ARG...`: Act NAME(ARG...) asked for once on the daemon's SOCKET, as the party the
//! table vouches this process's cgroup for, and the end of its run told: its value as plain JSON, or why it did not
//! end with one. A session's own commands use it (`coord read-brain`, `coord report-work`, and the MCP tools
//! `read_brain` and `report_work`): unlike `act` it holds no run and asks once. It waits for the run's end at most
//! the time given; a run that rests on a driver's answer longer is told as still running, and the daemon goes on
//! with it.

use serde_json::{json, Map, Value as Json};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

/// How one ask came out.
#[derive(Debug, PartialEq)]
pub enum Asked {
    /// The run ended with this value, written plain.
    Ended(Json),
    /// The Act was refused, and why: `source`, `policy` or `closes`, with the policy's words, or the daemon's own (no such
    /// Act or Event, a field of the wrong type).
    Refused(String),
    /// The run raised an Effect nobody answered (an Act of its body refused, a driver's `Fault`) or was cancelled:
    /// the line as the record has it.
    Failed(String),
    /// The daemon did not take the request, and why: no socket, nothing vouches for the caller.
    NotTaken(String),
    /// The run, so named, had not ended within the time given.
    Running(String),
}

/// A value of the record (`codec::of_value`) written plain, for a model or a script to read: a record as an object
/// of its fields, a tuple as an array; a constructor and an instance stay as the record writes them.
pub fn plain(j: &Json) -> Json {
    match j {
        Json::Array(xs) => Json::Array(xs.iter().map(plain).collect()),
        Json::Object(o) if o.contains_key("record") => {
            let mut out = Map::new();
            for f in o.get("fields").and_then(Json::as_array).into_iter().flatten() {
                if let Some([Json::String(k), v]) = f.as_array().map(Vec::as_slice) {
                    out.insert(k.clone(), plain(v));
                }
            }
            Json::Object(out)
        }
        Json::Object(o) if o.len() == 1 && o.contains_key("tuple") => plain(&o["tuple"]),
        Json::Object(o) => Json::Object(o.iter().map(|(k, v)| (k.clone(), plain(v))).collect()),
        other => other.clone(),
    }
}

/// The next reply on `conn`, by `until`.
fn next(conn: &mut BufReader<UnixStream>, until: Instant) -> Result<Json, String> {
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err("no reply in time".into());
    }
    conn.get_ref().set_read_timeout(Some(left)).map_err(|e| e.to_string())?;
    let mut l = String::new();
    match conn.read_line(&mut l) {
        Ok(0) => Err("the daemon closed the connection".into()),
        Ok(_) => serde_json::from_str(&l).map_err(|e| format!("a reply that is not JSON: {e}")),
        Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => Err("no reply in time".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// The value of a request's `done`, or why it was not taken; the record lines before it, with their places, go to
/// `lines`.
fn done(conn: &mut BufReader<UnixStream>, until: Instant, lines: &mut Vec<(u64, Json)>) -> Result<Json, String> {
    loop {
        let r = next(conn, until)?;
        match r["is"].as_str() {
            Some("line") => lines.push((r["at"].as_u64().unwrap_or_default(), r["line"].clone())),
            Some("done") => return Ok(r["value"].clone()),
            _ => return Err(r["why"].as_str().map_or_else(|| r.to_string(), str::to_string)),
        }
    }
}

/// Why a `refused` line's act was refused, in words.
fn why(w: &Json) -> String {
    match w {
        Json::String(s) => s.clone(),
        Json::Object(o) => o.iter().map(|(k, v)| format!("{k}: {}", v.as_str().map_or_else(|| v.to_string(), str::to_string))).collect::<Vec<_>>().join("; "),
        other => other.to_string(),
    }
}

/// Asks for NAME(ARGS) on `socket` once, and waits at most `wait` for the end of its run.
pub fn ask(socket: &Path, name: &str, args: &[String], wait: Duration) -> Asked {
    let until = Instant::now() + wait;
    let s = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(e) => return Asked::NotTaken(format!("{}: {e}", socket.display())),
    };
    let mut w = match s.try_clone() {
        Ok(w) => w,
        Err(e) => return Asked::NotTaken(e.to_string()),
    };
    let mut conn = BufReader::new(s);
    let mut send = |req: Json| w.write_all(format!("{req}\n").as_bytes()).map_err(|e| e.to_string());
    let mut lines = Vec::new();
    // where the record is now: the run's lines all come after it
    let asked = send(json!({"do": "older", "before": u64::MAX, "n": 1}))
        .and_then(|()| done(&mut conn, until, &mut lines))
        .and_then(|_| send(json!({"do": "act", "name": name, "args": args})))
        .and_then(|()| done(&mut conn, until, &mut Vec::new()));
    let started = match asked {
        Ok(v) => v,
        Err(why) => return Asked::NotTaken(why),
    };
    let Some(run) = started["run"].as_str().map(str::to_string) else { return Asked::NotTaken(format!("no run in {started}")) };
    let from = lines.last().map_or(0, |(at, _)| at + 1);
    let mut seen = Vec::new();
    if let Err(why) = send(json!({"do": "watch", "from": from})).and_then(|()| done(&mut conn, until, &mut seen)) {
        return Asked::NotTaken(why);
    }
    let mut refused = None;
    let mut k = 0;
    loop {
        while k < seen.len() {
            let l = &seen[k].1;
            k += 1;
            let ours = |kind: &str| l.get(kind).filter(|b| b["run"].as_str() == Some(run.as_str()));
            if let Some(b) = ours("refused").filter(|b| b["act"]["inst"].as_str() == Some(name)) {
                refused = Some(why(&b["why"]));
            } else if let Some(b) = ours("ended") {
                return Asked::Ended(plain(&b["value"]));
            } else if let Some(b) = ours("raised").filter(|b| b["effect"] == "Refused") {
                // the daemon refused the act itself (no such act, a field of the wrong type): its words
                let said = b["args"].get(1).and_then(Json::as_str).map(str::to_string);
                return Asked::Refused(refused.or(said).unwrap_or_else(|| l.to_string()));
            } else if ours("raised").is_some() || ours("cancelled").is_some() {
                return refused.map_or_else(|| Asked::Failed(l.to_string()), Asked::Refused);
            }
        }
        match next(&mut conn, until) {
            Ok(r) if r["is"] == "line" => seen.push((r["at"].as_u64().unwrap_or_default(), r["line"].clone())),
            Ok(_) => {}
            Err(_) => return Asked::Running(run),
        }
    }
}

/// The words `coordd-v2 ask` prints for `a`, and its exit code: on success the plain value alone, on stdout.
pub fn told(a: &Asked) -> (String, i32) {
    match a {
        Asked::Ended(v) => (v.to_string(), 0),
        Asked::NotTaken(why) => (format!("not taken: {why}"), 2),
        Asked::Refused(why) => (format!("refused: {why}"), 3),
        Asked::Failed(l) => (format!("failed: {l}"), 4),
        Asked::Running(run) => (format!("still running: run {run}; the daemon goes on with it"), 5),
    }
}
