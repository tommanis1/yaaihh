//! Claude Code's own telemetry, received by coordd: what the transcript does not hold of a request (its duration, time
//! to first token, failed requests), written as lines of the usage file (`usage.rs`, which joins them to the rows).
//!
//! Claude Code exports OpenTelemetry logs over OTLP/HTTP with JSON bodies when its session's environment says so
//! ([`env`]); coordd answers on a loopback port the system chose, written to `$COORD/telemetry.port`. Only the HTTP a
//! client of OTLP sends is understood: a request line, headers, a body by `Content-Length` or chunked, `Expect:
//! 100-continue`, keep-alive; the rest is refused with its status and the connection closed.
//!
//! A row must come from a session this coordd started: each request carries the coord's token in the header
//! [`HEADER`], and one without it is refused before its body is read. The token is `$COORD/telemetry.token` (mode 600),
//! made at the receiver's first start and kept; it is in no reply, log line or test output, and roster puts it into the
//! session's environment (`claude/telemetry-token`). It is for right numbers, not against malice.
//!
//! Kept of a log record only what a row needs ([`lines_of`]): of `api_request` the session, request id, model, the
//! four token counts it is joined by, duration and time to first token; of `api_error` and `api_retries_exhausted`
//! the session, request id, model, status, error (cut short), attempts and time. Nothing of a body, no prompt text,
//! and none of the user's or organization's attributes (`user.email`, `user.account_uuid`, `organization.id`, …):
//! they are not read. The metrics are not asked for ([`env`]): no row field comes from them.

use super::usage::{self, Failed, Line, Timing};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

/// The agent and account a session runs as, if coordd knows (the core's agents; the lead's wiring), for a failed
/// request's line.
pub type Who = Arc<dyn Fn(&str) -> Option<(String, String)> + Send + Sync>;

/// The request header that carries the coord's token ([`token`]); a session's exporter sends it by
/// `OTEL_EXPORTER_OTLP_HEADERS` (`claude/telemetry-token`).
pub const HEADER: &str = "x-coord-token";

/// The token of the coord at COORD, from `COORD/telemetry.token`; made there, the owner's only, when there is none.
fn token(coord: &str) -> std::io::Result<String> {
    use std::io::{Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    let file = format!("{coord}/telemetry.token");
    let named = |e: std::io::Error| std::io::Error::new(e.kind(), format!("{file}: {e}"));
    match std::fs::read_to_string(&file) {
        Ok(t) if !t.trim().is_empty() => return Ok(t.trim().to_string()),
        Ok(_) => std::fs::remove_file(&file).map_err(named)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(named(e)),
    }
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let t: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&file).and_then(|mut f| f.write_all(t.as_bytes())).map_err(named)?;
    Ok(t)
}

/// The most a request's line and headers may take.
const HEAD_LIMIT: u64 = 64 * 1024;
/// The most a body may take; Claude Code sends a batch every 5 s, a few KiB in the try (8729 bytes the largest).
const BODY_LIMIT: usize = 16 * 1024 * 1024;

/// Start the receiver for the coord at COORD: listen on a loopback port, write it to `COORD/telemetry.port`, and
/// serve each connection in a task of its own; the port. The port is the one the file names from coordd's last
/// start when it is free (the sessions that run were started with it, and send there until they are started again),
/// else one the system chooses. WHO names a failed request's agent and account. Requests are taken with the coord's
/// token only ([`token`]).
pub async fn start(coord: String, who: Who) -> std::io::Result<u16> {
    let token: Arc<str> = {
        let coord = coord.clone();
        tokio::task::spawn_blocking(move || token(&coord)).await.map_err(std::io::Error::other)??.into()
    };
    let file = format!("{coord}/telemetry.port");
    let last: Option<u16> = tokio::fs::read_to_string(&file).await.ok().and_then(|p| p.trim().parse().ok());
    let listener = match last {
        Some(p) => match TcpListener::bind(("127.0.0.1", p)).await {
            Ok(l) => l,
            Err(_) => TcpListener::bind("127.0.0.1:0").await?,
        },
        None => TcpListener::bind("127.0.0.1:0").await?,
    };
    let port = listener.local_addr()?.port();
    tokio::fs::write(&file, port.to_string()).await.map_err(|e| std::io::Error::new(e.kind(), format!("{file}: {e}")))?;
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((s, _)) => {
                    let (r, w) = s.into_split();
                    tokio::spawn(serve(BufReader::new(r), w, coord.clone(), who.clone(), token.clone()));
                }
                Err(e) => {
                    // out of descriptors, mostly: wait for some to close
                    eprintln!("telemetry: accept on 127.0.0.1:{port}: {e}");
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            }
        }
    });
    Ok(port)
}

/// The environment of an agent's Claude Code session that sends its telemetry logs to the receiver on PORT, as
/// OTLP/HTTP JSON (the doc's names). The account's UUID is asked to be left out (`OTEL_METRICS_INCLUDE_ACCOUNT_UUID`,
/// "Include user.account_uuid and user.account_id attributes in metrics and events"); the metrics are off.
pub fn env(port: u16) -> Vec<(&'static str, String)> {
    [
        ("CLAUDE_CODE_ENABLE_TELEMETRY", "1".to_string()),
        ("OTEL_LOGS_EXPORTER", "otlp".to_string()),
        ("OTEL_METRICS_EXPORTER", "none".to_string()),
        ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json".to_string()),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", format!("http://127.0.0.1:{port}")),
        ("OTEL_METRICS_INCLUDE_ACCOUNT_UUID", "false".to_string()),
    ]
    .into()
}

/// What must not be in that environment, whatever the owner's own shell has: the variables that would put prompt,
/// response, tool or body text into the telemetry, turn on traces, send the logs elsewhere (a per-signal endpoint or
/// protocol wins over [`env`]'s), or add headers to them. The one header a session sends, the coord's token, is set
/// after these are removed (`claude/telemetry-token`).
pub const UNSET: &[&str] = &[
    "OTEL_LOG_USER_PROMPTS",
    "OTEL_LOG_ASSISTANT_RESPONSES",
    "OTEL_LOG_TOOL_DETAILS",
    "OTEL_LOG_TOOL_CONTENT",
    "OTEL_LOG_RAW_API_BODIES",
    "CLAUDE_CODE_ENHANCED_TELEMETRY_BETA",
    "ENABLE_ENHANCED_TELEMETRY_BETA",
    "OTEL_TRACES_EXPORTER",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
    "OTEL_EXPORTER_OTLP_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
];

/// A request read: its method, path (without a query) and body, and whether the connection stays open after it.
struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
    keep: bool,
}

/// Why no request was read: the connection closed or failed, or the request is refused with this status and reason.
enum Stop {
    Gone,
    Refuse(u16, &'static str),
}

impl From<std::io::Error> for Stop {
    fn from(_: std::io::Error) -> Stop {
        Stop::Gone
    }
}

/// Serve one connection: its requests one after another until its sender closes it or sends one that is refused,
/// as every request without TOKEN is. A silent connection stays, as long as its session lives: no time limit of ours
/// ends it.
async fn serve(mut r: impl AsyncBufRead + Unpin, mut w: impl AsyncWrite + Unpin, coord: String, who: Who, token: Arc<str>) {
    loop {
        let req = match request(&mut r, &mut w, &token).await {
            Ok(req) => req,
            Err(Stop::Refuse(status, why)) => {
                let _ = answer(&mut w, status, why, false).await;
                return;
            }
            Err(Stop::Gone) => return,
        };
        let (status, text) = match (req.method.as_str(), req.path.as_str()) {
            ("POST", "/v1/logs") => match serde_json::from_slice::<Value>(&req.body) {
                Ok(v) => {
                    let lines = lines_of(&v, &*who);
                    let cells = super::cells::of_lines(&coord, &lines, &*who);
                    let coord = coord.clone();
                    // a disk write is not done on the runtime's threads
                    let written = tokio::task::spawn_blocking(move || lines.iter().try_for_each(|l| usage::append(&coord, l)).and_then(|_| super::cells::append(&coord, &cells))).await;
                    if let Ok(Err(e)) = written {
                        eprintln!("telemetry: {e}");
                    }
                    (200, "{}")
                }
                Err(_) => (400, "not JSON"),
            },
            ("POST", "/v1/metrics") => (200, "{}"),
            (_, "/v1/logs" | "/v1/metrics") => (405, "POST only"),
            _ => (404, "only /v1/logs and /v1/metrics are here"),
        };
        if answer(&mut w, status, text, req.keep).await.is_err() || !req.keep {
            return;
        }
    }
}

/// Write the answer STATUS with TEXT (JSON when it is `{}`), saying whether the connection stays open (KEEP).
async fn answer(w: &mut (impl AsyncWrite + Unpin), status: u16, text: &str, keep: bool) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        _ => "Error",
    };
    let kind = if text == "{}" { "application/json" } else { "text/plain" };
    let connection = if keep { "keep-alive" } else { "close" };
    let head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: {connection}\r\n\r\n", text.len());
    w.write_all(head.as_bytes()).await?;
    w.write_all(text.as_bytes()).await?;
    w.flush().await
}

/// One line ending in CRLF or LF, without it, taking from BUDGET; `Gone` at the end of the stream, BAD when the line
/// is longer than the budget left.
async fn line(r: &mut (impl AsyncBufRead + Unpin), budget: &mut u64, bad: Stop) -> Result<String, Stop> {
    let mut buf = Vec::new();
    let n = r.take(*budget).read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Err(Stop::Gone);
    }
    if buf.last() != Some(&b'\n') {
        return Err(if (n as u64) < *budget { Stop::Gone } else { bad });
    }
    *budget -= n as u64;
    buf.pop();
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf).map_err(|_| Stop::Refuse(400, "not UTF-8"))
}

/// Read one request from R; W gets the `100 Continue` a client that asks for it waits for, once its body is one taken.
/// A request whose [`HEADER`] is not TOKEN is refused before that, with nothing of its body read.
async fn request(r: &mut (impl AsyncBufRead + Unpin), w: &mut (impl AsyncWrite + Unpin), token: &str) -> Result<Request, Stop> {
    let mut budget = HEAD_LIMIT;
    let first = line(r, &mut budget, Stop::Refuse(431, "request line too long")).await?;
    let mut parts = first.split(' ');
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next()) else { return Err(Stop::Refuse(400, "bad request line")) };
    let mut headers = HashMap::new();
    loop {
        let h = line(r, &mut budget, Stop::Refuse(431, "headers too long")).await?;
        if h.is_empty() {
            break;
        }
        let Some((k, v)) = h.split_once(':') else { return Err(Stop::Refuse(400, "bad header")) };
        headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
    }
    if headers.get(HEADER).map(String::as_str) != Some(token) {
        return Err(Stop::Refuse(401, "not a session of this coord"));
    }
    let has = |k: &str, v: &str| headers.get(k).is_some_and(|h| h.split(',').any(|p| p.trim().eq_ignore_ascii_case(v)));
    let keep = if version == "HTTP/1.0" { has("connection", "keep-alive") } else { !has("connection", "close") };
    let chunked = has("transfer-encoding", "chunked");
    let length = match headers.get("content-length") {
        Some(_) if chunked => return Err(Stop::Refuse(400, "both a length and chunks")),
        Some(n) => Some(n.parse::<usize>().map_err(|_| Stop::Refuse(400, "bad content-length"))?),
        None if headers.contains_key("transfer-encoding") && !chunked => return Err(Stop::Refuse(411, "a length or chunks")),
        None => None,
    };
    if length.is_some_and(|n| n > BODY_LIMIT) {
        return Err(Stop::Refuse(413, "body over 16 MiB"));
    }
    if has("expect", "100-continue") && (chunked || length.is_some_and(|n| n > 0)) {
        w.write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await?;
        w.flush().await?;
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let mut budget = 1024;
            let size = line(r, &mut budget, Stop::Refuse(400, "bad chunk size")).await?;
            let size = usize::from_str_radix(size.split(';').next().unwrap_or("").trim(), 16).map_err(|_| Stop::Refuse(400, "bad chunk size"))?;
            if size == 0 {
                // trailers, up to the empty line
                let mut budget = HEAD_LIMIT;
                while !line(r, &mut budget, Stop::Refuse(431, "trailers too long")).await?.is_empty() {}
                break;
            }
            if body.len() + size > BODY_LIMIT {
                return Err(Stop::Refuse(413, "body over 16 MiB"));
            }
            let at = body.len();
            body.resize(at + size, 0);
            r.read_exact(&mut body[at..]).await?;
            let mut budget = 2;
            if !line(r, &mut budget, Stop::Refuse(400, "chunk not ended")).await?.is_empty() {
                return Err(Stop::Refuse(400, "chunk not ended"));
            }
        }
    } else if let Some(n) = length {
        body.resize(n, 0);
        r.read_exact(&mut body).await?;
    }
    // refused once read, so the connection closes on nothing unread
    if headers.get("content-encoding").is_some_and(|e| !e.eq_ignore_ascii_case("identity")) {
        return Err(Stop::Refuse(415, "no content encoding is taken"));
    }
    Ok(Request { method: method.to_string(), path: target.split('?').next().unwrap_or("").to_string(), body, keep })
}

/// The usage lines of an OTLP logs body V (`resourceLogs[].scopeLogs[].logRecords[]`): a [`Timing`] per
/// `api_request`, a [`Failed`] per `api_error` and `api_retries_exhausted`, WHO naming a failure's agent and account;
/// every other record is passed over. A record's event is its `event.name` attribute (`api_request` in the try), else
/// its body without the `claude_code.` prefix.
pub fn lines_of(v: &Value, who: &dyn Fn(&str) -> Option<(String, String)>) -> Vec<Line> {
    let records = v["resourceLogs"].as_array().into_iter().flatten().flat_map(|r| r["scopeLogs"].as_array().into_iter().flatten()).flat_map(|s| s["logRecords"].as_array().into_iter().flatten());
    records
        .filter_map(|rec| {
            let a: HashMap<&str, &Value> =
                rec["attributes"].as_array().into_iter().flatten().filter_map(|kv| Some((kv["key"].as_str()?, kv["value"].as_object()?.values().next()?))).collect();
            let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(str::to_string);
            // OTLP JSON gives an intValue as a string or a number; Claude Code sends some numbers as strings
            let n = |k: &str| a.get(k).and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f as u64)).or_else(|| v.as_str()?.parse().ok()));
            let event = s("event.name").or_else(|| rec["body"]["stringValue"].as_str().map(|b| b.strip_prefix("claude_code.").unwrap_or(b).to_string()))?;
            let at = s("event.timestamp").or_else(|| nanos(&rec["timeUnixNano"])).unwrap_or_default();
            let session = s("session.id").unwrap_or_default();
            let model = s("model").unwrap_or_default();
            match event.as_str() {
                "api_request" => Some(Line::Timing(Timing {
                    at,
                    session,
                    request_id: s("request_id"),
                    model,
                    input: n("input_tokens").unwrap_or(0),
                    output: n("output_tokens").unwrap_or(0),
                    cache_read: n("cache_read_tokens").unwrap_or(0),
                    cache_write: n("cache_creation_tokens").unwrap_or(0),
                    duration_ms: n("duration_ms"),
                    ttft_ms: n("ttft_ms"),
                })),
                "api_error" | "api_retries_exhausted" => {
                    let exhausted = event == "api_retries_exhausted";
                    let (agent, account) = who(&session).unzip();
                    Some(Line::Failed(Failed {
                        at,
                        agent,
                        account,
                        request_id: s("request_id"),
                        model,
                        status: n("status_code"),
                        error: crate::transcript::clip(&s("error").unwrap_or_default(), usage::ERROR_CHARS),
                        attempt: n(if exhausted { "total_attempts" } else { "attempt" }),
                        duration_ms: n(if exhausted { "total_retry_duration_ms" } else { "duration_ms" }),
                        exhausted,
                        session,
                    }))
                }
                _ => None,
            }
        })
        .collect()
}

/// An OTLP time (nanoseconds since the epoch, as a string or number) in RFC 3339, ms, UTC.
fn nanos(v: &Value) -> Option<String> {
    let ns: i64 = v.as_i64().or_else(|| v.as_str()?.parse().ok())?;
    Some(chrono::DateTime::from_timestamp_millis(ns / 1_000_000)?.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testing::Tmp;

    /// An OTLP attribute.
    fn attr(k: &str, v: Value) -> Value {
        let kind = match &v {
            Value::String(_) => "stringValue",
            Value::Bool(_) => "boolValue",
            Value::Number(n) if n.is_f64() => "doubleValue",
            _ => "intValue",
        };
        serde_json::json!({"key": k, "value": {kind: v}})
    }

    /// A logs body in Claude Code's shape, one record per (EVENT, attributes), each also carrying the user's and
    /// organization's attributes that must not be kept.
    fn body(records: &[(&str, Vec<Value>)]) -> Value {
        let recs: Vec<Value> = records
            .iter()
            .map(|(event, attrs)| {
                let mut a = vec![
                    attr("user.id", "u-hash".into()),
                    attr("user.email", "someone@example.com".into()),
                    attr("user.account_uuid", "acc-uuid".into()),
                    attr("organization.id", "org-uuid".into()),
                    attr("session.id", "s1".into()),
                    attr("event.name", (*event).into()),
                    attr("event.timestamp", "2026-11-18T09:43:23.004Z".into()),
                ];
                a.extend(attrs.iter().cloned());
                serde_json::json!({"timeUnixNano": "1794995003004000000", "body": {"stringValue": format!("claude_code.{event}")}, "attributes": a})
            })
            .collect();
        serde_json::json!({"resourceLogs": [{"resource": {"attributes": [attr("service.name", "claude-code".into())]}, "scopeLogs": [{"scope": {"name": "com.anthropic.claude_code.events"}, "logRecords": recs}]}]})
    }

    fn try1_body() -> Value {
        body(&[
            ("user_prompt", vec![attr("prompt_length", "2".into()), attr("prompt", "<REDACTED>".into())]),
            (
                "api_request",
                vec![
                    attr("model", "claude-haiku-5-5".into()),
                    attr("input_tokens", 2.into()),
                    attr("output_tokens", 76.into()),
                    attr("cache_read_tokens", "11121".into()),
                    attr("cache_creation_tokens", 6449.into()),
                    attr("cost_usd", 0.0123.into()),
                    attr("duration_ms", 1998.into()),
                    attr("ttft_ms", 912.into()),
                    attr("request_id", "req_1".into()),
                ],
            ),
            (
                "api_error",
                vec![attr("model", "claude-haiku-5-5".into()), attr("error", "x".repeat(900).into()), attr("status_code", 529.into()), attr("attempt", 11.into()), attr("duration_ms", 40.into())],
            ),
            ("api_retries_exhausted", vec![attr("model", "claude-haiku-5-5".into()), attr("error", "overloaded".into()), attr("total_attempts", 11.into()), attr("total_retry_duration_ms", 61000.into())]),
        ])
    }

    fn who() -> Who {
        Arc::new(|s: &str| (s == "s1").then(|| ("orch".to_string(), "work".to_string())))
    }

    #[test]
    fn a_logs_body_gives_a_timing_and_failed_requests_and_nothing_of_the_user() {
        let lines = lines_of(&try1_body(), &*who());
        assert_eq!(lines.len(), 3, "the prompt's record is passed over");
        let Line::Timing(t) = &lines[0] else { panic!("{lines:?}") };
        assert_eq!(
            (t.session.as_str(), t.request_id.as_deref(), t.model.as_str(), t.input, t.output, t.cache_read, t.cache_write, t.duration_ms, t.ttft_ms, t.at.as_str()),
            ("s1", Some("req_1"), "claude-haiku-5-5", 2, 76, 11121, 6449, Some(1998), Some(912), "2026-11-18T09:43:23.004Z")
        );
        let Line::Failed(e) = &lines[1] else { panic!() };
        assert_eq!((e.agent.as_deref(), e.account.as_deref(), e.status, e.attempt, e.duration_ms, e.exhausted), (Some("orch"), Some("work"), Some(529), Some(11), Some(40), false));
        assert!(e.error.chars().count() <= usage::ERROR_CHARS + 1, "{}", e.error.len());
        let Line::Failed(x) = &lines[2] else { panic!() };
        assert_eq!((x.status, x.attempt, x.duration_ms, x.exhausted), (None, Some(11), Some(61000), true));
        let text = lines.iter().map(|l| serde_json::to_string(l).unwrap()).collect::<String>();
        for kept_out in ["someone@example.com", "acc-uuid", "org-uuid", "u-hash", "REDACTED", "cost"] {
            assert!(!text.contains(kept_out), "{kept_out} in {text}");
        }
    }

    /// Send RAW to the receiver on PORT and read until it closes the connection (or 2 s pass): what it answered.
    async fn send(port: u16, raw: &[&[u8]]) -> String {
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        for part in raw {
            s.write_all(part).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(2), s.read_to_end(&mut out)).await;
        String::from_utf8_lossy(&out).to_string()
    }

    #[tokio::test]
    async fn the_receiver_takes_logs_by_length_or_chunks_after_100_continue_and_refuses_the_rest() {
        let t = Tmp::new("telemetry-http");
        let coord = t.dir.to_string_lossy().to_string();
        let port = start(coord.clone(), who()).await.unwrap();
        assert_eq!(std::fs::read_to_string(t.dir.join("telemetry.port")).unwrap(), port.to_string());
        let json = try1_body().to_string();
        // the coord's token: the owner's only, the same after a second start, and each request below carries it
        let file = t.dir.join("telemetry.token");
        let tok = std::fs::read_to_string(&file).unwrap();
        assert!(tok.len() == 48 && std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&file).unwrap().permissions()) & 0o777 == 0o600);
        assert!(token(&coord).unwrap() == tok, "the token is kept");
        let h = |req: &str| req.replacen("\r\n", &format!("\r\nX-Coord-Token: {tok}\r\n"), 1);

        // without the token, or with another: refused before 100 Continue, and no line is written
        let bare = format!("POST /v1/logs HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n", json.len());
        let out = send(port, &[bare.as_bytes()]).await;
        assert!(out.starts_with("HTTP/1.1 401 ") && !out.contains("100 Continue") && !out.contains(&tok), "a request without the token is refused");
        let other = bare.replacen("\r\n", "\r\nX-Coord-Token: 00\r\n", 1);
        assert!(send(port, &[other.as_bytes(), json.as_bytes()]).await.starts_with("HTTP/1.1 401 "), "and one with another token");
        assert!(!std::path::Path::new(&usage::month_file(&coord, "2026-11")).exists());

        // by length, after 100-continue, on a connection kept for a second request (metrics) that closes it
        let head = h(&format!("POST /v1/logs HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n", json.len()));
        let metrics = h("POST /v1/metrics HTTP/1.1\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: 2\r\n\r\n{}");
        let out = send(port, &[head.as_bytes(), json.as_bytes(), metrics.as_bytes()]).await;
        assert!(out.starts_with("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\n"), "{out}");
        assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 2, "{out}");
        assert!(out.ends_with("Connection: close\r\n\r\n{}"), "{out}");

        // chunked, in two chunks with an extension and a trailer
        let (a, b) = json.split_at(100);
        let chunked = h(&format!("POST /v1/logs?x=1 HTTP/1.1\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x};ext=1\r\n{a}\r\n{:x}\r\n{b}\r\n0\r\nX-T: 1\r\n\r\n", a.len(), b.len()));
        assert!(send(port, &[chunked.as_bytes()]).await.starts_with("HTTP/1.1 200 OK"));

        // oversize: refused before 100 Continue, nothing read of it
        let big = h(&format!("POST /v1/logs HTTP/1.1\r\nExpect: 100-continue\r\nContent-Length: {}\r\n\r\n", BODY_LIMIT + 1));
        let out = send(port, &[big.as_bytes()]).await;
        assert!(out.starts_with("HTTP/1.1 413 ") && !out.contains("100 Continue"), "{out}");
        // elsewhere, compressed, not JSON
        assert!(send(port, &[h("GET / HTTP/1.1\r\nConnection: close\r\n\r\n").as_bytes()]).await.starts_with("HTTP/1.1 404 "));
        assert!(send(port, &[h("POST /v1/logs HTTP/1.1\r\nContent-Encoding: gzip\r\nContent-Length: 1\r\n\r\nx").as_bytes()]).await.starts_with("HTTP/1.1 415 "));
        assert!(send(port, &[h("POST /v1/logs HTTP/1.1\r\nConnection: close\r\nContent-Length: 3\r\n\r\nabc").as_bytes()]).await.starts_with("HTTP/1.1 400 "));

        // the two logs bodies are in the usage file: a timing and two failures each, the failures with their agent
        let wait = async {
            loop {
                let m = std::fs::read_to_string(usage::month_file(&coord, "2026-11")).unwrap_or_default();
                if m.lines().count() == 6 {
                    break m;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        let m = tokio::time::timeout(Duration::from_secs(5), wait).await.expect("six lines in the usage file");
        assert_eq!(m.matches("\"kind\":\"timing\"").count(), 2);
        assert_eq!(m.matches("\"agent\":\"orch\"").count(), 4);
    }

    #[test]
    fn the_session_environment_points_at_the_port_and_unsets_what_would_send_text() {
        let e: HashMap<_, _> = env(43123).into_iter().collect();
        assert_eq!(e["OTEL_EXPORTER_OTLP_ENDPOINT"], "http://127.0.0.1:43123");
        assert_eq!((e["OTEL_EXPORTER_OTLP_PROTOCOL"].as_str(), e["OTEL_LOGS_EXPORTER"].as_str(), e["OTEL_METRICS_EXPORTER"].as_str()), ("http/json", "otlp", "none"));
        assert!(UNSET.iter().all(|k| !e.contains_key(k)) && UNSET.contains(&"OTEL_LOG_RAW_API_BODIES"));
    }
}
