//! One row per API response, with its agent and account, from what Claude Code writes itself: its transcript, and its
//! telemetry for what the transcript lacks.
//!
//! **The file.** `$COORD/usage/YYYY-MM.jsonl`, one JSON line each ([`Line`]), appended, never rewritten:
//! - `{"kind":"request",…}`: a [`Row`], in the files from before the cells only: a turn's rows are read from its
//!   transcripts (the session's and its subagents') on from where the last read stopped, and kept summed (`cells.rs`);
//! - `{"kind":"timing",…}`: a request's duration and time to first token as Claude Code's telemetry said them
//!   (`telemetry.rs`), written when they arrive;
//! - `{"kind":"failed",…}`: a failed request (telemetry's `api_error`, `api_retries_exhausted`); the transcript has none.
//!
//! **How a row gets its duration.** A request's row (at its turn's end) and its timing (within seconds) come from two
//! writers that do not wait for each other, so both are lines of their own and [`read`] joins them: by request id when
//! both carry one, else by session, model and the four token counts. A timing near a month's end may land in the next
//! month's file.
//!
//! **No cost in a row**: the app computes cost from tokens and its price table, and goes on doing so.
//!
//! **Never counted twice over a restart.** What was read of each transcript is kept in `usage/offsets.json` (the
//! reader's own state, `TranscriptUsage::kept`), saved before the read's cells are appended: sums cannot be told
//! apart once added, so a coordd stopped between the two loses that read's cells rather than adding them again.

use crate::transcript::{self, Ended, TranscriptUsage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// One API response of an agent, as its transcript has it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Row {
    /// When the response ended (RFC 3339, UTC, ms): the time of its latest transcript line.
    pub at: String,
    pub agent: String,
    pub session: String,
    /// The account the agent ran on when the lines were read (the caller knows it: the folder it started the agent with).
    pub account: String,
    /// The API's id of the request (`requestId`); None when the response carried no `request-id` header.
    pub request_id: Option<String>,
    /// The response's message id (`msg_…`; a Codex rollout's request, which has none, `codex-AT-PROMPT-OUTPUT`, its
    /// time and the session's cumulative tokens): one row per message.
    pub message_id: String,
    /// The model asked for (`requestedModel`) and the one that answered (`message.model`).
    pub model_asked: Option<String>,
    pub model: String,
    pub effort: Option<String>,
    pub service_tier: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// The cache write, by how long it lives (5 minutes, 1 hour).
    pub cache_write_5m: u64,
    pub cache_write_1h: u64,
    pub thinking: u64,
    pub stop_reason: Option<String>,
    /// A subagent made it: the name of its transcript under `SESSION/subagents/` (`agent-…`), or "" for a line of the
    /// session's own transcript marked `isSidechain`; None for the agent's own requests.
    pub subagent: Option<String>,
    /// From the telemetry, joined in by [`read`]; never written by [`turn_ended`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<u64>,
}

/// A request's timing, from telemetry's `api_request`, with what it is joined to its [`Row`] by.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    pub at: String,
    pub session: String,
    pub request_id: Option<String>,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// The whole cache write (5 m and 1 h).
    pub cache_write: u64,
    pub duration_ms: Option<u64>,
    pub ttft_ms: Option<u64>,
}

/// A failed request, from telemetry's `api_error`, which Claude Code sends once, when it gives up on a request (the
/// doc: "Intermediate retry attempts are not logged as separate events"; `attempt` says how many were made), or from
/// `api_retries_exhausted` (`exhausted`), which it sends beside that `api_error` when there was more than one attempt,
/// with their whole time. So failed requests are the lines with `exhausted` false; an `exhausted` line adds the time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Failed {
    pub at: String,
    pub session: String,
    /// The agent and account of the session when the record came, if coordd knew them then; else [`read`] fills them
    /// from a row of the same session.
    pub agent: Option<String>,
    pub account: Option<String>,
    pub request_id: Option<String>,
    pub model: String,
    /// The HTTP status; None for a failure without one (a connection that failed).
    pub status: Option<u64>,
    /// The error's message, cut to [`ERROR_CHARS`].
    pub error: String,
    /// How many attempts were made (`attempt`, `total_attempts`).
    pub attempt: Option<u64>,
    /// `api_error`: the request's time; `api_retries_exhausted`: all its attempts' time.
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub exhausted: bool,
}

/// How much of an error's message a [`Failed`] keeps.
pub const ERROR_CHARS: usize = 500;

/// One line of a month's file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Line {
    Request(Row),
    Timing(Timing),
    Failed(Failed),
}

/// The file of the month AT falls in (an RFC 3339 time, or its `YYYY-MM` start; the current month if AT is neither):
/// `COORD/usage/YYYY-MM.jsonl`.
pub fn month_file(coord: &str, at: &str) -> String {
    let month = at.get(..7).filter(|m| m.as_bytes()[4] == b'-').map(str::to_string).unwrap_or_else(|| chrono::Utc::now().format("%Y-%m").to_string());
    format!("{coord}/usage/{month}.jsonl")
}

/// Append L to its month's file under COORD.
pub(crate) fn append(coord: &str, l: &Line) -> Result<(), String> {
    let at = match l {
        Line::Request(r) => &r.at,
        Line::Timing(t) => &t.at,
        Line::Failed(f) => &f.at,
    };
    let path = month_file(coord, at);
    let json = serde_json::to_string(l).map_err(|e| format!("{path}: {e}"))?;
    crate::jsonl::append(&path, &json).map_err(|e| format!("{path}: {e}"))
}

/// What the reader keeps of each transcript across coordd's starts: `COORD/usage/offsets.json`.
fn offsets_path(coord: &str) -> String {
    format!("{coord}/usage/offsets.json")
}

/// AGENT's turn has ended (the `stop` and `failed` hooks): read on in the transcript TRANSCRIPT of SESSION and its
/// subagents' from where the last read stopped, whole lines only, and append one [`Row`] per response read, on ACCOUNT
/// (none through the gateway, SOURCE), as cells.
/// How many rows went in. Blocking (file reads and writes): call it off the core's lock.
///
/// Reads of different agents may come at once; they are taken one at a time under `usage/offsets.lock`, which also
/// keeps the owner's other coordds off this coord's file.
pub fn turn_ended(coord: &str, agent: &str, session: &str, transcript: &str, account: &str, source: usage_rollup::Source) -> Result<Vec<Row>, String> {
    let dir = format!("{coord}/usage");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{dir}: {e}"))?;
    let _lock = super::Lock::take(Path::new(&format!("{dir}/offsets.lock"))).map_err(|e| format!("{dir}/offsets.lock: {e}"))?;
    let path = offsets_path(coord);
    let mut kept: HashMap<String, TranscriptUsage> = super::read_json(Path::new(&path)).unwrap_or_default();
    kept.retain(|f, u| u.fits(f));
    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for file in transcript::session_files(transcript) {
        let subagent = (file != transcript).then(|| Path::new(&file).file_stem().map(|s| s.to_string_lossy().to_string())).flatten();
        let u = kept.entry(file.clone()).or_default();
        u.read(&file);
        for e in u.ended.drain(..) {
            let Some(r) = row(e, agent, session, account, subagent.clone()) else { continue };
            // a message read again in this read (a later line of it) counts at its latest line
            match seen.get(&r.message_id) {
                Some(&i) => rows[i] = r,
                None => {
                    seen.insert(r.message_id.clone(), rows.len());
                    rows.push(r);
                }
            }
        }
    }
    let keep: HashMap<String, TranscriptUsage> = kept.iter().map(|(p, u)| (p.clone(), u.kept())).collect();
    super::write_json(Path::new(&path), &keep).map_err(|e| format!("{path}: {e}"))?;
    super::cells::append(coord, &super::cells::of_rows(coord, &rows, source))?;
    Ok(rows)
}

/// The row of request E: a Claude one's with what its line says beyond its tokens (`Said`); one of a Codex rollout,
/// which says nothing more (an opencode agent's, as its follower writes it: the caller reads only the transcripts of
/// agents on the owner's accounts), with its model and tokens alone.
fn row(e: Ended, agent: &str, session: &str, account: &str, subagent: Option<String>) -> Option<Row> {
    let said = e.said.unwrap_or_default();
    let write_1h = e.cache.map_or(0, |c| c.write_1h);
    Some(Row {
        at: chrono::DateTime::from_timestamp_millis(e.at)?.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        agent: agent.to_string(),
        session: session.to_string(),
        account: account.to_string(),
        request_id: said.request_id,
        message_id: e.id,
        model_asked: said.requested_model,
        model: e.model,
        effort: said.effort,
        service_tier: said.service_tier,
        input: e.tokens.input,
        output: e.tokens.output,
        cache_read: e.tokens.cache_read,
        cache_write_5m: e.tokens.cache_write.saturating_sub(write_1h),
        cache_write_1h: write_1h,
        thinking: e.tokens.reasoning,
        stop_reason: said.stop_reason,
        subagent: subagent.or(said.sidechain.then(String::new)),
        duration_ms: None,
        ttft_ms: None,
    })
}

/// What a request is joined to its timing by when either lacks a request id: its session, model and tokens.
fn fingerprint(session: &str, model: &str, input: u64, output: u64, cache_read: u64, cache_write: u64) -> String {
    format!("{session}|{model}|{input}|{output}|{cache_read}|{cache_write}")
}

/// Rows and failed attempts of the files read, joined.
#[derive(Debug, Default, PartialEq)]
pub struct Month {
    pub rows: Vec<Row>,
    pub failed: Vec<Failed>,
}

/// The lines of FILES (a month's, and the next one's for the timings that landed there), joined: one row per message
/// id (the last), each with its timing; each failed attempt with the agent and account of its session, where a row
/// says them. A line that is not one (a line cut by a full disk) is skipped.
pub fn read(files: &[String]) -> Month {
    join(files.iter().filter_map(|f| std::fs::read_to_string(f).ok()).flat_map(|text| text.lines().filter_map(|l| serde_json::from_str::<Line>(l).ok()).collect::<Vec<_>>()))
}

/// LINES, in the order they landed, joined as [`read`] says.
fn join(lines: impl IntoIterator<Item = Line>) -> Month {
    let (mut rows, mut timings, mut failed) = (Vec::<Row>::new(), Vec::new(), Vec::new());
    let mut at: HashMap<String, usize> = HashMap::new();
    for l in lines {
        match l {
            Line::Request(r) => match at.get(&r.message_id) {
                Some(&i) => rows[i] = r,
                None => {
                    at.insert(r.message_id.clone(), rows.len());
                    rows.push(r);
                }
            },
            Line::Timing(t) => timings.push(t),
            Line::Failed(x) => failed.push(x),
        }
    }
    let by_id: HashMap<&str, &Timing> = timings.iter().filter_map(|t| Some((t.request_id.as_deref()?, t))).collect();
    let by_print: HashMap<String, &Timing> = timings.iter().map(|t| (fingerprint(&t.session, &t.model, t.input, t.output, t.cache_read, t.cache_write), t)).collect();
    for r in &mut rows {
        let print = fingerprint(&r.session, &r.model, r.input, r.output, r.cache_read, r.cache_write_5m + r.cache_write_1h);
        if let Some(t) = r.request_id.as_deref().and_then(|id| by_id.get(id)).or_else(|| by_print.get(&print)) {
            (r.duration_ms, r.ttft_ms) = (t.duration_ms, t.ttft_ms);
        }
    }
    let of_session: HashMap<&str, &Row> = rows.iter().map(|r| (r.session.as_str(), r)).collect();
    for x in &mut failed {
        if let Some(r) = of_session.get(x.session.as_str()) {
            x.agent.get_or_insert_with(|| r.agent.clone());
            x.account.get_or_insert_with(|| r.account.clone());
        }
    }
    Month { rows, failed }
}

/// Why a caller (OWNER: the owner) may not have the usage rows: they name every agent and account, so they are the
/// owner's window's alone; None for the owner.
pub fn refused(owner: bool) -> Option<String> {
    (!owner).then(|| "the usage: the owner's window only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::testing::Tmp;

    /// An assistant line of Claude Code's transcript, with only what the reader looks at and a short text.
    fn assistant(id: &str, at: &str, output: u64, w5: u64, w1: u64, request: Option<&str>, sidechain: bool) -> String {
        serde_json::json!({
            "type": "assistant", "timestamp": at, "sessionId": "s1", "isSidechain": sidechain, "requestedModel": "claude-haiku-5-5",
            "effort": "medium", "requestId": request, "uuid": format!("u-{id}-{output}"),
            "message": {"id": id, "model": "claude-haiku-5-5", "stop_reason": "end_turn", "content": [{"type": "text", "text": "ok"}],
                "usage": {"input_tokens": 2, "output_tokens": output, "cache_read_input_tokens": 11121, "cache_creation_input_tokens": w5 + w1,
                    "output_tokens_details": {"thinking_tokens": 3}, "service_tier": "standard",
                    "cache_creation": {"ephemeral_5m_input_tokens": w5, "ephemeral_1h_input_tokens": w1}}}
        })
        .to_string()
            + "\n"
    }

    fn user(at: &str) -> String {
        serde_json::json!({"type": "user", "timestamp": at, "sessionId": "s1", "message": {"role": "user", "content": "hi"}}).to_string() + "\n"
    }

    fn put(path: &Path, text: &str) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
    }


    #[test]
    fn one_row_per_response_with_its_agent_account_split_cache_and_subagent() {
        let t = Tmp::new("usage-rows");
        let coord = t.dir.join("coord").to_string_lossy().to_string();
        let tr = t.dir.join("projects/-w/s1.jsonl");
        // one response over two lines (two content blocks): one row, at its latest usage
        put(&tr, &(user("2026-11-18T09:43:20.000Z") + &assistant("msg_1", "2026-11-18T09:43:22.900Z", 10, 6000, 449, Some("req_1"), false)));
        put(&tr, &assistant("msg_1", "2026-11-18T09:43:22.920Z", 76, 6000, 449, Some("req_1"), false));
        put(&tr, &assistant("msg_2", "2026-11-18T09:43:24.700Z", 4, 107, 0, None, false));
        put(&t.dir.join("projects/-w/s1/subagents/agent-a1.jsonl"), &assistant("msg_3", "2026-11-18T09:43:30.000Z", 9, 0, 0, Some("req_3"), true));

        let rows = turn_ended(&coord, "orch", "s1", &tr.to_string_lossy(), "work", usage_rollup::Source::Native).unwrap();
        assert_eq!(rows.len(), 3);
        let r = &rows[0];
        assert_eq!(
            (r.agent.as_str(), r.account.as_str(), r.session.as_str(), r.message_id.as_str(), r.request_id.as_deref()),
            ("orch", "work", "s1", "msg_1", Some("req_1"))
        );
        assert_eq!((r.input, r.output, r.cache_read, r.cache_write_5m, r.cache_write_1h, r.thinking), (2, 76, 11121, 6000, 449, 3));
        assert_eq!(
            (r.model_asked.as_deref(), r.model.as_str(), r.effort.as_deref(), r.service_tier.as_str()),
            (Some("claude-haiku-5-5"), "claude-haiku-5-5", Some("medium"), "standard")
        );
        assert_eq!((r.stop_reason.as_deref(), r.subagent.as_deref(), r.at.as_str()), (Some("end_turn"), None, "2026-11-18T09:43:22.920Z"));
        assert_eq!(rows[1].request_id, None);
        assert_eq!(rows[2].subagent.as_deref(), Some("agent-a1"));
        // no row is copied: the transcript's requests are kept as summed cells only, with no cost
        assert!(!Path::new(&month_file(&coord, "2026-11")).exists());
        let text = std::fs::read_to_string(super::super::cells::path(&coord)).unwrap();
        assert_eq!(text.lines().count(), 1, "three requests of one agent, account and model in one five minutes");
        assert!(!text.contains("cost") && !text.contains("msg_1"), "{text}");
    }

    /// An opencode agent's requests as its follower writes them (agent-base/bin/opencode-rpc `step`: a Codex
    /// rollout's `turn_context` and `token_count`): a row each on its account, with its model, the cached input apart
    /// and the reasoning in the output and as thinking, as a Claude row has them. Two written in one millisecond are
    /// two rows; a count repeated unchanged is none.
    #[test]
    fn an_opencode_agents_requests_written_as_a_codex_rollouts_give_a_row_each_on_its_account() {
        use serde_json::json;
        let t = Tmp::new("usage-opencode");
        let coord = t.dir.join("coord").to_string_lossy().to_string();
        let tr = t.dir.join("coord/agents/oc1/opencode/coord/transcript.jsonl");
        let count = |at: &str, total: &serde_json::Value, last: &serde_json::Value| {
            json!({"timestamp": at, "type": "event_msg", "payload": {"type": "token_count", "part": "prt_1", "info": {"total_token_usage": total, "last_token_usage": last}}}).to_string() + "\n"
        };
        let tokens = |input: u64, cached: u64, written: u64, output: u64, reasoning: u64| {
            json!({"input_tokens": input, "cached_input_tokens": cached, "cache_write_input_tokens": written, "output_tokens": output, "reasoning_output_tokens": reasoning, "total_tokens": input + output})
        };
        let (one, two, both) = (tokens(1300, 1000, 200, 50, 10), tokens(1305, 1300, 0, 20, 0), tokens(2605, 2300, 200, 70, 10));
        let at = "2026-11-20T18:00:00.000Z";
        let model = json!({"timestamp": at, "type": "turn_context", "payload": {"model": "anthropic/claude-sonnet-5-5"}}).to_string() + "\n";
        put(&tr, &(model + &count(at, &one, &one) + &count(at, &both, &two) + &count("2026-11-20T18:00:01.000Z", &both, &json!({}))));

        let rows = turn_ended(&coord, "oc1", "ses_1", &tr.to_string_lossy(), "go", usage_rollup::Source::Native).unwrap();
        assert_eq!(rows.len(), 2);
        let of = |r: &Row| (r.input, r.output, r.cache_read, r.cache_write_5m, r.cache_write_1h, r.thinking);
        assert_eq!(rows.iter().map(of).collect::<Vec<_>>(), [(100, 50, 1000, 200, 0, 10), (5, 20, 1300, 0, 0, 0)]);
        assert!(rows.iter().all(|r| (r.agent.as_str(), r.account.as_str(), r.session.as_str(), r.model.as_str(), r.at.as_str()) == ("oc1", "go", "ses_1", "anthropic/claude-sonnet-5-5", at)));
        assert!(rows.iter().all(|r| r.request_id.is_none() && r.model_asked.is_none() && r.service_tier.is_empty() && r.subagent.is_none()));
        assert_ne!(rows[0].message_id, rows[1].message_id);
        assert!(turn_ended(&coord, "oc1", "ses_1", &tr.to_string_lossy(), "go", usage_rollup::Source::Native).unwrap().is_empty(), "nothing new: nothing again");
    }

    #[test]
    fn reading_goes_on_from_where_it_stopped_across_a_restart_and_waits_for_a_whole_line() {
        let t = Tmp::new("usage-offsets");
        let coord = t.dir.join("coord").to_string_lossy().to_string();
        let tr = t.dir.join("projects/-w/s1.jsonl");
        let p = tr.to_string_lossy().to_string();
        put(&tr, &assistant("msg_1", "2026-11-18T09:00:00.000Z", 5, 0, 0, None, false));
        let first = turn_ended(&coord, "orch", "s1", &p, "a", usage_rollup::Source::Native).unwrap();
        assert_eq!(first.len(), 1);
        assert!(turn_ended(&coord, "orch", "s1", &p, "a", usage_rollup::Source::Native).unwrap().is_empty(), "nothing new: nothing again");
        // a line still being written is not read
        let next = assistant("msg_2", "2026-11-18T09:01:00.000Z", 6, 0, 0, None, false);
        let (head, tail) = next.split_at(40);
        put(&tr, head);
        assert!(turn_ended(&coord, "orch", "s1", &p, "a", usage_rollup::Source::Native).unwrap().is_empty());
        put(&tr, tail);
        // nothing is kept in memory between calls, as over a restart: the offsets on file say where to go on, and the
        // new row is on the account the agent runs on now
        assert!(Path::new(&offsets_path(&coord)).is_file());
        let rows = [first, turn_ended(&coord, "orch", "s1", &p, "b", usage_rollup::Source::Native).unwrap()].concat();
        assert_eq!(rows.iter().map(|r| (r.message_id.as_str(), r.account.as_str())).collect::<Vec<_>>(), [("msg_1", "a"), ("msg_2", "b")]);
    }

    #[test]
    fn a_timing_meets_its_row_by_request_id_or_by_tokens_whichever_came_first_and_a_failure_gets_its_agent() {
        let t = Tmp::new("usage-join");
        let coord = t.dir.join("coord").to_string_lossy().to_string();
        let timing = |id: Option<&str>, output: u64, d: u64| {
            Line::Timing(Timing {
                at: "2026-11-18T09:43:23.004Z".into(),
                session: "s1".into(),
                request_id: id.map(str::to_string),
                model: "claude-haiku-5-5".into(),
                input: 2,
                output,
                cache_read: 11121,
                cache_write: 6449,
                duration_ms: Some(d),
                ttft_ms: Some(d / 2),
            })
        };
        // the first timing before its row (its tokens differ: the request id joins them), the second after
        append(&coord, &timing(Some("req_1"), 999, 2000)).unwrap();
        let tr = t.dir.join("projects/-w/s1.jsonl");
        put(&tr, &assistant("msg_1", "2026-11-18T09:43:22.920Z", 76, 6449, 0, Some("req_1"), false));
        put(&tr, &assistant("msg_2", "2026-11-18T09:43:24.700Z", 4, 6000, 449, None, false));
        for r in turn_ended(&coord, "orch", "s1", &tr.to_string_lossy(), "work", usage_rollup::Source::Native).unwrap() {
            append(&coord, &Line::Request(r)).unwrap();
        }
        append(&coord, &timing(None, 4, 1468)).unwrap();
        let failed = Failed { at: "2026-11-18T09:44:00.000Z".into(), session: "s1".into(), status: Some(529), error: "overloaded".into(), attempt: Some(1), ..Default::default() };
        append(&coord, &Line::Failed(failed)).unwrap();

        let m = read(&[month_file(&coord, "2026-11")]);
        assert_eq!(m.rows.iter().map(|r| (r.duration_ms, r.ttft_ms)).collect::<Vec<_>>(), [(Some(2000), Some(1000)), (Some(1468), Some(734))]);
        assert_eq!((m.failed[0].agent.as_deref(), m.failed[0].account.as_deref(), m.failed[0].status), (Some("orch"), Some("work"), Some(529)));
    }
}
