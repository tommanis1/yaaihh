//! The raw pane's source (owner m759, paper-research m760): what the model proxy sent to the model for an agent, from
//! the proxy's own request log. Read by coordd for `Request::RawPage` (N-802: the window on Windows cannot reach the
//! WSL machine's files), and by the window on Linux from a coordd older than that (N-40: coordd then only says where
//! it is, `Request::RawSource`). The proxy writes one file per request in its log folder
//! (`v1-messages-<time>-<id>.log`), in sections: `=== REQUEST INFO ===`, `=== HEADERS ===`, `=== REQUEST BODY ===`,
//! `=== API REQUEST 1 ===` (what went upstream: its headers, then `Body:`), `=== API RESPONSE 1 ===` (status, headers,
//! then the streamed events). The files are read where they are (no copy); the folder is capped and its oldest files
//! deleted, so a file may vanish between listing and reading, which only drops it. No other file of the folder is
//! opened.
//!
//! Which agent a request is for: the header `X-Claude-Code-Session-Id`, the agent's Claude Code session id (its
//! transcript's file name). Nothing of the headers is shown: they carry the account; only the session id is matched,
//! and the time, the body's model and the response's status are said. Of the body, `metadata.user_id` is left out
//! too: it names the account's user (paper-research N-802.p5).
//!
//! An opencode agent calls its providers itself, with no proxy in the way: its plugin (agent-base
//! `opencode/plugin/coord.js`) writes each of its model requests to a file of its own in the agent's coord folder
//! (`raw/call-<time>.log`), whole once the answer has ended, in the same sections with no headers at all: REQUEST
//! INFO says the time and `Session:` (opencode's session id), and the file ends with `=== END ===`. Its body and
//! reply are an OpenAI chat request and its stream (`choices`' deltas: OpenCode Go and opencode's other
//! OpenAI-compatible providers), read here beside the Messages API's.
//!
//! A request is shown as lines (`RawPage`): a head line, the system prompt, each tool, each message, and
//! the reply put together from the streamed events, each as sent. A page's offsets are the requests' times in
//! nanoseconds (REQUEST INFO's Timestamp), so a page reads on after the last request shown whatever was deleted.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;

/// A stretch of the proxy's log of a session (the raw pane, owner m750, m817): its lines, the offset of the first and
/// the offset after the last. A line longer than [`RAW_LINE_MAX`] is cut there, with how much was left out.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize, PartialEq)]
pub struct RawPage {
    pub lines: Vec<String>,
    pub start: u64,
    pub next: u64,
    /// The session the offsets are in (an agent's session changes when it resumes or compacts).
    pub session: String,
    /// The time of each request whose head line (`══ request …`) is in LINES, in their order: the pane's cut keeps
    /// whole requests and reads Earlier from the first one kept.
    #[serde(default)]
    pub heads: Vec<u64>,
    /// What its lines are read from when no gateway's log holds them, in his words, said over the pane: a session's own
    /// record (coordd's `record_page`, fix 5, accounts-lead N-1446 p2446). None for the proxy's log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
}

/// The longest line a [`RawPage`] carries whole: a pasted screenshot's line is ~0.4 MB of base64.
pub const RAW_LINE_MAX: usize = 16 * 1024;

/// The folder DIR, as a project file writes `proxy_log`, on this host with home HOME: `~/` is the app's own home (a
/// box's is another), an absolute path is itself; nothing else is read (N-40: no folder coordd reports is resolved).
pub fn host_dir(dir: &str, home: &str) -> Result<String, String> {
    match dir.strip_prefix("~/") {
        Some(rest) => Ok(format!("{}/{rest}", home.trim_end_matches('/'))),
        None if dir.starts_with('/') => Ok(dir.to_string()),
        None => Err("proxy_log must be ~/ or absolute: the app reads it on the host".into()),
    }
}

/// How many requests a page holds: each repeats the conversation so far, and a long session's runs to megabytes.
const PER_PAGE: usize = 3;
/// How much of a file's start holds its REQUEST INFO and HEADERS.
const HEAD: usize = 16 * 1024;

/// Each log file's request time and session id, as read once (a file's head does not change once written).
static HEADS: Mutex<Option<HashMap<String, (u64, String)>>> = Mutex::new(None);

/// A request time `2026-11-11T08:41:49.996213192+01:00` as nanoseconds since the epoch.
fn nanos(ts: &str) -> Option<u64> {
    let t = chrono::DateTime::parse_from_rfc3339(ts.trim()).ok()?;
    u64::try_from(t.timestamp_nanos_opt()?).ok()
}

/// The time and session id in the head of the log file at PATH; None while it holds neither yet, and None for a file
/// with no session id (a request from a client that is not Claude Code, his own included): it is no agent's, so no
/// session, not even an empty one, reads it (docket-core3, N-802.p13 point 3; paper-research p15).
fn head(path: &str) -> Option<(u64, String)> {
    if let Some(h) = HEADS.lock().unwrap().get_or_insert_with(HashMap::new).get(path) {
        // kept with an empty id: read once and found without one
        return Some(h.clone()).filter(|(_, s)| !s.is_empty());
    }
    let mut buf = vec![0u8; HEAD];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).ok()?;
    let text = String::from_utf8_lossy(&buf[..n]);
    let mut time = None;
    let mut session = None;
    for l in text.lines() {
        if let Some(t) = l.strip_prefix("Timestamp:").filter(|_| time.is_none()) {
            time = nanos(t);
        }
        // the proxy's header, or the `Session:` of an opencode agent's own file
        if let Some((k, v)) = l.split_once(':')
            && (k.trim().eq_ignore_ascii_case("x-claude-code-session-id") || k == "Session")
        {
            session = Some(v.trim().to_string());
        }
        if l.starts_with("=== REQUEST BODY") {
            break;
        }
    }
    let h = (time?, session.unwrap_or_default());
    HEADS.lock().unwrap().get_or_insert_with(HashMap::new).insert(path.to_string(), h.clone());
    Some(h).filter(|(_, s)| !s.is_empty())
}

/// The requests in DIR for SESSION, by time: (time, path). Files only: a link in the folder is not followed out of it
/// (docket-core3, N-802.p13 point 4).
fn requests(dir: &str, session: &str) -> Vec<(u64, String)> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_file())).filter_map(|e| e.file_name().into_string().ok()).filter(|n| (n.starts_with("v1-messages-") || n.starts_with("call-")) && n.ends_with(".log")).collect())
        .unwrap_or_default();
    names.sort();
    let mut out: Vec<(u64, String)> = names.into_iter().filter_map(|n| {
        let p = format!("{dir}/{n}");
        let (t, s) = head(&p)?;
        (s == session).then_some((t, p))
    }).collect();
    out.sort();
    // forget the heads of files the proxy deleted
    if let Some(m) = HEADS.lock().unwrap().as_mut()
        && m.len() > 20_000
    {
        m.retain(|p, _| std::path::Path::new(p).exists());
    }
    out
}

/// Whether the file at PATH is written to its end: its stream stopped, an opencode agent's file is there (written
/// whole, `=== END ===`), or nothing was added for a few seconds.
fn finished(path: &str) -> bool {
    let Ok(meta) = std::fs::metadata(path) else { return false };
    if meta.modified().ok().and_then(|m| m.elapsed().ok()).is_some_and(|e| e.as_secs() >= 5) {
        return true;
    }
    let Ok(mut f) = std::fs::File::open(path) else { return false };
    use std::io::{Seek, SeekFrom};
    let len = meta.len();
    let _ = f.seek(SeekFrom::Start(len.saturating_sub(512)));
    let mut tail = String::new();
    let _ = f.read_to_string(&mut tail);
    tail.contains("message_stop") || tail.contains("=== END ===")
}

/// Which of a session's requests a page holds, TIMES being theirs in order and FINISHED whether request I is written
/// to its end: the PER_PAGE from time AFTER on, or the PER_PAGE before time BEFORE, or the last PER_PAGE, and only
/// finished ones, so that none is shown half; with the page's start and next offsets. The proxy's log's ([`page`]),
/// and a session's own record's (coordd's `record_page`, fix 5).
pub fn choose(times: &[u64], finished: impl Fn(usize) -> bool, before: Option<u64>, after: Option<u64>) -> (Vec<usize>, u64, u64) {
    let chosen: Vec<usize> = match (before, after) {
        (_, Some(a)) => (0..times.len()).filter(|&i| times[i] >= a).take_while(|&i| finished(i)).take(PER_PAGE).collect(),
        (b, None) => {
            let done: Vec<usize> = (0..times.len()).filter(|&i| b.is_none_or(|b| times[i] < b)).take_while(|&i| finished(i)).collect();
            done[done.len().saturating_sub(PER_PAGE)..].to_vec()
        }
    };
    let (first, last) = (chosen.first().map(|&i| times[i]), chosen.last().map(|&i| times[i]));
    let (start, next) = match (before, after) {
        (_, Some(a)) => (a, last.map_or(a, |t| t + 1)),
        (Some(b), None) => (first.unwrap_or(b), b),
        (None, None) => (first.unwrap_or(0), last.map_or(0, |t| t + 1)),
    };
    (chosen, start, next)
}

/// A page of SESSION's requests in DIR as lines: the ones [`choose`] picks.
pub fn page(dir: &str, session: &str, before: Option<u64>, after: Option<u64>) -> RawPage {
    let all = requests(dir, session);
    let times: Vec<u64> = all.iter().map(|(t, _)| *t).collect();
    let (chosen, start, next) = choose(&times, |i| finished(&all[i].1), before, after);
    let (lines, heads) = read_requests(chosen.iter().map(|&i| &all[i]));
    RawPage { lines, start, next, session: format!("session {session}"), heads, about: None }
}

/// The most requests of one turn read at once (turn.rs): a long turn's last ones.
const TURN_MAX: usize = 40;

/// Which of a session's requests (TIMES, FINISHED as for [`choose`]) one turn's page holds: the finished ones from time
/// SINCE to before UNTIL, at most the last [`TURN_MAX`]; with the page's start and next offsets.
pub fn choose_turn(times: &[u64], finished: impl Fn(usize) -> bool, since: u64, until: u64) -> (Vec<usize>, u64, u64) {
    let chosen: Vec<usize> = (0..times.len()).filter(|&i| times[i] >= since && times[i] < until).take_while(|&i| finished(i)).collect();
    let chosen = chosen[chosen.len().saturating_sub(TURN_MAX)..].to_vec();
    let start = chosen.first().map_or(since, |&i| times[i]);
    (chosen, start, until)
}

/// SESSION's finished requests in DIR from time SINCE to before UNTIL (one turn's, turn.rs): [`choose_turn`]'s.
pub fn turn_page(dir: &str, session: &str, since: u64, until: u64) -> RawPage {
    let all = requests(dir, session);
    let times: Vec<u64> = all.iter().map(|(t, _)| *t).collect();
    let (chosen, start, next) = choose_turn(&times, |i| finished(&all[i].1), since, until);
    let (lines, heads) = read_requests(chosen.iter().map(|&i| &all[i]));
    RawPage { lines, start, next, session: format!("session {session}"), heads, about: None }
}

/// The lines of the proxy's request files CHOSEN (each its time and path), each read whole, and the times of those
/// read; a file gone since it was listed says so.
fn read_requests<'a>(chosen: impl Iterator<Item = &'a (u64, String)>) -> (Vec<String>, Vec<u64>) {
    let (mut lines, mut heads) = (Vec::new(), Vec::new());
    for (t, p) in chosen {
        match std::fs::read(p) {
            Ok(bytes) => {
                heads.push(*t);
                lines.extend(request_lines(*t, &String::from_utf8_lossy(&bytes)));
            }
            Err(_) => lines.push(format!("── a request at {t}: its log file is gone (the proxy deletes the oldest)")),
        }
    }
    (lines.into_iter().map(cut).collect(), heads)
}

/// L cut at [`RAW_LINE_MAX`], with how much was left out.
pub fn cut(l: String) -> String {
    let max = RAW_LINE_MAX;
    if l.len() <= max {
        return l;
    }
    let mut at = max;
    while !l.is_char_boundary(at) {
        at -= 1;
    }
    format!("{} … ({} more bytes)", &l[..at], l.len() - at)
}

/// The section of TEXT that starts with the line `=== NAME ===`, to the next section.
fn section<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let mark = format!("=== {name} ===");
    let at = text.find(&mark)? + mark.len();
    let rest = &text[at..];
    let end = rest.find("\n=== ").unwrap_or(rest.len());
    Some(&rest[..end])
}

/// One request's log TEXT (at time T) as lines: what went to the model and what came back.
fn request_lines(t: u64, text: &str) -> Vec<String> {
    // what went upstream: API REQUEST 1's body (the proxy may have changed the client's); else the client's own
    let body = section(text, "API REQUEST 1").and_then(|s| s.split_once("\nBody:\n").map(|(_, b)| b.trim())).or_else(|| section(text, "REQUEST BODY").map(str::trim)).unwrap_or("");
    let req: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let model = req["model"].as_str().unwrap_or("?");
    let status = section(text, "API RESPONSE 1").and_then(|s| s.lines().find_map(|l| l.strip_prefix("Status:")).map(str::trim)).unwrap_or("?");
    let mut out = vec![head_line(t, model, &format!("status {status}"))];
    if req.is_null() {
        out.push(format!("(the body is not JSON: {} bytes)", body.len()));
        return out;
    }
    out.extend(body_lines(&req));
    out.extend(reply(section(text, "API RESPONSE 1").unwrap_or("")));
    out
}

/// A request's head line, `══ request TIME · MODEL · HOW ══`: T (ns) in local time to the millisecond, HOW how it went
/// (`status 200`; a session record's `answered`).
fn head_line(t: u64, model: &str, how: &str) -> String {
    let when = chrono::DateTime::from_timestamp_nanos(t as i64).with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S%.3f");
    format!("══ request {when} · {model} · {how} ══")
}

/// A request body REQ (the Messages API's, or an OpenAI chat request) as lines: its system prompt, its tools, its
/// messages, and the rest of it on one line (`[params]`), the account's user left out.
fn body_lines(req: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    // the system prompt: a string or blocks
    match &req["system"] {
        serde_json::Value::String(s) => out.push(format!("[system]\n{s}")),
        serde_json::Value::Array(bs) => {
            for b in bs {
                out.push(format!("[system]\n{}", b["text"].as_str().map(str::to_string).unwrap_or_else(|| b.to_string())));
            }
        }
        _ => {}
    }
    if let Some(tools) = req["tools"].as_array() {
        out.push(format!("[tools] {}", tools.len()));
        for tl in tools {
            // the Messages API's `name`, an OpenAI chat request's `function.name`
            out.push(format!("[tool {}] {}", tl["name"].as_str().or(tl["function"]["name"].as_str()).unwrap_or("?"), tl));
        }
    }
    let msgs = req["messages"].as_array().cloned().unwrap_or_default();
    out.push(format!("[messages] {}", msgs.len()));
    for (i, m) in msgs.iter().enumerate() {
        let role = m["role"].as_str().unwrap_or("?");
        match &m["content"] {
            serde_json::Value::String(s) => out.push(format!("[{i} {role}]\n{s}")),
            serde_json::Value::Array(bs) => {
                for b in bs {
                    out.push(format!("[{i} {role} {}]\n{}", b["type"].as_str().unwrap_or("?"), block(b)));
                }
            }
            // an OpenAI chat assistant message that only calls tools
            serde_json::Value::Null if m["tool_calls"].is_array() => {}
            other => out.push(format!("[{i} {role}] {other}")),
        }
        // an OpenAI chat message's tool calls, each as its name and arguments (a `tool` message is its result)
        for c in m["tool_calls"].as_array().into_iter().flatten() {
            out.push(format!("[{i} {role} tool_use]\n{}", chat_call(c)));
        }
    }
    let mut rest: serde_json::Map<String, serde_json::Value> = req.as_object().map(|o| o.iter().filter(|(k, _)| !["system", "tools", "messages"].contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    // the account's user is nobody's to read in the pane (N-802.p5); metadata with nothing else in it goes with it
    if let Some(m) = rest.get_mut("metadata").and_then(serde_json::Value::as_object_mut) {
        m.remove("user_id");
        if m.is_empty() {
            rest.remove("metadata");
        }
    }
    out.push(format!("[params] {}", serde_json::Value::Object(rest)));
    out
}

/// A content block in words: text as it is, a tool call as its name and input, a result as its content.
fn block(b: &serde_json::Value) -> String {
    match b["type"].as_str() {
        Some("text") => b["text"].as_str().unwrap_or("").to_string(),
        Some("thinking") => b["thinking"].as_str().unwrap_or("").to_string(),
        Some("tool_use") => format!("{} {}", b["name"].as_str().unwrap_or("?"), b["input"]),
        Some("tool_result") => match &b["content"] {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(cs) => cs.iter().map(|c| c["text"].as_str().map(str::to_string).unwrap_or_else(|| c.to_string())).collect::<Vec<_>>().join("\n"),
            other => other.to_string(),
        },
        _ => b.to_string(),
    }
}

/// The reply in a response section's streamed events: each content block put together, then how it stopped. The
/// Messages API's events, or an OpenAI chat stream's chunks ([`chat_delta`]); not streamed, the one JSON answer of
/// either; an error's JSON as it came.
fn reply(resp: &str) -> Vec<String> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut stop = String::new();
    let mut plain = None;
    for l in resp.lines() {
        let Some(d) = l.strip_prefix("data:") else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(d.trim()) else { continue };
        match v["type"].as_str() {
            Some("content_block_start") => {
                let b = &v["content_block"];
                let kind = b["type"].as_str().unwrap_or("?").to_string();
                let head = if kind == "tool_use" { format!("{} ", b["name"].as_str().unwrap_or("?")) } else { String::new() };
                blocks.push((kind, head));
            }
            Some("content_block_delta") => {
                let d = &v["delta"];
                let piece = d["text"].as_str().or(d["thinking"].as_str()).or(d["partial_json"].as_str()).unwrap_or("");
                if let Some(b) = blocks.last_mut() {
                    b.1.push_str(piece);
                }
            }
            Some("message_delta") => stop = v["delta"]["stop_reason"].as_str().unwrap_or("").to_string(),
            Some("message") => plain = Some(v),
            _ => {
                if let Some(c) = v["choices"].get(0) {
                    chat_delta(&mut blocks, &c["delta"]);
                    if let Some(f) = c["finish_reason"].as_str() {
                        stop = f.to_string();
                    }
                }
            }
        }
    }
    if !blocks.is_empty() {
        let mut out: Vec<String> = blocks.into_iter().map(|(k, t)| format!("[reply {k}]\n{t}")).collect();
        out.push(format!("[reply stop] {stop}"));
        return out;
    }
    // a reply that was not streamed: one JSON answer, or an error
    let json = || resp.lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok());
    if let Some(v) = plain.or_else(|| json().find(|v| v.get("content").is_some())) {
        return answer_lines(&v);
    }
    if let Some(c) = json().find_map(|v| v["choices"].get(0).cloned()) {
        let m = &c["message"];
        let mut out: Vec<String> = [("thinking", m["reasoning_content"].as_str().or(m["reasoning"].as_str())), ("text", m["content"].as_str())]
            .into_iter()
            .filter_map(|(k, t)| Some(format!("[reply {k}]\n{}", t.filter(|t| !t.is_empty())?)))
            .collect();
        out.extend(m["tool_calls"].as_array().into_iter().flatten().map(|c| format!("[reply tool_use]\n{}", chat_call(c))));
        out.push(format!("[reply stop] {}", c["finish_reason"].as_str().unwrap_or("?")));
        return out;
    }
    if let Some(e) = json().find_map(|v| v.get("error").cloned()) {
        return vec![format!("[reply error] {e}")];
    }
    vec!["[reply] (none in the log)".into()]
}

/// An answer V, a Messages API message's JSON, as lines: each content block, then how it stopped.
fn answer_lines(v: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = v["content"].as_array().into_iter().flatten().map(|b| format!("[reply {}]\n{}", b["type"].as_str().unwrap_or("?"), block(b))).collect();
    out.push(format!("[reply stop] {}", v["stop_reason"].as_str().unwrap_or("?")));
    out
}

/// A request read from elsewhere than the proxy's log, a session's own record (coordd's `record_page`, fix 5,
/// accounts-lead N-1446 p2446), as the proxy's are read: its head (time T, MODEL, HOW it went), its body REQ rebuilt as
/// the Messages API's, then ERROR (the record's word for an answer that failed) and ANSWER (a message's JSON), or that
/// the record holds none; each line cut at [`RAW_LINE_MAX`].
pub fn rebuilt_lines(t: u64, model: &str, how: &str, req: &serde_json::Value, error: Option<&str>, answer: Option<&serde_json::Value>) -> Vec<String> {
    let mut out = vec![head_line(t, model, how)];
    out.extend(body_lines(req));
    out.extend(error.map(|e| format!("[reply error] {e}")));
    match answer {
        Some(a) => out.extend(answer_lines(a)),
        None => out.push("[reply] (none in its session's record)".into()),
    }
    out.into_iter().map(cut).collect()
}

/// An OpenAI chat tool call C as the Messages API's are shown: its name and its arguments.
fn chat_call(c: &serde_json::Value) -> String {
    format!("{} {}", c["function"]["name"].as_str().unwrap_or("?"), c["function"]["arguments"].as_str().unwrap_or(""))
}

/// One OpenAI chat stream chunk's DELTA added to BLOCKS, as the Messages API's blocks are: its reasoning (`thinking`),
/// its text, and each tool call (`tool_use`), whose first piece names it and whose arguments come in pieces after. A
/// block is begun where the kind changes or a call is named.
fn chat_delta(blocks: &mut Vec<(String, String)>, delta: &serde_json::Value) {
    let mut add = |kind: &str, named: Option<String>, piece: &str| match (blocks.last_mut(), named) {
        (Some((k, t)), None) if k.as_str() == kind => t.push_str(piece),
        (_, named) => blocks.push((kind.to_string(), named.unwrap_or_default() + piece)),
    };
    if let Some(r) = delta["reasoning_content"].as_str().or(delta["reasoning"].as_str()).filter(|r| !r.is_empty()) {
        add("thinking", None, r);
    }
    if let Some(t) = delta["content"].as_str().filter(|t| !t.is_empty()) {
        add("text", None, t);
    }
    for c in delta["tool_calls"].as_array().into_iter().flatten() {
        let f = &c["function"];
        add("tool_use", f["name"].as_str().map(|n| format!("{n} ")), f["arguments"].as_str().unwrap_or(""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "=== REQUEST INFO ===\nVersion: 8\nTimestamp: 2026-11-11T08:41:49.996213192+01:00\n\n=== HEADERS ===\nAuthorization: Bearer xxx\nX-Claude-Code-Session-Id: abc-123\n\n=== REQUEST BODY ===\n{\"model\":\"m\"}\n\n=== API REQUEST 1 ===\nTimestamp: 2026-11-11T08:41:50+01:00\nAuth: provider=claude, label=someone@example.com\n\nHeaders:\nX-Api-Key: secret\n\nBody:\n{\"model\":\"claude-haiku-5-5\",\"system\":[{\"type\":\"text\",\"text\":\"You are a fixer.\"}],\"tools\":[{\"name\":\"Bash\",\"input_schema\":{}}],\"messages\":[{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"fix mean\"}]}],\"max_tokens\":100,\"metadata\":{\"user_id\":\"user_7f3e_account_9a1c\"}}\n\n=== API RESPONSE 1 ===\nTimestamp: x\n\nStatus: 200\nHeaders:\nX: y\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Done\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\": 7adfb80\"}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n";

    #[test]
    fn a_request_reads_as_its_parts_and_no_header_but_the_allowed_shows() {
        let t = nanos("2026-11-11T08:41:49.996213192+01:00").unwrap();
        let lines = request_lines(t, LOG);
        let all = lines.join("\n");
        assert!(lines[0].contains("claude-haiku-5-5") && lines[0].contains("status 200"), "{}", lines[0]);
        assert!(all.contains("[system]\nYou are a fixer.") && all.contains("[tool Bash]") && all.contains("[0 user text]\nfix mean"), "{all}");
        assert!(all.contains("[reply text]\nDone: 7adfb80") && all.contains("[reply stop] end_turn"), "{all}");
        assert!(all.contains("\"max_tokens\":100"));
        // the body's metadata.user_id names the account's user: left out, and its empty metadata with it (N-802.p5)
        for secret in ["Bearer", "secret", "someone@example.com", "Authorization", "X-Api-Key", "user_7f3e", "metadata"] {
            assert!(!all.contains(secret), "{secret} shown: {all}");
        }
    }

    #[test]
    fn a_page_is_the_sessions_finished_requests_by_time() {
        let dir = std::env::temp_dir().join(format!("yaaihh-proxylog-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let other = LOG.replace("abc-123", "someone-else").replace("08:41:49.9", "08:41:51.0");
        let later = LOG.replace("08:41:49.9", "08:42:10.0");
        std::fs::write(dir.join("v1-messages-2026-11-11T084149-1.log"), LOG).unwrap();
        std::fs::write(dir.join("v1-messages-2026-11-11T084151-2.log"), other).unwrap();
        std::fs::write(dir.join("v1-messages-2026-11-11T084210-3.log"), later).unwrap();
        std::fs::write(dir.join("api-hello-1.log"), "x").unwrap();
        // a request with no session id (not Claude Code's), and a link out of the folder: neither is read for any
        // session, the empty one included (N-802.p13 points 3 and 4)
        let none = LOG.replace("X-Claude-Code-Session-Id: abc-123\n", "").replace("08:41:49.9", "08:41:55.0");
        std::fs::write(dir.join("v1-messages-2026-11-11T084155-4.log"), none).unwrap();
        let outside = std::env::temp_dir().join(format!("yaaihh-proxylog-out-{}.log", std::process::id()));
        std::fs::write(&outside, LOG.replace("08:41:49.9", "08:41:57.0")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("v1-messages-2026-11-11T084157-5.log")).unwrap();
        let d = dir.display().to_string();
        assert!(page(&d, "", None, None).lines.is_empty());
        let p = page(&d, "abc-123", None, None);
        assert_eq!(p.lines.iter().filter(|l| l.starts_with("══ request")).count(), 2, "{:#?}", p.lines);
        // reading on from next gives nothing new; from the first's time, both
        assert!(page(&d, "abc-123", None, Some(p.next)).lines.is_empty());
        let older = page(&d, "abc-123", Some(p.next - 1), None);
        assert_eq!(older.lines.iter().filter(|l| l.starts_with("══ request")).count(), 1);
        assert_eq!(older.next, p.next - 1);
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&outside).ok();
    }

    /// A request rebuilt from a session's own record (coordd's `record_page`, fix 5) reads as the proxy's: the same
    /// lines for the same body and answer but the head, which says how it went; an error is said before its answer,
    /// and no answer is said.
    #[test]
    fn a_request_rebuilt_from_a_record_reads_as_the_proxys() {
        let t = nanos("2026-11-11T08:41:49.996213192+01:00").unwrap();
        let logged = request_lines(t, LOG);
        let body = section(LOG, "API REQUEST 1").and_then(|s| s.split_once("\nBody:\n")).map(|(_, b)| b.trim()).unwrap();
        let req: serde_json::Value = serde_json::from_str(body).unwrap();
        let answer = serde_json::json!({"content": [{"type": "text", "text": "Done: 7adfb80"}], "stop_reason": "end_turn"});
        let rebuilt = rebuilt_lines(t, "claude-haiku-5-5", "answered", &req, None, Some(&answer));
        assert_eq!(rebuilt[1..], logged[1..], "the same lines but the head");
        assert!(rebuilt[0].ends_with(" · claude-haiku-5-5 · answered ══") && logged[0].ends_with(" · claude-haiku-5-5 · status 200 ══"), "{} / {}", rebuilt[0], logged[0]);
        let failed = rebuilt_lines(t, "m", "failed (authentication_failed)", &req, Some("authentication_failed"), Some(&answer));
        let at = |l: &str| failed.iter().position(|x| x == l);
        assert!(at("[reply error] authentication_failed") < at("[reply text]\nDone: 7adfb80"), "{failed:#?}");
        let none = rebuilt_lines(t, "m", "no answer in its record", &req, None, None);
        assert_eq!(none.last().map(String::as_str), Some("[reply] (none in its session's record)"));
    }

    /// An opencode agent's own file (its plugin's, agent-base `opencode/plugin/coord.js`): no headers, the session in
    /// REQUEST INFO, an OpenAI chat request and its stream. Made-up words only.
    const CALL: &str = "=== REQUEST INFO ===\nTimestamp: 2026-11-22T15:00:00.123000001Z\nSession: ses_made_up\n\n=== API REQUEST 1 ===\nBody:\n{\"model\":\"kimi-k3\",\"messages\":[{\"role\":\"system\",\"content\":\"You are a fixer.\"},{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"fix mean\"}]},{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\"}}]},{\"role\":\"tool\",\"tool_call_id\":\"c1\",\"content\":\"mean.rs\"}],\"tools\":[{\"type\":\"function\",\"function\":{\"name\":\"bash\",\"parameters\":{}}}],\"stream\":true}\n\n=== API RESPONSE 1 ===\nStatus: 200\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"Look at \"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"mean.rs\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Reading it\"}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c2\",\"type\":\"function\",\"function\":{\"name\":\"read\",\"arguments\":\"\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"filePath\\\":\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"mean.rs\\\"}\"}}]}}]}\n\ndata: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n=== END ===\n";

    /// An opencode agent's calls read as the proxy's: its session from REQUEST INFO, its OpenAI chat request as the
    /// system prompt, the messages with their tool calls and results, and the tools by name; the streamed reply as
    /// its reasoning, its text and its tool call put together. A file there is finished (written whole); one that
    /// did not stream, or failed, says its answer or its error.
    #[test]
    fn an_opencode_agents_calls_read_as_the_proxys() {
        let t = nanos("2026-11-22T15:00:00.123000001Z").unwrap();
        let all = request_lines(t, CALL).join("\n");
        assert!(all.starts_with("══ request ") && all.contains(" · kimi-k3 · status 200 ══"), "{all}");
        assert!(all.contains("[0 system]\nYou are a fixer.") && all.contains("[1 user text]\nfix mean"), "{all}");
        assert!(all.contains("[2 assistant tool_use]\nbash {\"command\":\"ls\"}") && !all.contains("[2 assistant] null"), "{all}");
        assert!(all.contains("[3 tool]\nmean.rs") && all.contains("[tool bash]"), "{all}");
        assert!(all.contains("[reply thinking]\nLook at mean.rs") && all.contains("[reply text]\nReading it"), "{all}");
        assert!(all.contains("[reply tool_use]\nread {\"filePath\":\"mean.rs\"}") && all.contains("[reply stop] tool_calls"), "{all}");
        let plain = CALL.split("=== API RESPONSE 1 ===").next().unwrap().to_string()
            + "=== API RESPONSE 1 ===\nStatus: 200\n\n{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Done\"},\"finish_reason\":\"stop\"}]}\n\n=== END ===\n";
        let all = request_lines(t, &plain).join("\n");
        assert!(all.contains("[reply text]\nDone") && all.contains("[reply stop] stop"), "{all}");
        let failed = CALL.split("=== API RESPONSE 1 ===").next().unwrap().to_string()
            + "=== API RESPONSE 1 ===\nStatus: 401\n\n{\"error\":{\"message\":\"made-up key\"}}\n\n=== END ===\n";
        let all = request_lines(t, &failed).join("\n");
        assert!(all.contains("status 401") && all.contains("[reply error] {\"message\":\"made-up key\"}"), "{all}");

        let dir = std::env::temp_dir().join(format!("yaaihh-proxylog-oc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("call-20261122T150000123000001Z.log"), CALL).unwrap();
        std::fs::write(dir.join("call-20261122T150001000000002Z.log"), CALL.replace("15:00:00.123000001", "15:00:01.000000002").replace("ses_made_up", "ses_other")).unwrap();
        // one being written: the plugin writes a hidden file and renames it whole, which is not read
        std::fs::write(dir.join(".call-20261122T150002000000003Z.log"), CALL.replace("15:00:00.123000001", "15:00:02.000000003")).unwrap();
        let p = page(&dir.display().to_string(), "ses_made_up", None, None);
        assert_eq!(p.heads, vec![t], "{:#?}", p.lines);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// On a real log: PROXY_LOG=DIR PROXY_SESSION=ID cargo test --release -p yaaihh reads_a_real_log -- --ignored --nocapture
    #[test]
    #[ignore]
    fn reads_a_real_log() {
        let (Ok(dir), Ok(session)) = (std::env::var("PROXY_LOG"), std::env::var("PROXY_SESSION")) else { return };
        let p = page(&dir, &session, None, None);
        for l in &p.lines {
            let first = l.lines().next().unwrap_or("");
            println!("{} ({} bytes)", first.chars().take(100).collect::<String>(), l.len());
        }
        println!("start {} next {} lines {}", p.start, p.next, p.lines.len());
    }
}
