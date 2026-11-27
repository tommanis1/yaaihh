//! The native usage as summed five-minute cells in `COORD/usage/cells.jsonl`: no transcript row copied, served from a cursor.

use super::usage::{self, Failed, Line, Row};
use coord_types::AccountUsage;
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Seek, SeekFrom};
use usage_rollup::{provider, Cell, Key, Source, Sums, STEPS};

/// The most bytes of cells one answer carries; the reader asks again from where it stopped.
pub const ANSWER_BYTES: u64 = 8 << 20;

pub fn path(coord: &str) -> String {
    format!("{coord}/usage/cells.jsonl")
}

/// The time of AT (RFC 3339) in milliseconds.
fn ms(at: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|t| t.timestamp_millis())
}

fn key(coord: &str, agent: &str, account: &str, model: &str, source: Source) -> Key {
    let project = std::path::Path::new(coord).file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
    Key { provider: provider(model), account: account.into(), source, model: model.into(), agent: agent.into(), project }
}

/// CELLS with each five minutes and key summed once.
fn summed(cells: impl IntoIterator<Item = Cell>) -> Vec<Cell> {
    let mut by: BTreeMap<(i64, Key), Sums> = BTreeMap::new();
    for c in cells {
        by.entry((c.at.div_euclid(STEPS[0]) * STEPS[0], c.key)).or_default().add(&c.sums);
    }
    by.into_iter().map(|((at, key), sums)| Cell { at, key, sums }).collect()
}

/// The cells of a turn's requests, counted from the harness's files of an agent on the owner's accounts or the gateway (SOURCE).
pub fn of_rows(coord: &str, rows: &[Row], source: Source) -> Vec<Cell> {
    summed(rows.iter().filter_map(|r| {
        let sums = Sums {
            requests: 1,
            input: r.input,
            output: r.output,
            cache_read: r.cache_read,
            cache_write_5m: r.cache_write_5m,
            cache_write_1h: r.cache_write_1h,
            thinking: r.thinking,
            ..Default::default()
        };
        Some(Cell { at: ms(&r.at)?, key: key(coord, &r.agent, &r.account, &r.model, source), sums })
    }))
}

/// The cells of the telemetry's LINES under their session's agent and account (WHO): latency, failures, 529s.
pub fn of_lines(coord: &str, lines: &[Line], who: &dyn Fn(&str) -> Option<(String, String)>) -> Vec<Cell> {
    summed(lines.iter().filter_map(|l| match l {
        Line::Timing(t) => {
            let (agent, account) = who(&t.session)?;
            Some(Cell { at: ms(&t.at)?, key: key(coord, &agent, &account, &t.model, Source::Native), sums: Sums::timed(t.duration_ms, t.ttft_ms) })
        }
        Line::Failed(f) => failed(coord, f, who(&f.session)),
        Line::Request(_) => None,
    }))
}

fn failed(coord: &str, f: &Failed, who: Option<(String, String)>) -> Option<Cell> {
    let (agent, account) = who.or_else(|| Some((f.agent.clone()?, f.account.clone()?)))?;
    let sums = Sums { failed: !f.exhausted as u64, overloaded: (!f.exhausted && f.status == Some(529)) as u64, ..Default::default() };
    Some(Cell { at: ms(&f.at)?, key: key(coord, &agent, &account, &f.model, Source::Native), sums })
}

/// Append CELLS to COORD's file, one line each.
pub fn append(coord: &str, cells: &[Cell]) -> Result<(), String> {
    let p = path(coord);
    cells.iter().try_for_each(|c| crate::jsonl::append(&p, &serde_json::to_string(c).unwrap_or_default()).map_err(|e| format!("{p}: {e}")))
}

/// The whole lines past byte AFTER, at most [`ANSWER_BYTES`]; a cursor past the end (a new file) starts at 0.
pub fn served(coord: &str, after: u64) -> coord_types::UsageCells {
    let p = path(coord);
    let Ok(mut f) = std::fs::File::open(&p) else { return Default::default() };
    let len = f.metadata().map_or(0, |m| m.len());
    let from = if after > len { 0 } else { after };
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(from)).is_err() || f.take(ANSWER_BYTES).read_to_end(&mut buf).is_err() {
        return Default::default();
    }
    let whole = buf.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let cells = buf[..whole].split(|&b| b == b'\n').filter_map(|l| serde_json::from_slice(l).ok()).collect();
    coord_types::UsageCells { cells, next: from + whole as u64, restarted: after > len }
}

/// Every cell of COORD's file.
pub fn all(coord: &str) -> Vec<Cell> {
    let (mut cells, mut at) = (Vec::new(), 0);
    loop {
        let a = served(coord, at);
        if a.next == at {
            return cells;
        }
        at = a.next;
        cells.extend(a.cells);
    }
}

/// The cells made once from the month files' lines kept before cells were; how many went in.
pub fn seed(coord: &str) -> Result<usize, String> {
    if std::path::Path::new(&path(coord)).exists() {
        return Ok(0);
    }
    let mut months: Vec<String> = std::fs::read_dir(format!("{coord}/usage")).map_err(|e| e.to_string())?.filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.ends_with(".jsonl") && n.len() == 13).collect();
    months.sort();
    let files: Vec<String> = months.iter().map(|m| format!("{coord}/usage/{m}")).collect();
    let m = usage::read(&files);
    let sessions: HashMap<&str, (String, String)> = m.rows.iter().map(|r| (r.session.as_str(), (r.agent.clone(), r.account.clone()))).collect();
    let mut cells = of_rows(coord, &m.rows, Source::Native);
    for r in &m.rows {
        let sums = Sums::timed(r.duration_ms, r.ttft_ms);
        cells.extend(ms(&r.at).filter(|_| sums.timed + sums.first_tokens > 0).map(|at| Cell { at, key: key(coord, &r.agent, &r.account, &r.model, Source::Native), sums }));
    }
    cells.extend(m.failed.iter().filter_map(|f| failed(coord, f, sessions.get(f.session.as_str()).cloned())));
    let cells = summed(cells);
    append(coord, &cells)?;
    Ok(cells.len())
}

/// This coord's usage on the owner's accounts in MONTH (`YYYY-MM`) by account and model, for the Accounts page; kept until the file grows.
pub fn by_account(coord: &str, month: &str) -> HashMap<String, Vec<AccountUsage>> {
    type Kept = Option<((String, String, u64), HashMap<String, Vec<AccountUsage>>)>;
    static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(None);
    let len = std::fs::metadata(path(coord)).map_or(0, |m| m.len());
    let mut kept = KEPT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, v)) = kept.as_ref().filter(|((c, m, l), _)| c == coord && m == month && *l == len) {
        return v.clone();
    }
    let start = chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").ok();
    let span = start.and_then(|d| Some((d.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis(), d.checked_add_months(chrono::Months::new(1))?.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())));
    let mut out: HashMap<String, BTreeMap<String, AccountUsage>> = HashMap::new();
    for c in all(coord).iter().filter(|c| c.key.source == Source::Native && span.is_some_and(|(a, b)| (a..b).contains(&c.at))) {
        let u = out.entry(c.key.account.clone()).or_default().entry(c.key.model.clone()).or_insert_with(|| AccountUsage { model: c.key.model.clone(), ..Default::default() });
        let s = &c.sums;
        u.requests += s.requests;
        u.failed += s.failed;
        u.tokens.input += s.input;
        u.tokens.output += s.output;
        u.tokens.cache_read += s.cache_read;
        u.tokens.cache_write += s.cache_write_5m + s.cache_write_1h;
        u.tokens.reasoning += s.thinking;
        u.cache_write_1h += s.cache_write_1h;
        u.duration_ms += s.duration_ms;
        u.timed += s.timed;
    }
    let v: HashMap<String, Vec<AccountUsage>> = out.into_iter().map(|(a, by)| (a, by.into_values().collect())).collect();
    *kept = Some(((coord.to_string(), month.to_string(), len), v.clone()));
    v
}

#[cfg(test)]
mod tests;
