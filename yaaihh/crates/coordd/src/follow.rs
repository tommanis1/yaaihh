//! Following a Claude Code session's transcript: the one record coordd takes a Claude agent's turns, running tool calls
//! and seen mail from (design coordd-core-2026-11-10 §3). Hooks only say "look now"; a hook that comes late or never
//! costs a poll, not a wrong state. Delivery itself is the channel's acknowledgement (§4); the transcript showing a
//! mail adds nothing but a delivery the channel did not report (a session from before).
//!
//! A turn runs from a user or assistant entry to its end: the assistant's final message (`stop_reason: end_turn`),
//! Claude Code's `turn_duration` record, an interrupt (`[Request interrupted by user]`) or an API error message. A tool
//! call runs from its `tool_use` block to the `tool_result` that answers it; a turn's end ends any left. Meta
//! entries other than channel messages, compaction summaries and slash-command echoes start no turn. Mail arrives as a channel entry
//! (`<channel source="coord" ... id="m12">`) or, in sessions from before, inside a hook's "Mail that arrived for you"
//! text (`[m12 · from X]`). A channel message the session cannot take now (a menu open, a turn running) is queued
//! (`queue-operation` enqueue) and taken later: the session has it, so it counts as delivered then (9 Nov: orch's
//! open model menu queued 34 mails, which went again twice while coordd waited for them as entries).

use coord_types::Tool;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::time::Instant;

/// How much of a transcript a new follower reads first: enough for the last turn's state and the mail it took.
const WINDOW: u64 = 8 << 20;

/// Where a transcript stands, and the mail ids it showed since the last read.
pub struct Follower {
    pub path: String,
    offset: u64,
    rest: Vec<u8>,
    /// Whether no turn runs (the last turn-marking entry ended one, or there is none).
    pub ended: bool,
    /// When the entry that began the turn running was written (its `timestamp`), when one runs.
    pub running_since: String,
    /// The tool calls running: begun (`tool_use`) and not answered (`tool_result`) in the turn running.
    pub tools: BTreeMap<String, Tool>,
    /// When the transcript last grew.
    pub grew: Instant,
    /// The last model picked with `/model` in the session, and when (the entry's `timestamp`).
    pub model: Option<(String, String)>,
    /// The first read starts inside a line (a window that does not begin at the file's start).
    skip_partial: bool,
}

/// What one read found.
#[derive(Default)]
pub struct Seen {
    /// The mail ids of the entries read, in order.
    pub seen: Vec<String>,
    pub grew: bool,
    /// Whether the running tool calls changed.
    pub tools: bool,
}

#[derive(Deserialize)]
struct Entry {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    subtype: String,
    #[serde(rename = "isMeta", default)]
    meta: bool,
    #[serde(rename = "isCompactSummary", default)]
    compact: bool,
    #[serde(rename = "isApiErrorMessage", default)]
    api_error: bool,
    #[serde(default)]
    message: Option<Message>,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    timestamp: String,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: serde_json::Value,
    #[serde(default)]
    stop_reason: Option<String>,
}

enum Mark {
    Running,
    Ended,
}

/// What one entry says: of the turn (None: nothing), the mail ids it carries, the tool calls it begins and the ones
/// it answers.
#[derive(Default)]
struct Said {
    mark: Option<Mark>,
    ids: Vec<String>,
    began: Vec<Tool>,
    answered: Vec<String>,
    /// The model a `/model` pick set, and when.
    model: Option<(String, String)>,
}

impl Follower {
    /// Follow PATH from its last [`WINDOW`] bytes (the start of a line within them).
    pub fn new(path: &str) -> Follower {
        let len = std::fs::metadata(path).map_or(0, |m| m.len());
        let mut f = Follower { path: path.into(), offset: len.saturating_sub(WINDOW), rest: Vec::new(), ended: true, running_since: String::new(), tools: BTreeMap::new(), grew: Instant::now(), model: None, skip_partial: false };
        f.skip_partial = f.offset > 0;
        f
    }

    /// Read what was written since the last read: the state it leaves and the mail ids it shows.
    pub fn read(&mut self) -> Seen {
        let mut out = Seen::default();
        let Ok(mut file) = std::fs::File::open(&self.path) else { return out };
        let len = file.metadata().map_or(0, |m| m.len());
        if len < self.offset {
            // the file was replaced or cut: read it again from its window
            *self = Follower::new(&self.path);
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return out;
        }
        let mut buf = Vec::new();
        if file.take(len - self.offset).read_to_end(&mut buf).is_err() {
            return out;
        }
        self.offset += buf.len() as u64;
        out.grew = true;
        self.grew = Instant::now();
        let mut data = std::mem::take(&mut self.rest);
        data.extend_from_slice(&buf);
        let complete = data.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
        self.rest = data.split_off(complete);
        let mut lines = data.split(|b| *b == b'\n');
        if std::mem::take(&mut self.skip_partial) {
            lines.next(); // the window began inside a line
        }
        for line in lines.filter(|l| !l.is_empty()) {
            let Some(said) = classify(line) else { continue };
            out.seen.extend(said.ids);
            if said.model.is_some() {
                self.model = said.model;
            }
            for t in said.began {
                self.tools.insert(t.id.clone(), t);
                out.tools = true;
            }
            for id in said.answered {
                out.tools |= self.tools.remove(&id).is_some();
            }
            match said.mark {
                Some(Mark::Running) => {
                    #[derive(Deserialize)]
                    struct At {
                        #[serde(default)]
                        timestamp: String,
                    }
                    self.ended = false;
                    self.running_since = serde_json::from_slice::<At>(line).map(|a| a.timestamp).unwrap_or_default();
                }
                Some(Mark::Ended) => {
                    self.ended = true;
                    if !self.tools.is_empty() {
                        self.tools.clear();
                        out.tools = true;
                    }
                }
                None => {}
            }
        }
        out
    }
}

/// What a tool call is about, for its line in the agents' list: its command, file, pattern, description or prompt.
fn what(input: &serde_json::Value) -> String {
    ["command", "file_path", "pattern", "description", "prompt"].iter().find_map(|k| input.get(*k).and_then(|v| v.as_str())).unwrap_or("").chars().take(200).collect()
}

/// What entry LINE says ([`Said`]); None for an entry that says nothing (a system record other than `turn_duration`,
/// an attachment, a meta entry, a summary, a slash command's echo).
fn classify(line: &[u8]) -> Option<Said> {
    // most of a transcript is tool output: a cheap look first
    let head = &line[..line.len().min(400)];
    let has = |s: &[u8]| line.windows(s.len()).any(|w| w == s);
    let head_has = |s: &[u8]| head.windows(s.len()).any(|w| w == s);
    if !(head_has(b"\"type\":\"user\"") || head_has(b"\"type\":\"queue-operation\"") || head_has(b"\"type\":\"assistant\"") || has(b"\"subtype\":\"turn_duration\"") || has(b"\"type\":\"user\"") || has(b"\"type\":\"assistant\"")) {
        return None;
    }
    let e: Entry = serde_json::from_slice(line).ok()?;
    let ended = || Some(Said { mark: Some(Mark::Ended), ..Default::default() });
    match e.kind.as_str() {
        "queue-operation" if e.operation == "enqueue" => Some(Said { ids: mail_ids(e.content.as_deref().unwrap_or("").trim_start()), ..Default::default() }),
        "system" if e.subtype == "turn_duration" => ended(),
        "assistant" if e.api_error => ended(),
        "assistant" => {
            let last = e.message.as_ref().and_then(|m| m.stop_reason.as_deref()) == Some("end_turn");
            let began = match e.message.as_ref().map(|m| &m.content) {
                Some(serde_json::Value::Array(items)) => items
                    .iter()
                    .filter(|i| i["type"] == "tool_use")
                    .map(|i| Tool { id: i["id"].as_str().unwrap_or("").into(), tool: i["name"].as_str().unwrap_or("").into(), what: what(&i["input"]), at: e.timestamp.get(..19).map(|t| format!("{t}Z")).unwrap_or_default() })
                    .filter(|t| !t.id.is_empty())
                    .collect(),
                _ => Vec::new(),
            };
            Some(Said { mark: Some(if last { Mark::Ended } else { Mark::Running }), began, ..Default::default() })
        }
        "user" if e.compact => None,
        "user" => {
            let content = e.message.map(|m| m.content).unwrap_or_default();
            let answered: Vec<String> = match &content {
                serde_json::Value::Array(items) => items.iter().filter(|i| i["type"] == "tool_result").filter_map(|i| i["tool_use_id"].as_str().map(String::from)).collect(),
                _ => Vec::new(),
            };
            let text = match &content {
                serde_json::Value::String(s) => s.as_str(),
                serde_json::Value::Array(items) => items.iter().find_map(|i| (i["type"] == "text").then(|| i["text"].as_str()).flatten()).unwrap_or(""),
                _ => "",
            };
            let t = text.trim_start();
            if t.starts_with("[Request interrupted by user") {
                return ended();
            }
            // a channel message is a meta entry too (`isMeta`, `origin.kind: channel`): it starts a turn and carries mail
            let channel = t.starts_with("<channel source=");
            if let Some(m) = t.strip_prefix("<local-command-stdout>Set model to `").and_then(|r| r.split('`').next()).and_then(picked) {
                return Some(Said { model: Some((m, e.timestamp.clone())), ..Default::default() });
            }
            if (e.meta && !channel) || t.starts_with("<command-name>") || t.starts_with("<local-command") || t.starts_with("<command-message>") {
                return None;
            }
            Some(Said { mark: Some(Mark::Running), ids: mail_ids(t), answered, ..Default::default() })
        }
        _ => None,
    }
}

/// The model id of a `/model` pick's display NAME (`Set model to `Fable 5.2``): `claude-fable-5-2`, with `[1m]` for a
/// "(1M context)" pick. A name of no Claude family (a picker entry of the settings', `Kiro Opus 5.5`) is None: the
/// session's next reply names its model.
fn picked(name: &str) -> Option<String> {
    let (name, long) = match name.split_once(" (") {
        Some((n, rest)) => (n, rest.to_ascii_lowercase().contains("1m")),
        None => (name, false),
    };
    let mut words = name.split_whitespace();
    let (family, version) = (words.next()?.to_ascii_lowercase(), words.next()?);
    let known = ["fable", "mythos", "opus", "sonnet", "haiku"].contains(&family.as_str());
    let numeric = !version.is_empty() && version.chars().all(|c| c.is_ascii_digit() || c == '.');
    (known && numeric && words.next().is_none()).then(|| format!("claude-{family}-{}{}", version.replace('.', "-"), if long { "[1m]" } else { "" }))
}

/// The mail ids entry text T delivered: a coord channel entry's `id`, or each `[mN · from X]` of a hook's mail text.
fn mail_ids(t: &str) -> Vec<String> {
    if let Some(tag) = t.strip_prefix("<channel source=\"coord\"") {
        let tag = &tag[..tag.find('>').unwrap_or(tag.len())];
        return tag.split("id=\"").nth(1).and_then(|r| r.split('"').next()).map(|id| vec![id.to_string()]).unwrap_or_default();
    }
    if !t.starts_with("Mail that arrived for you (coord") && !t.starts_with("Stop hook feedback") {
        return Vec::new();
    }
    t.match_indices('[')
        .filter_map(|(i, _)| {
            let rest = &t[i + 1..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let ok = id.len() > 1 && (id.starts_with('m') || id.starts_with('q')) && id[1..].chars().all(|c| c.is_ascii_digit());
            (ok && rest[id.len()..].starts_with(" · from ")).then_some(id)
        })
        .collect()
}
