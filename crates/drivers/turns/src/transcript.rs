//! A harness's own transcript read in place: its passages and reported usage by byte span, nothing copied.

use serde_json::Value as Json;
use std::io::{Read, Seek, SeekFrom};

/// Whose transcript shape a file has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Claude,
    Codex,
    /// agy-rpc's rollout: Codex's lines, a request's input holding its cache writes too.
    Agy,
    /// Words alone, as the window's driver appends them: a span is the passage itself.
    Plain,
}

impl Shape {
    pub fn of(harness: &str) -> Option<Shape> {
        match harness {
            "claude-code" | "claude" => Some(Shape::Claude),
            "codex" => Some(Shape::Codex),
            "agy" => Some(Shape::Agy),
            _ => None,
        }
    }
}

/// A line of the transcript: its passage text, if it has one; the usage it reports, by kind; the message the usage
/// belongs to when later lines of it report it again (none: this line's alone).
#[derive(Debug, Default, PartialEq)]
pub struct Read1 {
    pub text: Option<String>,
    pub usage: Vec<(&'static str, i64)>,
    pub message: Option<String>,
    pub minute: i64,
}

/// The id of the passage at bytes `off..off + len` of `path`.
pub fn id(path: &str, off: u64, len: u64) -> String {
    format!("t:{path}:{off}:{len}")
}

/// The path and span an id names; none if it is not a transcript passage's.
pub fn span(id: &str) -> Option<(&str, u64, u64)> {
    let rest = id.strip_prefix("t:")?;
    let mut parts = rest.rsplitn(3, ':');
    let (len, off, path) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?);
    Some((path, off, len))
}

/// The whole lines of `path` from byte `at` on, each with its place; the place after the last whole line.
pub fn lines(path: &str, at: u64) -> std::io::Result<(Vec<(u64, Vec<u8>)>, u64)> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(at))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let (mut out, mut from) = (Vec::new(), 0usize);
    while let Some(n) = buf[from..].iter().position(|&b| b == b'\n') {
        out.push((at + from as u64, buf[from..from + n].to_vec()));
        from += n + 1;
    }
    Ok((out, at + from as u64))
}

/// The bytes `off..off + len` of `path`.
pub fn bytes(path: &str, off: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(off))?;
    let mut buf = vec![0; len as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// One line as `shape` writes it.
pub fn read(shape: Shape, line: &[u8]) -> Read1 {
    let json = || serde_json::from_slice::<Json>(line).ok();
    let at = |v: &Json| v["timestamp"].as_str().and_then(minute).unwrap_or(0);
    match shape {
        Shape::Plain => Read1 { text: String::from_utf8(line.to_vec()).ok(), ..Default::default() },
        Shape::Claude => json().map_or_else(Read1::default, |v| claude(&v, at(&v))),
        Shape::Codex => json().map_or_else(Read1::default, |v| codex(&v, at(&v))),
        Shape::Agy => json().map_or_else(Read1::default, |v| agy(&v, at(&v))),
    }
}

/// Text blocks of `content` whose type is `kind`, joined; none when empty.
fn texts(content: &Json, kind: &str) -> Option<String> {
    let t: Vec<&str> = content.as_array()?.iter().filter(|b| b["type"] == kind).filter_map(|b| b["text"].as_str()).collect();
    let t = t.join("\n");
    (!t.trim().is_empty()).then_some(t)
}

/// An assistant line of the main session: its text; its usage by Claude Code's four counts (no reasoning count).
fn claude(v: &Json, minute: i64) -> Read1 {
    if v["type"] != "assistant" || v["isSidechain"] == true {
        return Read1::default();
    }
    let m = &v["message"];
    let u = &m["usage"];
    let usage = [("input", "input_tokens"), ("cached_read", "cache_read_input_tokens"), ("cache_write", "cache_creation_input_tokens"), ("output", "output_tokens")]
        .into_iter()
        .filter_map(|(k, f)| Some((k, u[f].as_i64()?)))
        .collect();
    Read1 { text: texts(&m["content"], "text"), usage, message: m["id"].as_str().map(str::to_string), minute }
}

/// An assistant message, or a `token_count` with its last request's counts: input and output less what they include
/// of cached reads and reasoning.
fn codex(v: &Json, minute: i64) -> Read1 {
    let p = &v["payload"];
    match (v["type"].as_str(), p["type"].as_str()) {
        (Some("response_item"), Some("message")) if p["role"] == "assistant" => Read1 { text: texts(&p["content"], "output_text"), minute, ..Default::default() },
        (Some("event_msg"), Some("token_count")) => {
            let u = &p["info"]["last_token_usage"];
            let n = |f: &str| u[f].as_i64();
            let mut usage = Vec::new();
            if let Some(i) = n("input_tokens") {
                usage.push(("input", i - n("cached_input_tokens").unwrap_or(0)));
            }
            usage.extend(n("cached_input_tokens").map(|c| ("cached_read", c)));
            if let Some(o) = n("output_tokens") {
                usage.push(("output", o - n("reasoning_output_tokens").unwrap_or(0)));
            }
            usage.extend(n("reasoning_output_tokens").map(|r| ("reasoning", r)));
            Read1 { usage, minute, ..Default::default() }
        }
        _ => Read1::default(),
    }
}

/// A Codex line as agy-rpc writes it: a request's `cache_write_input_tokens`, which its input holds (agy-rpc's
/// `gemini_usage`), counted apart from the input, as v1's `codex_tokens` does.
fn agy(v: &Json, minute: i64) -> Read1 {
    let mut r = codex(v, minute);
    if let Some(w) = v["payload"]["info"]["last_token_usage"]["cache_write_input_tokens"].as_i64() {
        r.usage.iter_mut().filter(|(k, _)| *k == "input").for_each(|(_, i)| *i -= w);
        r.usage.push(("cache_write", w));
    }
    r
}

/// The text of the passage `id` names, read from its span.
pub fn text_of(shape: Shape, id: &str) -> Result<String, String> {
    let (path, off, len) = span(id).ok_or(format!("{id} names no transcript passage"))?;
    let line = bytes(path, off, len).map_err(|e| format!("{path}: {e}"))?;
    read(shape, &line).text.ok_or(format!("{id}: no passage there"))
}

/// Minutes since 1970 of an RFC 3339 time (`2026-11-26T13:35:48.123Z`), UTC.
pub fn minute(t: &str) -> Option<i64> {
    let n = |a: usize, b: usize| t.get(a..b)?.parse::<i64>().ok();
    let (y, m, d, h, mi) = (n(0, 4)?, n(5, 7)?, n(8, 10)?, n(11, 13)?, n(14, 16)?);
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let days = era * 146097 + (yoe * 365 + yoe / 4 - yoe / 100 + doy) - 719468;
    Some(days * 1440 + h * 60 + mi)
}
