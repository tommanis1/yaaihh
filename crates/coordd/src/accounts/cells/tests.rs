//! Cells from a turn's rows and the telemetry, served from a cursor, made once from the month files, summed per account.

use super::*;
use crate::accounts::testing::Tmp;
use crate::accounts::usage::{append as append_line, Timing};

fn row(at: &str, agent: &str, account: &str, model: &str, output: u64) -> Row {
    Row { at: at.into(), agent: agent.into(), session: format!("s-{agent}"), account: account.into(), message_id: format!("m-{at}-{output}"), model: model.into(), input: 2, output, cache_read: 900, cache_write_5m: 50, cache_write_1h: 10, thinking: 1, ..Default::default() }
}

fn coord(t: &Tmp) -> String {
    let c = t.dir.join("research");
    std::fs::create_dir_all(c.join("usage")).unwrap();
    c.to_string_lossy().into_owned()
}

#[test]
fn a_turns_rows_are_kept_summed_by_five_minutes_and_key_and_served_from_a_cursor() {
    let t = Tmp::new("cells-rows");
    let c = coord(&t);
    let rows = [row("2026-11-26T08:00:01.000Z", "w1", "acct-a", "claude-opus-5-5", 5), row("2026-11-26T08:04:59.000Z", "w1", "acct-a", "claude-opus-5-5", 7), row("2026-11-26T08:05:00.000Z", "w1", "acct-a", "claude-opus-5-5", 1), row("2026-11-26T08:01:00.000Z", "g1", "go", "opencode-go/gemini-4-argon", 3)];
    let cells = of_rows(&c, &rows, Source::Native);
    assert_eq!(cells.len(), 3);
    let first = cells.iter().find(|x| x.key.agent == "w1").unwrap();
    assert_eq!((first.sums.requests, first.sums.output, first.sums.cache_read), (2, 12, 1800));
    assert_eq!((first.key.provider.as_str(), first.key.project.as_str(), first.key.source), ("Claude", "research", Source::Native));
    append(&c, &cells).unwrap();
    let a = served(&c, 0);
    assert_eq!((a.cells, a.restarted), (cells.clone(), false));
    assert!(served(&c, a.next).cells.is_empty(), "nothing past the cursor");
    append(&c, &of_rows(&c, &rows[..1], Source::Native)).unwrap();
    assert_eq!(served(&c, a.next).cells.len(), 1);
    assert!(served(&c, 1 << 40).restarted, "a cursor past the end: the file was made again");
    assert_eq!(all(&c).len(), 4);
}

#[test]
fn the_telemetrys_timings_and_failures_add_latency_and_counts_under_their_sessions_agent() {
    let t = Tmp::new("cells-telemetry");
    let c = coord(&t);
    let who = |s: &str| (s == "s1").then(|| ("w1".to_string(), "acct-a".to_string()));
    let timing = |at: &str, d: u64, ttft: Option<u64>| Line::Timing(Timing { at: at.into(), session: "s1".into(), model: "claude-opus-5-5".into(), duration_ms: Some(d), ttft_ms: ttft, ..Default::default() });
    let failed = |status: Option<u64>, exhausted: bool| Line::Failed(Failed { at: "2026-11-26T08:02:00.000Z".into(), session: "s1".into(), model: "claude-opus-5-5".into(), status, exhausted, ..Default::default() });
    let lost = Line::Timing(Timing { at: "2026-11-26T08:02:00.000Z".into(), session: "elsewhere".into(), duration_ms: Some(5), ..Default::default() });
    let cells = of_lines(&c, &[timing("2026-11-26T08:01:00.000Z", 1200, Some(400)), timing("2026-11-26T08:03:00.000Z", 800, None), failed(Some(529), false), failed(Some(500), false), failed(Some(529), true), lost], &who);
    assert_eq!(cells.len(), 1, "one agent, account and model in one five minutes; the unknown session's timing has no cell");
    let s = cells[0].sums;
    assert_eq!((s.requests, s.duration_ms, s.timed, s.ttft_ms, s.first_tokens, s.failed, s.overloaded), (0, 2000, 2, 400, 1, 2, 1));
    assert_eq!((cells[0].key.agent.as_str(), cells[0].key.account.as_str()), ("w1", "acct-a"));
}

#[test]
fn the_month_files_kept_before_cells_make_them_once() {
    let t = Tmp::new("cells-seed");
    let c = coord(&t);
    let mut r = row("2026-11-20T10:00:00.000Z", "w1", "acct-a", "claude-fable-5-2", 9);
    r.request_id = Some("req_1".into());
    append_line(&c, &Line::Request(r.clone())).unwrap();
    append_line(&c, &Line::Request(r.clone())).unwrap();
    append_line(&c, &Line::Timing(Timing { at: r.at.clone(), session: r.session.clone(), request_id: Some("req_1".into()), model: r.model.clone(), duration_ms: Some(700), ttft_ms: Some(200), ..Default::default() })).unwrap();
    append_line(&c, &Line::Failed(Failed { at: r.at.clone(), session: r.session.clone(), model: r.model.clone(), status: Some(529), ..Default::default() })).unwrap();
    assert_eq!(seed(&c).unwrap(), 1);
    let s = all(&c)[0].sums;
    assert_eq!((s.requests, s.output, s.duration_ms, s.timed, s.ttft_ms, s.failed, s.overloaded), (1, 9, 700, 1, 200, 1, 1), "a row kept twice counts once, as the month's reader joins it");
    assert_eq!(seed(&c).unwrap(), 0, "made once");
}

#[test]
fn a_gateway_agents_requests_are_gateway_cells_on_no_account_kept_off_the_accounts_page() {
    let t = Tmp::new("cells-gateway");
    let c = coord(&t);
    let gw = row("2026-11-26T08:00:01.000Z", "w2", "", "claude-opus-5-5", 5);
    append(&c, &of_rows(&c, &[gw], Source::Gateway)).unwrap();
    append(&c, &of_rows(&c, &[row("2026-11-26T08:00:02.000Z", "w1", "acct-a", "claude-opus-5-5", 3)], Source::Native)).unwrap();
    let cells = all(&c);
    let g = cells.iter().find(|x| x.key.agent == "w2").unwrap();
    assert_eq!((g.key.source, g.key.account.as_str(), g.key.project.as_str(), g.sums.output), (Source::Gateway, "", "research", 5));
    assert_eq!(by_account(&c, "2026-11").keys().collect::<Vec<_>>(), ["acct-a"], "the gateway's own records count its accounts");
}

#[test]
fn a_month_on_an_account_sums_its_cells_per_model() {
    let t = Tmp::new("cells-month");
    let c = coord(&t);
    let rows = [row("2026-11-18T09:43:22.920Z", "orch", "work", "claude-haiku-5-5", 76), row("2026-11-18T09:50:00.000Z", "orch", "work", "claude-haiku-5-5", 4), row("2026-12-01T00:00:01.000Z", "orch", "work", "claude-haiku-5-5", 1)];
    append(&c, &of_rows(&c, &rows, Source::Native)).unwrap();
    let who = |_: &str| Some(("orch".to_string(), "work".to_string()));
    append(&c, &of_lines(&c, &[Line::Timing(Timing { at: rows[0].at.clone(), session: "s".into(), model: "claude-haiku-5-5".into(), duration_ms: Some(1500), ..Default::default() })], &who)).unwrap();
    let by = by_account(&c, "2026-11");
    let u = &by["work"][0];
    assert_eq!((u.model.as_str(), u.requests, u.tokens.output, u.tokens.cache_write, u.cache_write_1h, u.tokens.reasoning, u.duration_ms, u.timed), ("claude-haiku-5-5", 2, 80, 120, 20, 2, 1500, 1));
    assert_eq!(by_account(&c, "2026-12")["work"][0].requests, 1);
    assert!(by_account(&c, "2026-10").is_empty());
}

#[test]
fn an_agy_turn_as_agy_rpc_writes_it_becomes_cells_on_its_account() {
    use serde_json::json;
    let t = Tmp::new("cells-agy");
    let c = coord(&t);
    let tr = t.dir.join("research/agents/n1/agy/coord/transcript.jsonl");
    std::fs::create_dir_all(tr.parent().unwrap()).unwrap();
    let at = "2026-11-26T08:00:00.000Z";
    let usage = |input: u64, cached: u64, output: u64| json!({"input_tokens": input, "cached_input_tokens": cached, "cache_write_input_tokens": 0, "output_tokens": output, "reasoning_output_tokens": 0, "total_tokens": input + output});
    let lines = [
        json!({"timestamp": at, "type": "turn_context", "payload": {"model": "gemini-3.8-flash"}}),
        json!({"timestamp": at, "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": usage(1000, 800, 40), "last_token_usage": usage(1000, 800, 40), "request": "r1"}}}),
        json!({"timestamp": at, "type": "event_msg", "payload": {"type": "token_count", "info": {"total_token_usage": usage(2500, 2000, 60), "last_token_usage": usage(1500, 1200, 20), "request": "r2"}}}),
    ];
    std::fs::write(&tr, lines.iter().map(|l| l.to_string() + "\n").collect::<String>()).unwrap();
    let rows = usage::turn_ended(&c, "n1", "thread:1", &tr.to_string_lossy(), "google-personal", Source::Native).unwrap();
    assert_eq!(rows.len(), 2);
    let cell = &all(&c)[0];
    assert_eq!((cell.key.provider.as_str(), cell.key.account.as_str(), cell.key.model.as_str()), ("agy", "google-personal", "gemini-3.8-flash"));
    assert_eq!((cell.sums.requests, cell.sums.input, cell.sums.cache_read, cell.sums.output), (2, 500, 2000, 60));
}
