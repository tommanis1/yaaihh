//! What Claude Code says of an account's 5-hour and weekly windows, and that the account is out, read only. Three
//! sources, all made by Claude Code from what the API answered it:
//!
//! - the status-line command's input ([`from_statusline`]): `rate_limits.five_hour` and `.seven_day`, each
//!   `used_percentage` (0 to 100) and `resets_at` (Unix seconds). It "appears only for claude.ai Pro, Max, and Ultra
//!   subscribers … only after the first API response in the session", and "Claude Code drops a window once its
//!   `resets_at` time passes" (docs `statusline.md`);
//! - the `StopFailure` hook's input, coordd's `failed` event ([`from_failed`]): `error` (`rate_limit`, …), optional
//!   `error_details` ("429 Too Many Requests") and `last_assistant_message` (the error text shown, "API Error: Rate
//!   limit reached"). It names no reset time (docs `hooks.md`);
//! - the `Notification` hook's quota types ([`from_notification`]): `quota_auto_resume_fired` (Claude Code went on after
//!   a usage limit), `quota_auto_resume_stale` (the limit reset during a long sleep), `quota_auto_resume_disabled` (a
//!   wait at a limit ended without going on), with `message` and an optional `title`; no reset time either.
//!
//! Beside them, what Claude Code says when asked: a limited turn's row in its transcript ([`row_of`]), the
//! `rate_limit_event` of the window opener's message in print mode ([`from_event`]), and its `get_usage` on the owner's
//! press ([`from_usage`]). Each becomes a [`Said`], and [`apply`] puts it into the account's [`State`].
//!
//! An OpenCode Go account has a third window, its month, read from Go's usage page with the key kept for it
//! (accounts/opencode.rs `go_usage`): `from` is [`GO_USAGE`] then.

use super::{State, Window};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// `out_until` when an account is out and nothing said until when: it stays out until a later signal says otherwise or it
/// is cleared ([`reset_unknown`] shows it). A time no clock reaches.
pub const UNKNOWN_RESET: i64 = i64::MAX;

/// Whether STATE's `out_until` means "out, reset unknown".
pub fn reset_unknown(state: &State) -> bool {
    state.out_until == Some(UNKNOWN_RESET)
}

/// Why an account is out, as the `StopFailure` hook's `error` names it ([`State::out_for`]): the plan's or the key's
/// limit was met; the key was refused, or a sign-in's login to two turns ([`apply`]); the key's organisation has no
/// credit.
pub const RATE_LIMIT: &str = "rate_limit";
pub const REFUSED: &str = "authentication_failed";
pub const NO_CREDIT: &str = "billing_error";

/// Whether an account out for WHY comes back only when the owner acts (`coord account clear ID`, a new key, a
/// sign-in): a refused key or login and low credit do not mend by themselves.
pub fn owners_to_clear(why: &str) -> bool {
    why == REFUSED || why == NO_CREDIT
}

/// What one signal says of an account.
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    /// The windows it gave; None: it says nothing of that window.
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    /// The month's window: OpenCode Go's usage page alone names one ([`GO_USAGE`]).
    pub month: Option<Window>,
    /// Its weeks of one model, when the signal names them (`get_usage` alone): shown on the card, never a verdict.
    pub model_weeks: Option<Vec<coord_types::ModelWeek>>,
    /// Whether the account has a 5-hour limit, when the signal says so beside its windows (`get_usage`, a
    /// `rate_limit_event` with its `unifiedWindows`); a 5-hour window given says it has one all the same ([`apply`]).
    pub five_hour_limit: Option<bool>,
    /// Whether its requests run on extra usage, when the signal says (`get_usage` alone, [`from_usage`]): Some(Some(T)),
    /// on it until T; Some(None), not on it; None, the signal says nothing of it. Shown on the card, never a verdict.
    pub extra_usage: Option<Option<i64>>,
    /// Whether the account can take requests now, by this signal.
    pub verdict: Verdict,
    /// Why it is out, when the signal says ([`RATE_LIMIT`], [`REFUSED`], [`NO_CREDIT`]), and the words Claude
    /// Code showed for it (the hook's `last_assistant_message`; opencode's error message, `error_details`), for the
    /// card.
    pub out_for: Option<&'static str>,
    pub words: Option<String>,
    /// What it says of one agent's session on the account beside its limits, if anything.
    pub session: Option<Session>,
    /// The one model it is said of, when it is that model's alone ([`State::models_out`]): an agy turn of it refused by
    /// its limit (`Core::bound_said`), out for that model and not the account; the policy's word that it is taken
    /// again ([`Said::again_model`]). None: the account's.
    pub model: Option<String>,
    /// Which source: "statusline", "usage", "rate_limit_event", "failed", "notification", "transcript", "policy",
    /// "stop", "delivery", [`GO_USAGE`].
    pub from: &'static str,
}

/// [`Said::from`] for OpenCode Go's usage page (accounts/opencode.rs `go_usage`): as `usage`, a reading of every window
/// the account has, so a window it does not name is kept only until its reset ([`apply`]).
pub const GO_USAGE: &str = "go usage";

/// What a signal says of one agent's session on an account, beside the account's limits ([`apply`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Session {
    /// A sign-in account's login refused this agent's turn ([`from_failed`]).
    Refused(String),
    /// A turn ended answered ([`Said::answered`]).
    Answered,
    /// A delivery handed to it was seen in the session ([`Said::reached`]).
    Reached,
    /// A delivery handed to this agent was never seen in its session ([`Said::unseen`]).
    Unseen(String),
}

impl Said {
    /// A signal from FROM that says VERDICT and nothing else.
    pub(super) fn of(verdict: Verdict, from: &'static str) -> Said {
        Said { five_hour: None, seven_day: None, month: None, model_weeks: None, five_hour_limit: None, extra_usage: None, verdict, out_for: None, words: None, session: None, model: None, from }
    }

    /// The policy's word, not a signal of Claude Code's: a key account out for its rate limit with no time named
    /// is taken again ([`again_due`]).
    pub fn again() -> Said {
        Said::of(Verdict::In, "policy")
    }

    /// The policy's word that an agy account out for MODEL with no time named takes it again ([`model_again_at`]).
    pub fn again_model(model: &str) -> Said {
        Said { model: Some(model.to_string()), ..Said::again() }
    }

    /// The `Stop` hook: a turn on the account ended answered. It says nothing of the limits; a refused turn before
    /// it was that session's ([`State::refused`]).
    pub fn answered() -> Said {
        Said { session: Some(Session::Answered), ..Said::of(Verdict::Silent, "stop") }
    }

    /// A delivery was seen in a session on the account, its line for it (`Core::ticket_seen`): messages reach it.
    pub fn reached() -> Said {
        Said { session: Some(Session::Reached), ..Said::of(Verdict::Silent, "delivery") }
    }

    /// A delivery handed to AGENT on the account was never seen in its session (`Core::not_read`).
    pub fn unseen(agent: &str) -> Said {
        Said { session: Some(Session::Unseen(agent.to_string())), ..Said::of(Verdict::Silent, "delivery") }
    }
}

/// What a signal says of the account's being out.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// It is out, until then (seconds) if the signal or a known window says when.
    Out(Option<i64>),
    /// It can take requests: an answer came through, or a limit has reset.
    In,
    /// It says nothing of it.
    Silent,
}

/// The status-line input (statusline.md, "Rate limit usage"). Windows at 100 per cent make the account out until the
/// latest of their resets; windows all below 100 say it is in (the input follows an answered request). Without
/// `rate_limits` (not a subscription, or no answer yet this session) it says nothing.
pub fn from_statusline(input: &Value) -> Said {
    let five_hour = window(&input["rate_limits"]["five_hour"]);
    let seven_day = window(&input["rate_limits"]["seven_day"]);
    let full: Vec<i64> = [&five_hour, &seven_day].into_iter().flatten().filter(|w| w.used_percent >= 100.0).map(|w| w.resets_at).collect();
    let verdict = match (full.iter().max(), five_hour.is_some() || seven_day.is_some()) {
        (Some(&until), _) => Verdict::Out(Some(until)),
        (None, true) => Verdict::In,
        (None, false) => Verdict::Silent,
    };
    Said { five_hour, seven_day, out_for: matches!(verdict, Verdict::Out(_)).then_some(RATE_LIMIT), ..Said::of(verdict, "statusline") }
}

/// What Claude Code's own `get_usage` answered of a sign-in account ([`super::opener::read`] asks it): the
/// structured data its `/usage` prints from (2.1.330's `S`, "Requests the structured /usage data … Experimental — the
/// response shape may change"). Claude Code fetches it with its own login from its usage endpoint; nothing goes to a
/// model, and nothing of the login comes back. Its `rate_limits` holds each window as `utilization` (0 to 100) and
/// `resets_at` (ISO 8601, null while nothing was used of it yet: then no window here), as `/usage` shows them:
/// - `five_hour`, `Current session`: given at all, the account has a 5-hour limit ([`Said::five_hour_limit`]);
/// - `seven_day`, `Current week (all models)`;
/// - the weeks of one model, for the card ([`Said::model_weeks`]) and never a verdict: `seven_day_sonnet` named
///   `Sonnet only` on a `max` or `team` plan or none named (as `/usage` shows it), then each `model_scoped` by its
///   `display_name` (`Fable`). One whose reset cannot be read is left out alone.
///
/// Its `extra_usage` says whether extra usage is on and how much of its monthly limit is used: past a window's limit
/// with it on and credit left, the requests run on extra usage until the latest reset of a window used up
/// ([`Said::extra_usage`]); otherwise they do not. `/usage`'s "You are currently using your overages" line, read
/// here before, comes from the headers of a model's answer in that same process, which a read sends none of.
///
/// The windows as the status line gives them (`from: "usage"`, at 100 per cent out until the latest reset, else in).
/// Err with what it said when it gives neither of the account's windows (`rate_limits_available` false: a key, a
/// provider, no profile scope; a failed fetch), or the reset of one of them could not be read: nothing is taken from it
/// then.
pub fn from_usage(answer: &Value) -> Result<Said, String> {
    let limits = &answer["rate_limits"];
    if answer["rate_limits_available"] != Value::Bool(true) || !limits.is_object() {
        let plan = answer["subscription_type"].as_str().unwrap_or("none");
        return Err(format!("get_usage said no plan limits apply to it (plan {plan})"));
    }
    // one window: how much of it was used, and its reset when it says one; None when it says nothing of it
    let read = |w: &Value| -> Result<Option<(f64, Option<i64>)>, String> {
        let Some(used) = w["utilization"].as_f64() else { return Ok(None) };
        match w["resets_at"].as_str() {
            None => Ok(Some((used, None))),
            Some(t) => chrono::DateTime::parse_from_rfc3339(t).map(|t| Some((used, Some(t.timestamp())))).map_err(|_| format!("get_usage: a reset it said could not be read: {t}")),
        }
    };
    let (five, seven) = (read(&limits["five_hour"])?, read(&limits["seven_day"])?);
    if five.is_none() && seven.is_none() {
        return Err(format!("get_usage said no window: {}", limits.to_string().chars().take(200).collect::<String>()));
    }
    let window = |r: Option<(f64, Option<i64>)>| r.and_then(|(used_percent, resets_at)| Some(Window { used_percent, resets_at: resets_at? }));
    let (five_hour, seven_day) = (window(five), window(seven));
    let sonnet = matches!(answer["subscription_type"].as_str(), Some("max" | "team") | None).then(|| ("Sonnet only".to_string(), &limits["seven_day_sonnet"]));
    let scoped = limits["model_scoped"].as_array().into_iter().flatten().filter_map(|m| Some((m["display_name"].as_str()?.to_string(), m)));
    let model_weeks = sonnet
        .into_iter()
        .chain(scoped)
        .filter_map(|(model, w)| Some(coord_types::ModelWeek { model, window: window(read(w).ok()?)? }))
        .collect();
    let full = [&five_hour, &seven_day].into_iter().flatten().filter(|w| w.used_percent >= 100.0).map(|w| w.resets_at).max();
    let verdict = full.map_or(Verdict::In, |until| Verdict::Out(Some(until)));
    let extra = &limits["extra_usage"];
    let credit = match (extra["used_credits"].as_f64(), extra["monthly_limit"].as_f64()) {
        (Some(used), Some(limit)) => used < limit,
        _ => !extra["utilization"].as_f64().is_some_and(|u| u >= 100.0),
    };
    let extra_usage = Some(full.filter(|_| extra["is_enabled"] == Value::Bool(true) && credit));
    Ok(Said { five_hour, seven_day, model_weeks: Some(model_weeks), five_hour_limit: Some(five.is_some()), extra_usage, out_for: full.map(|_| RATE_LIMIT), ..Said::of(verdict, "usage") })
}

/// The `rate_limit_info` of a print-mode run's `rate_limit_event` (2.1.329's stream-json: `Fwa` sends it once an
/// answer brought the limits, none when nothing did). Its `unifiedWindows` holds the 5-hour and weekly windows, each
/// `utilization` (a fraction, past 1 when used past its cap) and `resetsAt` (seconds), "present only for accounts
/// whose responses carry that window": with it, whether a 5-hour one is there says whether the account has a 5-hour
/// limit. Without it, the one window its top level names ([`named`]) when it says how much of it was used. Its
/// top-level `status` says whether requests are refused ("A rejected window alone does not mean requests are refused;
/// the top-level status says whether they are"): `rejected` is out until the reset it names, anything else in.
pub fn from_event(info: &Value) -> Said {
    let used = |w: &Value| Some(Window { used_percent: w["utilization"].as_f64()? * 100.0, resets_at: w["resetsAt"].as_f64()? as i64 });
    let (all, claim) = (&info["unifiedWindows"], named(info));
    let top = |kind: &str| claim.filter(|(k, _)| *k == kind).and_then(|_| used(info));
    let five_hour = used(&all["five_hour"]).or_else(|| top("five_hour"));
    let seven_day = used(&all["seven_day"]).or_else(|| top("seven_day"));
    let five_hour_limit = all.is_object().then_some(five_hour.is_some());
    let (verdict, out_for) = match info["status"].as_str() {
        Some("rejected") => (Verdict::Out(claim.map(|(_, t)| t)), Some(RATE_LIMIT)),
        _ => (Verdict::In, None),
    };
    Said { five_hour, seven_day, five_hour_limit, out_for, ..Said::of(verdict, "rate_limit_event") }
}

/// The window Claude Code's `rate_limit_info` INFO names (`rateLimitType`, "" when it names none) and when that resets
/// (`resetsAt`, seconds), as a limited turn's row ([`row_of`]: its `quotaLimits` and `apiErrorParams.rate_limit_info`)
/// and a print-mode run's `rate_limit_event` ([`from_event`]) carry it; None with no reset.
fn named(info: &Value) -> Option<(&str, i64)> {
    Some((info["rateLimitType"].as_str().unwrap_or_default(), info["resetsAt"].as_f64()? as i64))
}

/// The day Claude Code's short date MONTH_DAY names in YEAR (`Nov 20`: its `toLocaleString("en-US", {month: "short",
/// day: "numeric"})`): a reset's "use by" in resets.rs.
pub(super) fn short_date(month_day: &str, year: i32) -> Option<chrono::NaiveDate> {
    let (month, day) = month_day.split_once(' ')?;
    let month = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|m| *m == month)? as u32 + 1;
    chrono::NaiveDate::from_ymd_opt(year, month, day.parse().ok()?)
}

/// The `StopFailure` input (coordd's `failed` event) of a session on an account that runs on a key (KEY) or on a
/// sign-in. A turn that ended on `rate_limit` makes the account out; Claude Code's signal names no time, so [`apply`]
/// takes a known window's reset, or none. An opencode agent's names one when opencode had it (`resets_at`, seconds:
/// opencode-rpc's `failure`, from its provider's `retry-after`), and that is the time. A server throttle ("Server is
/// temporarily limiting requests", unrelated to the plan's quota, docs `errors.md`) says nothing of the account's
/// limits.
///
/// For a key account two more errors are the account's and not the work's (tried in the rig on 18 Nov, Claude Code
/// 2.1.327; design-api-key-accounts.md section 4): `authentication_failed` (the API answered 401 or 403 through all
/// of Claude Code's own retries: the key is refused) and `billing_error` (400 "Your credit balance is too low"). Both
/// make it out with no time, for the owner to clear. On a sign-in account `authentication_failed` is the login's
/// refusal of AGENT's turn, which [`apply`] counts: a second one with no answered turn between makes it out; what the
/// login is now is `claude auth status`'s to say (mover.rs). `billing_error` says nothing there.
///
/// Every other error says nothing.
pub fn from_failed(input: &Value, key: bool, agent: &str) -> Said {
    let text = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or("");
    let words = format!("{} {}", text("error_details"), text("last_assistant_message")).to_lowercase();
    let out_for = match text("error") {
        "rate_limit" if !words.contains("temporarily limiting") => Some(RATE_LIMIT),
        "authentication_failed" if key => Some(REFUSED),
        "billing_error" if key => Some(NO_CREDIT),
        _ => None,
    };
    let shown = Some(text("last_assistant_message").trim()).filter(|w| !w.is_empty()).map(String::from);
    let until = input.get("resets_at").and_then(Value::as_i64).filter(|_| out_for == Some(RATE_LIMIT));
    match out_for {
        Some(why) => Said { out_for: Some(why), words: shown, ..Said::of(Verdict::Out(until), "failed") },
        None if text("error") == "authentication_failed" => Said { session: Some(Session::Refused(agent.to_string())), words: shown, ..Said::of(Verdict::Silent, "failed") },
        None => Said::of(Verdict::Silent, "failed"),
    }
}

/// The seconds a refused request's WORDS name until its limit resets: the gateway's `(reset after 18m48s)` or a Go
/// duration (`1h2m3s`, `30s`, `1.5s`, `500ms`), rounded up to a whole second; none when they name none.
pub fn reset_after(words: &str) -> Option<i64> {
    let (_, rest) = words.split_once("reset after ")?;
    let span = rest.split(|c: char| !(c.is_ascii_alphanumeric() || c == '.')).next()?.trim_end_matches('.');
    let (mut secs, mut num, mut any) = (0f64, String::new(), false);
    let mut chars = span.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let n: f64 = std::mem::take(&mut num).parse().ok()?;
        secs += n * match c {
            'h' => 3600.0,
            'm' if chars.peek() == Some(&'s') => {
                chars.next();
                0.001
            }
            'm' => 60.0,
            's' => 1.0,
            _ => return None,
        };
        any = true;
    }
    (any && num.is_empty()).then(|| secs.ceil() as i64)
}

/// The seconds from NOW until the reset Codex's WORDS name at its usage limit, on this machine's clock: a day and a
/// time (`… or try again at Nov 27th, 2026 11:52 AM.`), or a time alone (`try again at 3:04 PM.`: today, or tomorrow
/// once it has passed today). None when they name none, or a day and time already past.
pub fn try_again_at(words: &str, now: i64) -> Option<i64> {
    use chrono::TimeZone;
    let (_, rest) = words.split_once("try again at ")?;
    let mut parts: Vec<&str> = rest.split_whitespace().take(5).collect();
    let half = parts.iter().position(|p| matches!(p.trim_end_matches('.'), "AM" | "PM"))?;
    parts.truncate(half + 1);
    let h = half.checked_sub(1)?;
    let time = chrono::NaiveTime::parse_from_str(&format!("{} {}", parts[h], parts[half].trim_end_matches('.')), "%I:%M %p").ok()?;
    let local = |d: chrono::NaiveDate| Some(chrono::Local.from_local_datetime(&d.and_time(time)).earliest()?.timestamp());
    let at = match parts[..h] {
        [] => {
            let today = chrono::Local.timestamp_opt(now, 0).single()?.date_naive();
            let t = local(today)?;
            if t > now { t } else { local(today.succ_opt()?)? }
        }
        [month, day, year] => local(short_date(&format!("{month} {}", day.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == ',')), year.parse().ok()?)?)?,
        _ => return None,
    };
    (at > now).then_some(at - now)
}

/// The `Notification` input: `quota_auto_resume_fired` and `quota_auto_resume_stale` say the limit has reset (in);
/// `quota_auto_resume_disabled` says a wait at a limit ended without going on, so the account met its limit (out,
/// no time given); every other type says nothing.
pub fn from_notification(input: &Value) -> Said {
    let verdict = match input.get("notification_type").and_then(Value::as_str).unwrap_or("") {
        "quota_auto_resume_fired" | "quota_auto_resume_stale" => Verdict::In,
        "quota_auto_resume_disabled" => Verdict::Out(None),
        _ => Verdict::Silent,
    };
    Said { out_for: matches!(verdict, Verdict::Out(_)).then_some(RATE_LIMIT), ..Said::of(verdict, "notification") }
}

/// What the transcript's row of a refused request says ([`row_of`]), as `Core::account_said` is handed it (event
/// `limit_row`, `resets_at` in seconds): the account is out until the reset the row names. A failed turn's hook names
/// no time ([`from_failed`]); this is the time, from what Claude Code wrote itself.
///
/// For a key account (KEY) the row can also say what the hook left unnamed: the API's 402 `billing_error`, for which
/// the hook's `error` is `unknown` (the rig, 18 Nov). Out with no time, for the owner to clear, as [`from_failed`]'s
/// `billing_error` is.
pub fn from_row(input: &Value, key: bool) -> Said {
    if let Some(until) = input.get("resets_at").and_then(Value::as_i64) {
        return Said { out_for: Some(RATE_LIMIT), ..Said::of(Verdict::Out(Some(until)), "transcript") };
    }
    if key && input["error"] == NO_CREDIT {
        let words = input["words"].as_str().map(str::trim).filter(|w| !w.is_empty()).map(String::from);
        return Said { out_for: Some(NO_CREDIT), words, ..Said::of(Verdict::Out(None), "transcript") };
    }
    Said::of(Verdict::Silent, "transcript")
}

/// How far back from a transcript's end [`row_of`] looks for the last assistant row.
const TAIL: u64 = 1 << 20;

/// The transcript at PATH when it is a session's `.jsonl` under CONFIG's `projects`, links followed (a path with `..`
/// or a link leading elsewhere is not). PATH comes from a hook's input, the agent's side: coordd opens no other file.
fn transcript_in(config: &Path, path: &str) -> Option<PathBuf> {
    let real = Path::new(path).canonicalize().ok()?;
    (real.starts_with(config.join("projects").canonicalize().ok()?) && real.extension().is_some_and(|e| e == "jsonl")).then_some(real)
}

/// What the last assistant row of the transcript at PATH says of the account ([`from_row`]): `resets_at` and `window`
/// when the plan's limit refused the request; `error` [`NO_CREDIT`] with its `words` on `billing_error`. The row must be
/// written at or after SINCE (ms, the process's start), so an earlier account's row is not taken. Claude Code writes it
/// with `isApiErrorMessage: true`, `error: "rate_limit"` and `quotaLimits` (`resetsAt`, `rateLimitType`). None when PATH
/// is no transcript of CONFIG ([`transcript_in`]), or the last assistant row is anything else or further back than
/// [`TAIL`]. Blocking: called off the core's lock.
pub fn row_of(config: &Path, path: &str, since: i64) -> Option<Value> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(transcript_in(config, path)?).ok()?;
    let from = f.metadata().ok()?.len().saturating_sub(TAIL);
    f.seek(SeekFrom::Start(from)).ok()?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    // a line begun before the tail is not a whole row
    let whole = if from > 0 { text.split_once('\n').map_or("", |(_, rest)| rest) } else { &text };
    let row = whole.lines().rev().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(|v| v["type"] == "assistant")?;
    if row["isApiErrorMessage"] != true || crate::transcript::epoch_ms(row["timestamp"].as_str()?)? < since {
        return None;
    }
    if row["error"] == RATE_LIMIT {
        let (window, resets_at) = [&row["quotaLimits"], &row["apiErrorParams"]["rate_limit_info"]].into_iter().find_map(named)?;
        return Some(serde_json::json!({"resets_at": resets_at, "window": window}));
    }
    (row["apiErrorType"] == NO_CREDIT).then(|| serde_json::json!({"error": NO_CREDIT, "words": row["message"]["content"][0]["text"]}))
}

/// Put SAID into STATE at NOW (seconds). A signal that says nothing changes nothing.
///
/// - Windows: a window given replaces the one on file. A window the status line leaves out is dropped once its reset
///   has passed (as Claude Code drops it) and kept until then. Whether it has a 5-hour limit
///   ([`State::five_hour_limit`]): as the signal says, else yes when it gave a 5-hour window; kept when it says neither.
///   When they were seen and from what (`limits_seen`, `limits_from`): only a signal that gave a window sets them.
/// - Out with a time: `out_until` is that time. Out without one (a failed turn, a wait that gave up): the latest reset
///   of a known, still running window that is at 100 per cent, else of any known running window (the limit met is one
///   of them; a `session` or `weekly` word could pick it, but which window a 429 was is not said reliably, so the later
///   one is taken: the account is not used too early). With no running window known: [`UNKNOWN_RESET`].
/// - Out for one model ([`Said::model`]): the account is not out; that model is, in `models_out`, until the time named,
///   else by the same rule from the account's windows of some models ([`unnamed_reset`] of `model_weeks`: t3code names
///   no model of a window), since the first such signal's time while it stays out. In for one model: it is back. A
///   model whose time has passed is dropped then.
/// - Out, either way: why and since when are kept (`out_for`, `out_since`, `out_words`). Since when is the first
///   signal's time while the account stays out. An account out for a reason the owner clears ([`owners_to_clear`])
///   stays out as it is: a later signal's rate limit, time or "in" would bring it back by itself with its key or login
///   still refused.
/// - In: `out_until` and what was kept with it are cleared ([`State::back_in`]).
/// - Of a session ([`Session`]), none of which is a reading of the limits: a sign-in's first refused turn is kept in
///   `refused`; a second while it stands, from any agent (the first one's own again is a turn after its restart),
///   makes the account out for [`REFUSED`] with no time, for the owner's clear or a sign-in. A turn that ended
///   answered takes the first away. A delivery seen in a session makes it heard, and takes away a miss and deafness;
///   one never seen is kept in `missed` and makes it not heard, and a second while that stands makes it deaf.
pub fn apply(state: &mut State, said: &Said, now: i64) {
    let sign = |agent: &String| Some(super::Sign { since: now, agent: agent.clone() });
    let second = match (&said.session, &state.refused) {
        (Some(Session::Answered), _) => return state.refused = None,
        (Some(Session::Reached), _) => return (state.heard, state.missed, state.deaf) = (Some(now), None, None),
        (Some(Session::Unseen(agent)), _) => {
            state.heard = None;
            match state.missed {
                None => state.missed = sign(agent),
                Some(_) => state.deaf = state.deaf.take().or_else(|| sign(agent)),
            }
            return;
        }
        (Some(Session::Refused(agent)), None) => return state.refused = sign(agent),
        (Some(Session::Refused(_)), Some(_)) => true,
        (None, _) => false,
    };
    let (verdict, out_for) = if second { (Verdict::Out(Some(UNKNOWN_RESET)), Some(REFUSED)) } else { (said.verdict.clone(), said.out_for) };
    // a reading of windows: the account's own, or only windows of some models (an agy sign-in's, t3code's families)
    let windows = said.five_hour.is_some() || said.seven_day.is_some() || said.month.is_some() || said.model_weeks.as_ref().is_some_and(|w| !w.is_empty());
    if verdict == Verdict::Silent && !windows {
        return;
    }
    let keep = |old: &Option<Window>, new: &Option<Window>| match new {
        Some(w) => Some(w.clone()),
        None if matches!(said.from, "statusline" | "usage" | GO_USAGE | super::agy::T3CODE) => old.clone().filter(|w| w.resets_at > now),
        None => old.clone(),
    };
    state.five_hour = keep(&state.five_hour, &said.five_hour);
    state.seven_day = keep(&state.seven_day, &said.seven_day);
    state.month = keep(&state.month, &said.month);
    if let Some(has) = said.five_hour_limit.or(said.five_hour.is_some().then_some(true)) {
        state.five_hour_limit = Some(has);
    }
    // the weeks of one model: as `get_usage` named them now, or those named before while they run or have not begun
    // ([`coord_types::NOT_STARTED`], as the mover keeps them)
    match &said.model_weeks {
        Some(weeks) => (state.model_weeks, state.model_weeks_seen) = (weeks.clone(), Some(now)),
        None => state.model_weeks.retain(|w| w.window.resets_at == coord_types::NOT_STARTED || w.window.resets_at > now),
    }
    if let Some(until) = said.extra_usage {
        state.extra_usage_until = until;
    }
    let was_out = state.out_until.is_some_and(|t| t > now);
    // out for what only the owner mends: it stays out as it is, whatever a session says of its limits meanwhile (a
    // status line drawn after a refused turn still carries the windows of the last answered one)
    let held = was_out && state.out_for.as_deref().is_some_and(owners_to_clear);
    match (&verdict, said.model.as_deref()) {
        _ if held => {}
        (Verdict::Out(until), Some(model)) => {
            let until = until.unwrap_or_else(|| unnamed_reset(state.model_weeks.iter().map(|w| &w.window), now));
            let since = state.models_out.iter().find(|o| o.model == model && o.until > now).map_or(now, |o| o.since);
            state.models_out.retain(|o| o.model != model && o.until > now);
            state.models_out.push(coord_types::ModelOut { model: model.to_string(), until, since, words: said.words.clone() });
        }
        (Verdict::In, Some(model)) => state.models_out.retain(|o| o.model != model && o.until > now),
        (Verdict::Out(Some(until)), None) => state.out_until = Some(*until),
        (Verdict::Out(None), None) => state.out_until = Some(unnamed_reset([&state.five_hour, &state.seven_day, &state.month].into_iter().flatten(), now)),
        (Verdict::In, None) => state.back_in(),
        (Verdict::Silent, _) => {}
    }
    if matches!(verdict, Verdict::Out(_)) && said.model.is_none() {
        if !was_out {
            (state.out_for, state.out_since, state.out_words) = (None, Some(now), None);
        }
        if let (Some(why), false) = (out_for, held) {
            (state.out_for, state.out_words) = (Some(why.to_string()), said.words.clone());
        }
    }
    // when the windows were seen, and from what: only a signal that gave one (a verdict alone does not change the card's
    // "as of")
    if windows {
        (state.limits_seen, state.limits_from) = (now, said.from.to_string());
    }
}

/// Whether STATE, a key account's, is to be taken again at NOW by the policy's `key-again-after` of SECS seconds: it
/// is out for its rate limit, nothing named a time, and SECS have passed since it went out. A key's limits are
/// counted per minute and its refusal's row names no reset (design-api-key-accounts.md section 4), so without this
/// only the owner's `coord account clear ID` would bring it back. With MODEL, the same of an agy account's being out
/// for that model alone ([`model_again_at`]).
pub fn again_due(state: &State, model: Option<&str>, secs: u64, now: i64) -> bool {
    match model {
        Some(m) => model_again_at(state, m, secs),
        None => again_at(state, secs),
    }
    .is_some_and(|at| at <= now)
}

/// When something out with no time named is back (seconds), by WINDOWS at NOW: the latest reset of those used up,
/// else of those running (which window a 429 was is not said reliably, so the later is taken); [`UNKNOWN_RESET`] with
/// none running. A window not begun ([`coord_types::NOT_STARTED`]) is none of them. One rule for an account's own
/// windows and an agy model's over its account's windows.
pub fn unnamed_reset<'a>(windows: impl IntoIterator<Item = &'a Window>, now: i64) -> i64 {
    let running: Vec<&Window> = windows.into_iter().filter(|w| w.resets_at > now).collect();
    let full = running.iter().filter(|w| w.used_percent >= 100.0).map(|w| w.resets_at).max();
    full.or_else(|| running.iter().map(|w| w.resets_at).max()).unwrap_or(UNKNOWN_RESET)
}

/// When (seconds) the policy's `key-again-after` of SECS takes STATE's MODEL again: it is out for MODEL with no time
/// named and no window to take one from ([`UNKNOWN_RESET`]), as [`again_at`] for an account; None otherwise.
pub fn model_again_at(state: &State, model: &str, secs: u64) -> Option<i64> {
    state.models_out.iter().find(|o| o.model == model && o.until == UNKNOWN_RESET).map(|o| o.since.saturating_add(secs as i64))
}

/// When (seconds) the policy's `key-again-after` of SECS takes STATE again; None when it is not out for its rate limit
/// with no time named.
pub fn again_at(state: &State, secs: u64) -> Option<i64> {
    (reset_unknown(state) && state.out_for.as_deref() == Some(RATE_LIMIT)).then_some(state.out_since?.saturating_add(secs as i64))
}

/// One window of the status line's `rate_limits`, if both its numbers are there.
fn window(v: &Value) -> Option<Window> {
    Some(Window { used_percent: v.get("used_percentage")?.as_f64()?, resets_at: v.get("resets_at")?.as_f64()? as i64 })
}

/// The status line's text for the terminal: the two windows in per cent ("5h 24% · 7d 41%"), or nothing when there
/// are none. The script `agent-base/claude/statusline` prints the same.
pub fn line(said: &Said) -> String {
    [("5h", &said.five_hour), ("7d", &said.seven_day)]
        .into_iter()
        .filter_map(|(name, w)| w.as_ref().map(|w| format!("{name} {:.0}%", w.used_percent)))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// One rule for an out with no time named ([`unnamed_reset`]), for an account and for an agy model: the used-up
    /// window's reset, else the latest running, else none known. One model out leaves the account in; all out, it takes
    /// none until the first is back; the policy brings one model back; the owner's clear, all.
    #[test]
    fn an_out_with_no_time_named_is_one_rule_for_an_account_and_for_one_model() {
        let w = |used_percent: f64, resets_at: i64| Window { used_percent, resets_at };
        let now = 1_000;
        assert_eq!(unnamed_reset(&[w(100.0, 2_000), w(40.0, 9_000)], now), 2_000, "the used-up one");
        assert_eq!(unnamed_reset(&[w(10.0, 2_000), w(40.0, 9_000)], now), 9_000, "none used up: the latest running");
        assert_eq!(unnamed_reset(&[w(100.0, 900), w(0.0, coord_types::NOT_STARTED)], now), UNKNOWN_RESET, "none running");
        let week = coord_types::ModelWeek { model: "Claude & GPT (shared) · 5-hour".into(), window: w(100.0, 2_000) };
        let mut s = State { signed_in: true, models: vec!["agy/a".into(), "agy/b".into()], model_weeks: vec![week], ..State::default() };
        let out = |m: &str, until: Option<i64>| Said { model: Some(m.into()), out_for: Some(RATE_LIMIT), ..Said::of(Verdict::Out(until), "failed") };
        apply(&mut s, &out("agy/a", None), now);
        assert_eq!((s.model_out("agy/a", now), s.out_until, s.out_for.as_deref(), s.can_take(now)), (Some(2_000), None, None, true));
        apply(&mut s, &out("agy/b", Some(1_500)), now);
        assert_eq!((s.models_all_out(now), s.out_now(now)), (Some(1_500), Some(1_500)));
        assert!(!s.can_take(now) && s.can_take(1_500) && !s.can_take_model("agy/a", 1_500), "back when the first is");
        apply(&mut s, &Said::again_model("agy/b"), now);
        assert_eq!((s.model_out("agy/b", now), s.models_all_out(now), s.model_out("agy/a", now)), (None, None, Some(2_000)));
        s.cleared();
        assert!(s.models_out.is_empty());
    }

    /// The gateway's words at a 429 name the reset as a Go duration; words without one name none.
    #[test]
    fn a_refused_requests_words_name_its_reset_after() {
        let gateway = "API Error: Request rejected (429) · All credentials for model claude-opus-5-5 are cooling down (last error: rate_limit_error: This request would exceed your account's rate limit. Please try again later.) (reset after 18m48s)";
        assert_eq!(reset_after(gateway), Some(18 * 60 + 48));
        assert_eq!(reset_after("API Error: Request rejected (429) · This request would exceed your account's rate limit. Please try again later."), None);
        for (w, s) in [("reset after 30s", Some(30)), ("(reset after 1h2m3s)", Some(3723)), ("reset after 1.5s.", Some(2)), ("reset after 500ms", Some(1)), ("reset after 2h0m0s)", Some(7200))] {
            assert_eq!(reset_after(w), s, "{w}");
        }
        for w in ["reset after soon", "reset after 12", "reset after 3d", "reset after "] {
            assert_eq!(reset_after(w), None, "{w}");
        }
    }

    /// Codex's words at its usage limit name the reset as a time on this machine's clock; one past names none.
    #[test]
    fn codexs_usage_limit_names_its_reset_as_a_time() {
        use chrono::TimeZone;
        let codex = "You\u{2019}ve hit your usage limit. Upgrade to Pro (https://chatgpt.com/explore/pro), visit https://chatgpt.com/codex/settings/usage to purchase more credits or try again at Nov 27th, 2026 11:52 AM.";
        let reset = chrono::Local.with_ymd_and_hms(2026, 11, 27, 11, 52, 0).earliest().unwrap().timestamp();
        let now = reset - 20 * 3600;
        assert_eq!(try_again_at(codex, now), Some(20 * 3600));
        assert_eq!(try_again_at(&codex.replace("11:52 AM", "1:05 PM").replace("27th", "1st").replace("Nov", "Dec"), now), Some(chrono::Local.with_ymd_and_hms(2026, 12, 1, 13, 5, 0).earliest().unwrap().timestamp() - now));
        assert_eq!(try_again_at(codex, reset), None, "past: no reset named");
        let clock = |h, m| chrono::Local.with_ymd_and_hms(2026, 11, 26, h, m, 0).earliest().unwrap().timestamp();
        let noon = clock(12, 0);
        assert_eq!(try_again_at("try again at 3:04 PM.", noon), Some(clock(15, 4) - noon), "a time alone: today");
        assert_eq!(try_again_at("try again at 11:30 AM.", noon), Some(chrono::Local.with_ymd_and_hms(2026, 11, 27, 11, 30, 0).earliest().unwrap().timestamp() - noon), "passed today: tomorrow");
        assert_eq!(try_again_at("please try again later", now), None);
        assert_eq!(try_again_at("try again at PM.", now), None, "no time before its half of the day");
    }

    #[test]
    fn the_last_assistant_row_of_a_refused_turn_names_the_reset() {
        let dir = std::env::temp_dir().join(format!("coordd-limit-row-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // an account's folder as `Store::layout` leaves it: `config/projects` is a link to the owner's folder
        let (config, owners) = (dir.join("accounts/claude/a/config"), dir.join("home/.claude/projects"));
        std::fs::create_dir_all(owners.join("-work")).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        std::os::unix::fs::symlink(&owners, config.join("projects")).unwrap();
        let path = config.join("projects/-work/s.jsonl").display().to_string();
        let row_of = |path: &str, since| row_of(&config, path, since);
        let write = |rows: &[Value]| std::fs::write(&path, rows.iter().map(|r| format!("{r}\n")).collect::<String>()).unwrap();
        let asked = json!({"type": "user", "timestamp": "2026-11-18T10:00:00.000Z", "message": {"role": "user", "content": "a prompt"}});
        let answered = json!({"type": "assistant", "timestamp": "2026-11-18T10:00:05.000Z", "message": {"role": "assistant", "model": "claude-opus-5-5", "content": [{"type": "text", "text": "ok"}]}});
        // the row as Claude Code writes it at a 429 of the plan's limit
        let refused = json!({"type": "assistant", "timestamp": "2026-11-18T10:01:00.250Z", "isApiErrorMessage": true, "error": "rate_limit", "apiError": "usage_limit_reached", "apiErrorStatus": 429,
            "apiErrorParams": {"rate_limit_info": {"status": "rejected", "resetsAt": 1_795_010_000, "rateLimitType": "five_hour"}},
            "quotaLimits": {"status": "rejected", "resetsAt": 1_795_010_000, "rateLimitType": "five_hour"},
            "message": {"role": "assistant", "model": "<synthetic>", "content": [{"type": "text", "text": "You've hit your session limit"}]}});
        let after = json!({"type": "system", "timestamp": "2026-11-18T10:01:00.300Z", "subtype": "turn_duration"});
        let since = crate::transcript::epoch_ms("2026-11-18T10:00:00Z").unwrap();
        write(&[asked.clone(), answered.clone(), asked.clone(), refused.clone(), after]);
        assert_eq!(row_of(&path, since), Some(json!({"resets_at": 1_795_010_000, "window": "five_hour"})));
        // a row from before this process began is an earlier account's
        assert_eq!(row_of(&path, since + 3_600_000), None);
        // the turn went on after it: the last assistant row is an answer
        write(&[asked.clone(), refused.clone(), asked.clone(), answered.clone()]);
        assert_eq!(row_of(&path, since), None);
        // a server error is no limit of the plan; a row with only the older field is read too
        let mut other = refused.clone();
        other["error"] = json!("server_error");
        write(&[asked.clone(), other]);
        assert_eq!(row_of(&path, since), None);
        let mut older = refused.clone();
        older.as_object_mut().unwrap().remove("quotaLimits");
        write(&[asked, older]);
        assert_eq!(row_of(&path, since).map(|r| r["resets_at"].clone()), Some(json!(1_795_010_000)));
        assert_eq!(row_of(&config.join("projects/-work/none.jsonl").display().to_string(), since), None);
        // the API's 402, which the hook names only `unknown`: the row says billing_error, with the words shown (as
        // Claude Code 2.1.327 wrote it in the rig)
        let no_credit = json!({"type": "assistant", "timestamp": "2026-11-18T10:02:00.000Z", "isApiErrorMessage": true, "error": "unknown", "apiErrorStatus": 402, "apiErrorType": "billing_error",
            "resendCannotCure": true, "message": {"role": "assistant", "model": "<synthetic>", "content": [{"type": "text", "text": "API Error: 402 billing"}]}});
        write(&[answered.clone(), no_credit]);
        assert_eq!(row_of(&path, since), Some(json!({"error": "billing_error", "words": "API Error: 402 billing"})));
        write(&[answered.clone(), refused.clone()]);
        // the path is the hook's, so the agent's: only a `.jsonl` under the folder's `projects` is opened. The same
        // rows elsewhere, reached by `..`, through a link from inside, or under another name, say nothing
        let rows = std::fs::read(&path).unwrap();
        std::fs::write(dir.join("home/outside.jsonl"), &rows).unwrap();
        std::fs::write(owners.join("-work/s.txt"), &rows).unwrap();
        std::os::unix::fs::symlink(dir.join("home/outside.jsonl"), owners.join("-work/link.jsonl")).unwrap();
        let outside = dir.join("home/outside.jsonl").display().to_string();
        assert_eq!(std::fs::read(&outside).unwrap(), rows);
        for elsewhere in [outside.as_str(), &format!("{}/projects/-work/../../../outside.jsonl", config.display()), &format!("{}/projects/-work/link.jsonl", config.display()), &format!("{}/projects/-work/s.txt", config.display())] {
            assert!(std::path::Path::new(elsewhere).is_file(), "{elsewhere}");
            assert_eq!(row_of(elsewhere, since), None, "{elsewhere}");
        }
        // and the real one by another spelling of its own place is still read
        assert_eq!(row_of(&format!("{}/projects/-work/../-work/s.jsonl", config.display()), since).map(|r| r["resets_at"].clone()), Some(json!(1_795_010_000)));
        // a config folder without `projects` has no transcripts
        assert_eq!(super::row_of(&dir.join("accounts/claude/none/config"), &path, since), None);
        // handed to the signals: out until that time; without a time it says nothing
        for key in [false, true] {
            let said = from_row(&json!({"resets_at": 1_795_010_000, "window": "five_hour"}), key);
            assert_eq!((said.verdict, said.out_for), (Verdict::Out(Some(1_795_010_000)), Some(RATE_LIMIT)));
            assert_eq!(from_row(&json!({}), key).verdict, Verdict::Silent);
        }
        // a 402's row puts a key account out for the owner to clear, and says nothing of a sign-in account
        let row = json!({"error": "billing_error", "words": "API Error: 402 billing"});
        let said = from_row(&row, true);
        assert_eq!((said.verdict, said.out_for, said.words.as_deref()), (Verdict::Out(None), Some(NO_CREDIT), Some("API Error: 402 billing")));
        assert_eq!(from_row(&row, false).verdict, Verdict::Silent);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The status line's input as statusline.md shows it, cut to what matters here.
    fn statusline(five: f64, five_at: i64, seven: f64, seven_at: i64) -> Value {
        json!({"session_id": "s1", "transcript_path": "/p/s1.jsonl", "model": {"display_name": "Opus"},
               "rate_limits": {"five_hour": {"used_percentage": five, "resets_at": five_at},
                               "seven_day": {"used_percentage": seven, "resets_at": seven_at}}})
    }

    #[test]
    fn the_status_line_gives_both_windows_and_a_full_one_makes_the_account_out_until_its_reset() {
        let said = from_statusline(&statusline(23.5, 2_000, 41.2, 9_000));
        assert_eq!(said.five_hour, Some(Window { used_percent: 23.5, resets_at: 2_000 }));
        assert_eq!(said.verdict, Verdict::In);
        assert_eq!(line(&said), "5h 24% · 7d 41%");
        let mut s = State { signed_in: true, out_until: Some(1_500), ..State::default() };
        apply(&mut s, &said, 1_000);
        assert_eq!((s.out_until, s.limits_seen, s.limits_from.as_str()), (None, 1_000, "statusline"));

        apply(&mut s, &from_statusline(&statusline(100.0, 2_000, 60.0, 9_000)), 1_100);
        assert_eq!(s.out_until, Some(2_000));
        assert!(!s.can_take(1_999) && s.can_take(2_000));
        apply(&mut s, &from_statusline(&statusline(100.0, 2_000, 100.0, 9_000)), 1_200);
        assert_eq!(s.out_until, Some(9_000));
    }

    #[test]
    fn a_status_line_without_limits_says_nothing_and_a_dropped_window_goes_once_it_reset() {
        let mut s = State::default();
        let none = from_statusline(&json!({"session_id": "s1"}));
        assert_eq!((none.verdict.clone(), line(&none)), (Verdict::Silent, String::new()));
        apply(&mut s, &none, 1_000);
        assert_eq!(s, State::default());

        apply(&mut s, &from_statusline(&statusline(30.0, 2_000, 50.0, 9_000)), 1_000);
        let only_week = json!({"rate_limits": {"seven_day": {"used_percentage": 55, "resets_at": 9_000}}});
        apply(&mut s, &from_statusline(&only_week), 1_500);
        assert_eq!(s.five_hour.as_ref().map(|w| w.resets_at), Some(2_000), "kept while it runs");
        apply(&mut s, &from_statusline(&only_week), 2_100);
        assert_eq!(s.five_hour, None, "dropped once its reset passed");
        assert_eq!(s.seven_day, Some(Window { used_percent: 55.0, resets_at: 9_000 }));
    }

    #[test]
    fn a_turn_failed_on_its_limit_is_out_until_a_known_reset_or_unknown() {
        let failed = json!({"hook_event_name": "StopFailure", "error": "rate_limit", "error_details": "429 Too Many Requests",
                            "last_assistant_message": "API Error: Rate limit reached"});
        let said = from_failed(&failed, false, "w");
        assert_eq!((&said.verdict, said.out_for, said.words.as_deref()), (&Verdict::Out(None), Some(RATE_LIMIT), Some("API Error: Rate limit reached")));

        // a full window on file gives the time
        let mut s = State { signed_in: true, five_hour: Some(Window { used_percent: 100.0, resets_at: 2_000 }), seven_day: Some(Window { used_percent: 70.0, resets_at: 9_000 }), ..State::default() };
        apply(&mut s, &said, 1_000);
        assert_eq!((s.out_until, s.limits_seen, s.limits_from.as_str()), (Some(2_000), 0, ""), "a verdict alone is no reading of the windows");
        // no full one: the later of the running ones
        let mut s = State { five_hour: Some(Window { used_percent: 90.0, resets_at: 2_000 }), seven_day: Some(Window { used_percent: 70.0, resets_at: 9_000 }), ..State::default() };
        apply(&mut s, &said, 1_000);
        assert_eq!(s.out_until, Some(9_000));
        // none known, or only ones that reset already: unknown
        let mut s = State { signed_in: true, five_hour: Some(Window { used_percent: 100.0, resets_at: 500 }), ..State::default() };
        apply(&mut s, &said, 1_000);
        assert!(reset_unknown(&s) && !s.can_take(i64::MAX - 1));
        // why and since when are kept while it is out; a later word of the same limit does not move "since"
        assert_eq!((s.out_for.as_deref(), s.out_since, s.out_words.as_deref()), (Some("rate_limit"), Some(1_000), Some("API Error: Rate limit reached")));
        apply(&mut s, &said, 1_030);
        assert_eq!(s.out_since, Some(1_000));
        // the next status line from a session on the account settles it, and nothing of its being out is left
        apply(&mut s, &from_statusline(&statusline(5.0, 19_000, 70.0, 90_000)), 2_000);
        assert_eq!((s.out_until, &s.out_for, s.out_since, &s.out_words), (None, &None, None, &None));
    }

    /// An opencode agent's `failed` names its provider's reset when opencode had one (`resets_at`, opencode-rpc's
    /// `failure`): that is the time, over a window on file; beside any other error it says nothing.
    #[test]
    fn a_failed_turn_that_names_its_reset_is_out_until_it() {
        let said = from_failed(&json!({"error": "rate_limit", "error_details": "Go limit reached", "resets_at": 5_000}), true, "w");
        assert_eq!((&said.verdict, said.out_for), (&Verdict::Out(Some(5_000)), Some(RATE_LIMIT)));
        let mut s = State { signed_in: true, five_hour: Some(Window { used_percent: 100.0, resets_at: 2_000 }), ..State::default() };
        apply(&mut s, &said, 1_000);
        assert_eq!((s.out_until, s.out_for.as_deref()), (Some(5_000), Some(RATE_LIMIT)));
        let refused = from_failed(&json!({"error": "authentication_failed", "resets_at": 5_000}), true, "w");
        assert_eq!((&refused.verdict, refused.out_for), (&Verdict::Out(None), Some(REFUSED)));
        assert_eq!(from_failed(&json!({"error": "APIError", "resets_at": 5_000}), true, "w").verdict, Verdict::Silent);
    }

    #[test]
    fn a_key_account_is_out_for_a_refused_key_or_no_credit_until_the_owner_and_for_its_rate_limit_until_the_policys_time() {
        let failed = |error: &str, words: &str| json!({"hook_event_name": "StopFailure", "error": error, "last_assistant_message": words});
        let key = State { signed_in: true, auth_method: Some("api_key".into()), ..State::default() };
        // the two that are the account's and the owner's to clear, as the rig showed them
        for (error, words, why) in [("authentication_failed", "Invalid API key · Fix external API key", REFUSED), ("billing_error", "Credit balance is too low", NO_CREDIT)] {
            let said = from_failed(&failed(error, words), true, "w");
            assert_eq!((&said.verdict, said.out_for), (&Verdict::Out(None), Some(why)));
            let mut s = key.clone();
            apply(&mut s, &said, 1_000);
            assert!(reset_unknown(&s) && owners_to_clear(s.out_for.as_deref().unwrap()));
            assert_eq!((s.out_for.as_deref(), s.out_since, s.out_words.as_deref()), (Some(why), Some(1_000), Some(words)));
            // never by the policy's time, however long ago
            assert_eq!(again_at(&s, 60), None);
            assert!(!again_due(&s, None, 60, i64::MAX - 1));
            // a rate limit said of it later (another session's turn) does not make it one that comes back by itself
            apply(&mut s, &from_failed(&failed("rate_limit", "API Error: Rate limit reached"), true, "w"), 1_100);
            assert_eq!((s.out_for.as_deref(), s.out_since, s.out_words.as_deref()), (Some(why), Some(1_000), Some(words)));
            // the owner clears it
            s.back_in();
            assert_eq!(State { limits_seen: 0, limits_from: String::new(), ..s }, key);
        }
        // its rate limit: no window, so no time; taken again SECS after it went out, by the policy's number
        let mut s = key.clone();
        apply(&mut s, &from_failed(&failed("rate_limit", "API Error: Rate limit reached"), true, "w"), 1_000);
        assert!(reset_unknown(&s));
        assert_eq!((again_at(&s, 60), again_due(&s, None, 60, 1_059), again_due(&s, None, 60, 1_060)), (Some(1_060), false, true));
        assert_eq!(again_at(&s, 5), Some(1_005), "the number is the policy's, read when it is asked");
        // a refused key found after it replaces the reason: it does not come back by itself any more
        apply(&mut s, &from_failed(&failed("authentication_failed", "Invalid API key"), true, "w"), 1_010);
        assert_eq!((s.out_for.as_deref(), s.out_since, again_at(&s, 60)), (Some(REFUSED), Some(1_000), None));
        // the policy's word takes it in again
        let mut s = State { out_until: Some(UNKNOWN_RESET), out_for: Some(RATE_LIMIT.into()), out_since: Some(1_000), ..key.clone() };
        apply(&mut s, &Said::again(), 1_060);
        assert_eq!(s, key.clone(), "taken in, and no window was read");
        // a sign-in account with a known reset is not the policy's to take again
        let signin = State { signed_in: true, out_until: Some(5_000), out_for: Some(RATE_LIMIT.into()), out_since: Some(1_000), ..State::default() };
        assert_eq!(again_at(&signin, 60), None);
    }

    #[test]
    fn other_failures_and_a_server_throttle_say_nothing_of_the_limits() {
        let mut s = State { signed_in: true, ..State::default() };
        for input in [
            json!({"error": "server_error", "error_details": "529 Overloaded"}),
            json!({"error": "billing_error"}),
            json!({"error": "rate_limit", "last_assistant_message": "API Error: Server is temporarily limiting requests"}),
            json!({}),
        ] {
            apply(&mut s, &from_failed(&input, false, "w"), 1_000);
        }
        assert_eq!(s, State { signed_in: true, ..State::default() });
        // for a key account the server's troubles, a throttle and an error with no name say nothing either
        for input in [json!({"error": "server_error"}), json!({"error": "unknown"}), json!({"error": "rate_limit", "last_assistant_message": "API Error: Server is temporarily limiting requests"})] {
            apply(&mut s, &from_failed(&input, true, "w"), 1_000);
        }
        assert_eq!(s, State { signed_in: true, ..State::default() });
    }

    #[test]
    fn a_sign_ins_refused_turn_is_kept_and_a_second_before_an_answered_turn_makes_it_out_for_the_owner() {
        let refused = |agent| from_failed(&json!({"error": "authentication_failed", "last_assistant_message": "Please run /login · API Error: 401"}), false, agent);
        let fresh = State { signed_in: true, seven_day: Some(Window { used_percent: 40.0, resets_at: 9_000 }), limits_seen: 900, limits_from: "statusline".into(), ..State::default() };
        let mut s = fresh.clone();
        // the first: kept, nothing else; it is no reading of the limits
        apply(&mut s, &refused("w1"), 1_000);
        assert_eq!(s, State { refused: Some(crate::accounts::Sign { since: 1_000, agent: "w1".into() }), ..fresh.clone() });
        assert!(s.can_take(1_000), "one session's refusal leaves the account taking agents");
        // a status line drawn after it (the windows of the last answered request) does not take it away; a turn on it
        // that ended answered does: the refusal was that session's
        apply(&mut s, &from_statusline(&json!({"rate_limits": {"seven_day": {"used_percentage": 40, "resets_at": 9_000}}})), 1_050);
        assert_eq!(s.refused.as_ref().map(|r| r.agent.as_str()), Some("w1"));
        apply(&mut s, &Said::answered(), 1_100);
        assert_eq!(s, State { limits_seen: 1_050, ..fresh.clone() });
        // refused again after that: a first one again; then a second, from any agent (here the first's restart), puts it out
        let fresh = s.clone();
        apply(&mut s, &refused("w2"), 1_200);
        assert_eq!(s, State { refused: Some(crate::accounts::Sign { since: 1_200, agent: "w2".into() }), ..fresh });
        apply(&mut s, &refused("w2"), 1_300);
        assert_eq!((s.out_until, s.out_for.as_deref(), s.out_since, s.out_words.as_deref()), (Some(UNKNOWN_RESET), Some(REFUSED), Some(1_300), Some("Please run /login · API Error: 401")));
        assert!(!s.can_take(2_000) && owners_to_clear(REFUSED));
        // nothing but the owner's clear or a sign-in brings it back: not a rate limit's later word with a window
        // running, nor a status line drawn after the refusal, nor an answered turn of a session still on it
        apply(&mut s, &from_failed(&json!({"error": "rate_limit"}), false, "w3"), 1_400);
        apply(&mut s, &from_statusline(&json!({"rate_limits": {"seven_day": {"used_percentage": 41, "resets_at": 9_000}}})), 1_500);
        apply(&mut s, &Said::answered(), 1_600);
        assert_eq!((s.out_until, s.out_for.as_deref(), s.seven_day.as_ref().map(|w| w.used_percent)), (Some(UNKNOWN_RESET), Some(REFUSED), Some(41.0)));
        s.cleared();
        assert_eq!((s.out_until, s.refused.is_none()), (None, true));
        // a key account's refused key is the key's, as before: out at the first
        let mut k = State { signed_in: true, ..State::default() };
        apply(&mut k, &from_failed(&json!({"error": "authentication_failed"}), true, "w1"), 1_000);
        assert_eq!((k.out_until, k.out_for.as_deref(), k.refused), (Some(UNKNOWN_RESET), Some(REFUSED), None));
    }

    #[test]
    fn quota_notifications_say_in_or_out_and_others_nothing() {
        let note = |t: &str| json!({"hook_event_name": "Notification", "notification_type": t, "message": "…"});
        assert_eq!(from_notification(&note("quota_auto_resume_fired")).verdict, Verdict::In);
        assert_eq!(from_notification(&note("quota_auto_resume_stale")).verdict, Verdict::In);
        assert_eq!(from_notification(&note("quota_auto_resume_disabled")).verdict, Verdict::Out(None));
        assert_eq!(from_notification(&note("idle_prompt")).verdict, Verdict::Silent);
        let mut s = State::default();
        apply(&mut s, &from_notification(&note("quota_auto_resume_disabled")), 1_000);
        assert!(reset_unknown(&s));
        apply(&mut s, &from_notification(&note("quota_auto_resume_fired")), 1_100);
        assert_eq!((s.out_until, s.limits_seen, s.limits_from.as_str()), (None, 0, ""));
    }

    /// `get_usage`'s answer in 2.1.330's shape, made up: `subscription_type`, `rate_limits_available` and its
    /// `rate_limits` LIMITS (the session's totals and `behaviors`, which are not read, left out).
    fn usage(plan: Option<&str>, limits: Value) -> Value {
        json!({"subscription_type": plan, "rate_limits_available": true, "rate_limits": limits, "behaviors": null})
    }

    #[test]
    fn get_usage_gives_the_two_windows_a_full_one_makes_it_out_and_an_answer_without_them_nothing() {
        let utc = |y, m, d, h, min| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc().timestamp();
        let now = utc(2026, 11, 20, 10, 0);
        let limits = json!({
            "five_hour": {"utilization": 17.4, "resets_at": "2026-11-20T15:00:00.000000+00:00"},
            "seven_day": {"utilization": 6.0, "resets_at": "2026-11-24T09:30:00Z"},
            "seven_day_sonnet": {"utilization": 100.0, "resets_at": "2026-11-24T09:00:00Z"},
            "seven_day_opus": null,
            "extra_usage": {"is_enabled": false, "monthly_limit": null, "used_credits": null, "utilization": null},
        });
        let said = from_usage(&usage(Some("max"), limits.clone())).unwrap();
        assert_eq!(said.five_hour_limit, Some(true));
        assert_eq!(said.five_hour, Some(Window { used_percent: 17.4, resets_at: utc(2026, 11, 20, 15, 0) }), "not floored as /usage prints it");
        assert_eq!(said.seven_day, Some(Window { used_percent: 6.0, resets_at: utc(2026, 11, 24, 9, 30) }));
        assert_eq!((said.verdict, said.out_for, said.from, said.extra_usage), (Verdict::In, None, "usage", Some(None)), "the Sonnet week is not the account's");
        // but it is given for the card, named as /usage names it, as is each model_scoped week; one nothing was used of
        // yet (no reset) is no window
        let week = |model: &str, used_percent, resets_at| coord_types::ModelWeek { model: model.into(), window: Window { used_percent, resets_at } };
        assert_eq!(said.model_weeks, Some(vec![week("Sonnet only", 100.0, utc(2026, 11, 24, 9, 0))]));
        let mut fable = limits.clone();
        fable["model_scoped"] = json!([
            {"display_name": "Fable", "utilization": 52.0, "resets_at": "2026-11-24T09:00:00Z"},
            {"display_name": "Opus", "utilization": 0.0, "resets_at": null},
            {"display_name": "Haiku", "utilization": 9.0, "resets_at": "next Tuesday"},
        ]);
        let said = from_usage(&usage(Some("max"), fable.clone())).unwrap();
        assert_eq!(said.model_weeks, Some(vec![week("Sonnet only", 100.0, utc(2026, 11, 24, 9, 0)), week("Fable", 52.0, utc(2026, 11, 24, 9, 0))]), "a week whose reset is not read is left out alone");
        // on a pro plan /usage shows no Sonnet week
        assert_eq!(from_usage(&usage(Some("pro"), fable)).unwrap().model_weeks, Some(vec![week("Fable", 52.0, utc(2026, 11, 24, 9, 0))]));
        // kept: a status line, which names none, leaves them while they run; the next read says them anew
        let mut s = State { signed_in: true, ..State::default() };
        apply(&mut s, &said, now);
        assert_eq!(s.model_weeks.len(), 2);
        apply(&mut s, &from_statusline(&json!({"rate_limits": {"five_hour": {"used_percentage": 20.0, "resets_at": utc(2026, 11, 20, 15, 0)}}})), now + 60);
        assert_eq!(s.model_weeks.len(), 2, "a status line has none of them");
        apply(&mut s, &from_statusline(&json!({"rate_limits": {"five_hour": {"used_percentage": 20.0, "resets_at": utc(2026, 11, 25, 15, 0)}}})), utc(2026, 11, 24, 9, 1));
        assert!(s.model_weeks.is_empty(), "both weeks started again");
        apply(&mut s, &from_usage(&usage(Some("max"), limits.clone())).unwrap(), now);
        assert_eq!(s.model_weeks.iter().map(|w| w.model.as_str()).collect::<Vec<_>>(), ["Sonnet only"]);
        // a full window: out until the latest such reset
        let full = json!({"five_hour": {"utilization": 100.0, "resets_at": "2026-11-20T15:00:00Z"}, "seven_day": {"utilization": 40.0, "resets_at": "2027-01-02T09:00:00Z"}});
        let said = from_usage(&usage(Some("max"), full)).unwrap();
        assert_eq!((said.verdict, said.out_for), (Verdict::Out(Some(utc(2026, 11, 20, 15, 0))), Some(RATE_LIMIT)));
        assert_eq!(said.seven_day.unwrap().resets_at, utc(2027, 1, 2, 9, 0));
        // nothing used yet this session: no window, and a 5-hour limit all the same
        let said = from_usage(&usage(Some("max"), json!({"five_hour": {"utilization": 0.0, "resets_at": null}, "seven_day": {"utilization": 3.0, "resets_at": "2026-11-24T09:00:00Z"}}))).unwrap();
        assert!(said.five_hour_limit == Some(true) && said.five_hour.is_none() && said.seven_day == Some(Window { used_percent: 3.0, resets_at: utc(2026, 11, 24, 9, 0) }));
        let mut kept = State { signed_in: true, ..State::default() };
        apply(&mut kept, &said, now);
        assert_eq!(kept.five_hour_limit, Some(true), "kept with no 5-hour window given");
        let none = from_usage(&usage(Some("team"), json!({"five_hour": null, "seven_day": {"utilization": 3.0, "resets_at": "2026-11-24T09:00:00Z"}}))).unwrap();
        assert_eq!(none.five_hour_limit, Some(false), "no five_hour: no 5-hour limit");
        apply(&mut kept, &none, now);
        assert_eq!(kept.five_hour_limit, Some(false));
        // nothing is taken from an answer without the account's windows
        let signed_out = json!({"session": {"total_cost_usd": 0}, "subscription_type": null, "rate_limits_available": false, "rate_limits": null, "behaviors": null});
        assert_eq!(from_usage(&signed_out).unwrap_err(), "get_usage said no plan limits apply to it (plan none)", "as 2.1.330 answers with no login");
        assert!(from_usage(&usage(Some("max"), json!({"model_scoped": [{"display_name": "Fable", "utilization": 52.0, "resets_at": "2026-11-24T09:00:00Z"}]}))).unwrap_err().contains("said no window"), "a model's week alone");
        assert!(from_usage(&usage(Some("max"), json!({"five_hour": {"utilization": 5.0, "resets_at": "Nov 20, 3pm"}}))).unwrap_err().contains("could not be read"));
    }

    /// Read from get_usage: past a window's limit with extra usage on and credit left, the card says requests run on it
    /// until the reset; off, credit used up, or no window full, it does not; a status line leaves it.
    #[test]
    fn get_usage_says_extra_usage_carries_a_full_account_until_its_reset_while_it_is_on_with_credit_left() {
        let utc = |y, m, d, h, min| chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap().and_utc().timestamp();
        let now = utc(2026, 11, 20, 10, 0);
        let limits = |five: f64, extra: Value| json!({"five_hour": {"utilization": five, "resets_at": "2026-11-20T15:00:00Z"}, "seven_day": {"utilization": 40.0, "resets_at": "2026-11-24T09:30:00Z"}, "extra_usage": extra});
        let on = json!({"is_enabled": true, "monthly_limit": 5000, "used_credits": 1200, "utilization": 24.0, "currency": "USD"});
        let said = from_usage(&usage(Some("max"), limits(100.0, on.clone()))).unwrap();
        assert_eq!((said.extra_usage, &said.verdict), (Some(Some(utc(2026, 11, 20, 15, 0))), &Verdict::Out(Some(utc(2026, 11, 20, 15, 0)))), "the window used up; out by its limits all the same");
        let no_cap = json!({"is_enabled": true, "monthly_limit": null, "used_credits": null, "utilization": null});
        assert_eq!(from_usage(&usage(Some("max"), limits(100.0, no_cap))).unwrap().extra_usage, Some(Some(utc(2026, 11, 20, 15, 0))), "no monthly limit");
        let spent = json!({"is_enabled": true, "monthly_limit": 5000, "used_credits": 5000, "utilization": 100.0});
        let off = json!({"is_enabled": false, "monthly_limit": null, "used_credits": null, "utilization": null});
        for (extra, why) in [(spent, "its credit used up"), (off, "off"), (Value::Null, "not said")] {
            assert_eq!(from_usage(&usage(Some("max"), limits(100.0, extra))).unwrap().extra_usage, Some(None), "{why}");
        }
        assert_eq!(from_usage(&usage(Some("max"), limits(90.0, on))).unwrap().extra_usage, Some(None), "no window used up");
        let mut s = State { signed_in: true, ..State::default() };
        apply(&mut s, &said, now);
        assert_eq!(s.extra_usage_until, Some(utc(2026, 11, 20, 15, 0)));
        apply(&mut s, &from_statusline(&json!({"rate_limits": {"five_hour": {"used_percentage": 100.0, "resets_at": utc(2026, 11, 20, 15, 0)}}})), now + 60);
        assert_eq!(s.extra_usage_until, Some(utc(2026, 11, 20, 15, 0)), "a status line does not say");
        apply(&mut s, &from_usage(&usage(Some("max"), limits(30.0, Value::Null))).unwrap(), now + 120);
        assert_eq!(s.extra_usage_until, None);
    }

    /// Made-up `rate_limit_info`s of a print-mode run's `rate_limit_event`, in 2.1.329's shape (`h5r`: `status`,
    /// `resetsAt`, `rateLimitType`, `utilization`, and `unifiedWindows` when the answer's headers carry them).
    #[test]
    fn a_rate_limit_event_gives_its_windows_whether_there_is_a_5_hour_limit_and_out_only_when_rejected() {
        let now = 1_795_000_000;
        let both = json!({
            "status": "allowed", "resetsAt": now + 9_000, "rateLimitType": "five_hour", "utilization": 0.25, "isUsingOverage": false,
            "unifiedWindows": {"five_hour": {"utilization": 0.25, "resetsAt": now + 9_000, "status": "allowed"}, "seven_day": {"utilization": 0.5, "resetsAt": now + 300_000}},
        });
        let said = from_event(&both);
        assert_eq!(said.five_hour, Some(Window { used_percent: 25.0, resets_at: now + 9_000 }));
        assert_eq!(said.seven_day, Some(Window { used_percent: 50.0, resets_at: now + 300_000 }));
        assert_eq!((said.five_hour_limit, said.verdict, said.out_for, said.from), (Some(true), Verdict::In, None, "rate_limit_event"));
        // its windows hold no 5-hour one: the account has none
        let weekly = json!({"status": "allowed", "unifiedWindows": {"seven_day": {"utilization": 0.1, "resetsAt": now + 300_000}}});
        assert_eq!((from_event(&weekly).five_hour, from_event(&weekly).five_hour_limit), (None, Some(false)));
        // without them: the one window its top level names, when it says how much was used; another window is none
        let top = from_event(&json!({"status": "allowed_warning", "resetsAt": now + 300_000, "rateLimitType": "seven_day", "utilization": 0.9}));
        assert_eq!((top.five_hour, top.seven_day, top.five_hour_limit, top.verdict), (None, Some(Window { used_percent: 90.0, resets_at: now + 300_000 }), None, Verdict::In));
        let opus = from_event(&json!({"status": "allowed", "resetsAt": now + 300_000, "rateLimitType": "seven_day_opus", "utilization": 0.9}));
        assert_eq!((opus.five_hour, opus.seven_day, opus.five_hour_limit), (None, None, None));
        // a refused one: out until the reset it names, past its cap; the windows are kept
        let rejected = from_event(&json!({"status": "rejected", "resetsAt": now + 600, "rateLimitType": "five_hour", "unifiedWindows": {"five_hour": {"utilization": 1.02, "resetsAt": now + 600, "status": "rejected"}}}));
        assert_eq!((&rejected.verdict, rejected.out_for), (&Verdict::Out(Some(now + 600)), Some(RATE_LIMIT)));
        let mut s = State { signed_in: true, ..State::default() };
        apply(&mut s, &rejected, now);
        assert_eq!((s.out_until, s.five_hour_limit, s.five_hour.map(|w| w.used_percent.round())), (Some(now + 600), Some(true), Some(102.0)));
        // a limited turn's row reads the same object with the same reader
        assert_eq!(named(&json!({"resetsAt": 5.0, "rateLimitType": "seven_day"})), Some(("seven_day", 5)));
        assert_eq!(named(&json!({"rateLimitType": "five_hour"})), None);
    }
}
