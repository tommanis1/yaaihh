//! The window opener of the owner's sign-in accounts. A 5-hour window starts at the first request after the last one
//! reset, so an unused account starts its next window late; the opener sends it one tiny message so a new window starts
//! at once. Its numbers are policy text (agent-base/policy/accounts.eflint; the plans' weekly ceilings are
//! placement.eflint's), and everything said to Anthropic is said by Claude Code itself, in the account's folder:
//! - the rule is taken on what is on file of the account (its sessions' status lines, the owner's last `Check limits`,
//!   the last opening's answer); the round reads nothing of its own, so a window opened outside coordd is not seen until
//!   a session or his press says it;
//! - an opening is `claude -p PROMPT --model MODEL` there in stream-json, at most one per account within the idle
//!   wait ([`State::window_opened`]). The `rate_limit_event` its answer brings says the 5-hour window that runs and
//!   whether the account has a 5-hour limit ([`opened`]); an answer that says no 5-hour window is taken to have started
//!   one, looked at again once it would have ended.
//!
//! Every coordd of his runs the round. A look is claimed in the account's `state.json` under its lock
//! ([`State::window_look`]), so one coordd looks, and the others see when the next look is due.

use super::{limits, signin, Entry, Harness, Kind, State, Store};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

/// The opener's rule from the policy (`policy::opener`): in seconds, when an account is looked at again with its window
/// running (after its reset), after an opening whose answer said no 5-hour window (the window it started taken to run
/// that long), in a state only use or a new week changes (no 5-hour limit, at or over its ceiling, taking no requests)
/// and within which no second opening is made, and after an opening that failed; the opening's model and prompt; and
/// the weekly ceilings by plan as Claude Code names it (`pro`, `max`).
#[derive(Debug, Clone, PartialEq)]
pub struct Times {
    pub after_reset: i64,
    pub after_opening: i64,
    pub idle: i64,
    pub failed: i64,
    pub model: String,
    pub prompt: String,
    pub ceilings: BTreeMap<String, u64>,
}

/// What the rule does with an account at a look.
#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    /// Its window is to be opened now.
    Open,
    /// It is looked at again after these seconds, for this reason (the journal's words, said when they change).
    Wait(i64, String),
}

/// The gateway's rule on STATE, as on file, at NOW: having no 5-hour limit ([`State::five_hour_limit`]; one not known
/// counts as having one, so that its opening's answer says), its window running, its week at or over its plan's
/// ceiling, or its taking no requests (out, deaf, being removed) each wait; otherwise its window is to be opened.
pub fn next(state: &State, t: &Times, now: i64) -> Next {
    let plan = state.plan.as_deref().unwrap_or("plan not known");
    if state.five_hour_limit == Some(false) {
        return Next::Wait(t.idle, format!("no 5-hour limit on it ({plan})"));
    }
    if let Some(w) = state.five_hour.as_ref().filter(|w| w.resets_at > now) {
        return Next::Wait(w.resets_at - now + t.after_reset, format!("window open until {}", crate::core::at_secs(w.resets_at)));
    }
    let ceiling = state.plan.as_ref().and_then(|p| t.ceilings.get(p));
    if let (Some(c), Some(week)) = (ceiling, state.seven_day.as_ref().filter(|w| w.resets_at > now)) {
        if week.used_percent >= *c as f64 {
            return Next::Wait(t.idle, format!("window closed, not opened: weekly {:.0}% is at or over the {plan} ceiling {c}%", week.used_percent));
        }
    }
    if !state.can_take(now) {
        return Next::Wait(t.idle, "window closed, not opened: it takes no requests now".into());
    }
    Next::Open
}

/// Whether the opener looks at E at all: a signed-in sign-in account of Claude Code's, not being removed, and not out
/// for what only the owner mends (its login refused).
fn looked_at(e: &Entry) -> bool {
    let s = &e.state;
    e.harness == Harness::Claude && e.account.kind == Kind::Signin && s.signed_in && s.leaving.is_none() && !s.out_for.as_deref().is_some_and(super::limits::owners_to_clear)
}

/// The accounts of STORE whose look is due at NOW and that this coordd has claimed (`window_look` set a failed
/// opening's wait ahead under the account's lock, so no other coordd looks too, and one that ended mid-look is taken
/// up after it), for the caller to look at. An account whose window is on file as running is not claimed: its next
/// look is set for after that window's reset. Blocking (small files): not under the core's lock.
pub fn due(store: &Store, t: &Times, now: i64) -> Vec<String> {
    let mut claimed = Vec::new();
    for e in store.list(Harness::Claude).into_iter().filter(looked_at) {
        if e.state.window_look.is_some_and(|at| at > now) {
            continue;
        }
        let mut mine = false;
        let _ = store.update(Harness::Claude, &e.id, |s| {
            if s.window_look.is_some_and(|at| at > now) {
                return;
            }
            match s.five_hour.as_ref().filter(|w| w.resets_at > now) {
                Some(w) => s.window_look = Some(w.resets_at + t.after_reset),
                None => (s.window_look, mine) = (Some(now + t.failed), true),
            }
        });
        if mine {
            claimed.push(e.id);
        }
    }
    claimed
}

/// The look at an account at NOW, on what is on file of it (STATE), written to STATE: the words for the journal when
/// they changed (the journal only: a look says nothing to anyone), and whether its window is opened now. The rule
/// ([`next`]) opens it unless an opening went within T's idle wait (one that failed, or one whose coordd ended
/// mid-opening, taken up after it); an opening counts from here, whatever comes of it ([`State::window_opened`]).
pub fn look(state: &mut State, t: &Times, now: i64) -> (Option<String>, bool) {
    match (next(state, t, now), state.window_opened.filter(|at| now - at < t.idle)) {
        (Next::Wait(secs, why), _) => (after(state, secs, why, now), false),
        (Next::Open, Some(at)) => (after(state, t.idle, format!("opened at {} and no window said to run since: not opened again within {} s", crate::core::at_secs(at), t.idle), now), false),
        (Next::Open, None) => {
            state.window_opened = Some(now);
            (None, true)
        }
    }
}

/// The opening's RUN ([`open`]: what Claude Code printed, or why it failed), written to STATE at NOW: the last
/// `rate_limit_event` it printed, taken as a signal ([`limits::from_event`]), and then the next look as the rule
/// ([`next`]) puts it: at the reset of the 5-hour window it said, or later for a state the opening does not change
/// (no 5-hour limit, the week at its ceiling, out). An answer that said no 5-hour window (no event, or one of another
/// window) is taken to have started one: looked at again after T's after-opening wait. One that failed: after T's
/// failed wait. The words for the journal when they changed.
pub fn opened(state: &mut State, run: Result<String, String>, t: &Times, now: i64) -> Option<String> {
    let printed = match run {
        Ok(printed) => printed,
        Err(why) => return after(state, t.failed, format!("opening failed ({why}); looked at again in {} s", t.failed), now),
    };
    if let Some(event) = printed.lines().rev().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).find(|v| v["type"] == "rate_limit_event") {
        limits::apply(state, &limits::from_event(&event["rate_limit_info"]), now);
    }
    let plan = state.plan.clone().unwrap_or_else(|| "plan not known".into());
    match next(state, t, now) {
        Next::Wait(secs, why) => after(state, secs, format!("window opened ({plan}): {why}"), now),
        Next::Open => {
            let wait = t.after_opening + t.after_reset;
            after(state, wait, format!("window opened ({plan}): Claude Code said no 5-hour window of it, so the one it started is taken to run; looked at again in {wait} s"), now)
        }
    }
}

/// After the look at an account: when it is looked at next and why (NEXT, a wait), written to STATE; the words to say
/// in the journal when they are not what the last look said.
pub fn after(state: &mut State, wait: i64, why: String, now: i64) -> Option<String> {
    state.window_look = Some(now + wait);
    let new = state.window_said.as_deref() != Some(why.as_str());
    state.window_said = Some(why.clone());
    new.then_some(why)
}

/// Claude Code's own `get_usage` in the account's folder CONFIG, as PROGRAM in print mode with the sign-in's
/// environment: its answer ([`limits::from_usage`] reads it), or why there is none. On the owner's press only (main.rs
/// `account_windows`), never in the round. PROGRAM is given no prompt, so nothing goes to a model: [`USAGE_REQUEST`]
/// is the one line on its input, a control request as the Agent SDK's `usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_
/// THIS_API_YET({skipBehaviors: true})` sends it (2.1.330 answers it with no `initialize` before it). Claude Code fetches
/// the limits from its usage endpoint with its own login. Its answer holds the plan, the windows and this session's
/// totals, and nothing of the login, which coordd neither reads nor passes on. Once the answer came, its input ends,
/// and so does PROGRAM; within [`READ_WAIT`], or it is stopped.
pub async fn read(program: &Path, config: &Path) -> Result<serde_json::Value, String> {
    let what = format!("{} get_usage", program.display());
    let mut cmd = read_command(program, config);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| format!("{what}: {e}"))?;
    let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else { return Err(format!("{what}: no pipes")) };
    let asked = async {
        input.write_all(USAGE_REQUEST.as_bytes()).await.map_err(|e| e.to_string())?;
        let mut lines = BufReader::new(output).lines();
        while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
            if let Some(answer) = usage_answer(&line) {
                return Ok(Some(answer));
            }
        }
        Ok::<_, String>(None)
    };
    let answer = tokio::time::timeout(READ_WAIT, asked).await;
    drop(input);
    let answer = match answer {
        Err(_) => return Err(format!("{what}: no answer within {} s", READ_WAIT.as_secs())),
        Ok(Err(e)) => return Err(format!("{what}: {e}")),
        Ok(Ok(answer)) => answer,
    };
    let out = tokio::time::timeout(READ_WAIT, child.wait_with_output()).await.map_err(|_| format!("{what}: did not end once asked"))?.map_err(|e| format!("{what}: {e}"))?;
    match answer {
        Some(answer) => answer.map_err(|e| format!("{what}: {e}")),
        None => Err(format!("{what} {} with no answer: {}", signin::how(out.status), signin::plain(&String::from_utf8_lossy(&out.stderr)).trim())),
    }
}

/// The request [`read`] writes: one line of stream-json, the limits alone (`skip_behaviors`: no scan of the local
/// transcripts).
pub const USAGE_REQUEST: &str = "{\"type\":\"control_request\",\"request_id\":\"coordd-usage\",\"request\":{\"subtype\":\"get_usage\",\"skip_behaviors\":true}}\n";

/// How long [`read`] waits for the answer, and then for PROGRAM's end.
pub const READ_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Whether LINE of [`read`]'s output is the answer to [`USAGE_REQUEST`]: Ok with its `response`, or Err with the error
/// it said; None for any other line.
fn usage_answer(line: &str) -> Option<Result<serde_json::Value, String>> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let r = &v["response"];
    if v["type"] != "control_response" || r["request_id"] != "coordd-usage" {
        return None;
    }
    Some(match r["subtype"].as_str() {
        Some("success") => Ok(r["response"].clone()),
        _ => Err(r["error"].as_str().unwrap_or("an error with no words").to_string()),
    })
}

/// The command [`read`] runs: print mode with its input and output in stream-json (2.1.330 gives stream-json in print
/// mode only with `--verbose`).
fn read_command(program: &Path, config: &Path) -> Command {
    command(program, config, &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose"])
}

/// One opening message on the account whose folder is CONFIG: T's prompt on T's model, sent by PROGRAM itself in print
/// mode with its login there ([`open_command`]); no session of it is kept. What it printed, for [`opened`].
pub async fn open(program: &Path, config: &Path, t: &Times) -> Result<String, String> {
    signin::run(open_command(program, config, t), &format!("{} -p {:?} --model {}", program.display(), t.prompt, t.model)).await
}

/// The command [`open`] runs: its answer in stream-json, one message a line, its `rate_limit_event` among them;
/// 2.1.329 gives stream-json in print mode only with `--verbose` ("stream-json requires --verbose").
fn open_command(program: &Path, config: &Path, t: &Times) -> Command {
    command(program, config, &["-p", &t.prompt, "--model", &t.model, "--output-format", "stream-json", "--verbose"])
}

/// PROGRAM with ARGS in print mode under CONFIG, as signin.rs runs Claude Code there (nothing of coordd's environment),
/// with no session kept, no MCP server, and its times in UTC.
fn command(program: &Path, config: &Path, args: &[&str]) -> Command {
    let mut cmd = signin::command(program, config);
    cmd.args(args).args(["--no-session-persistence", "--strict-mcp-config"]).env("TZ", "UTC");
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{testing::Tmp, Window};
    use serde_json::json;

    fn times() -> Times {
        Times {
            after_reset: 30,
            after_opening: 18_000,
            idle: 1800,
            failed: 600,
            model: "claude-haiku-5-5".into(),
            prompt: "Respond with zoo7".into(),
            ceilings: [("pro".to_string(), 70), ("max".to_string(), 80)].into(),
        }
    }

    /// An opening's answer as 2.1.329's stream-json prints it, made up: one message a line, its `rate_limit_event`
    /// (`Fwa`: `{type, rate_limit_info, uuid, session_id}`) with INFO between the others.
    fn printed(info: serde_json::Value) -> String {
        [
            json!({"type": "system", "subtype": "init", "session_id": "s"}),
            json!({"type": "rate_limit_event", "rate_limit_info": info, "uuid": "u", "session_id": "s"}),
            json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "zoo7"}]}}),
            json!({"type": "result", "subtype": "success", "is_error": false}),
        ]
        .map(|v| v.to_string())
        .join("\n")
    }

    #[test]
    fn the_rule_is_the_gateways_no_limit_open_window_ceiling_and_taking_none_wait_and_the_rest_is_opened() {
        let (t, now) = (times(), 1_000_000);
        let fine = State { signed_in: true, plan: Some("max".into()), ..State::default() };
        assert_eq!(next(&fine, &t, now), Next::Open, "a 5-hour limit not known counts as one");
        assert_eq!(next(&State { five_hour_limit: Some(true), ..fine.clone() }, &t, now), Next::Open);
        assert!(matches!(next(&State { five_hour_limit: Some(false), ..fine.clone() }, &t, now), Next::Wait(1800, w) if w == "no 5-hour limit on it (max)"));
        let open = State { five_hour: Some(Window { used_percent: 12.0, resets_at: now + 600 }), ..fine.clone() };
        assert!(matches!(next(&open, &t, now), Next::Wait(630, w) if w.starts_with("window open until")));
        // a window that has reset is closed
        let reset = State { five_hour: Some(Window { used_percent: 12.0, resets_at: now - 1 }), ..fine.clone() };
        assert_eq!(next(&reset, &t, now), Next::Open);
        let week = |used: f64, plan: &str| State { plan: Some(plan.into()), seven_day: Some(Window { used_percent: used, resets_at: now + 86_400 }), ..fine.clone() };
        assert!(matches!(next(&week(80.0, "max"), &t, now), Next::Wait(1800, w) if w == "window closed, not opened: weekly 80% is at or over the max ceiling 80%"));
        assert_eq!(next(&week(79.0, "max"), &t, now), Next::Open);
        assert!(matches!(next(&week(70.0, "pro"), &t, now), Next::Wait(1800, _)));
        assert_eq!(next(&week(99.0, "team"), &t, now), Next::Open, "a plan with no ceiling");
        assert_eq!(next(&State { seven_day: Some(Window { used_percent: 95.0, resets_at: now - 1 }), ..week(0.0, "max") }, &t, now), Next::Open, "last week's figure counts no more");
        let deaf = State { deaf: Some(crate::accounts::Sign { since: now, agent: "w".into() }), ..fine.clone() };
        assert!(matches!(next(&deaf, &t, now), Next::Wait(1800, w) if w.contains("takes no requests")));
        // one opening within the idle wait: a look after one that failed, or was taken up after its coordd ended
        let mut opened = State { window_opened: Some(now), ..fine.clone() };
        let (said, open_now) = look(&mut opened, &t, now + 120);
        assert!(!open_now && said.is_some_and(|w| w.contains("not opened again within 1800 s")));
        assert_eq!(opened.window_look, Some(now + 120 + 1800));
        assert_eq!(look(&mut State { window_opened: Some(now - 1800), ..fine.clone() }, &t, now), (None, true), "once the idle time is over");
        let mut running = State { five_hour: Some(Window { used_percent: 1.0, resets_at: now + 900 }), ..opened };
        assert!(!look(&mut running, &t, now + 120).1);
        assert_eq!(running.window_look, Some(now + 900 + 30), "the window it opened runs: until its reset");
    }

    #[test]
    fn an_opening_reads_its_own_rate_limit_event_and_one_that_says_no_window_is_not_made_again_inside_it() {
        let (t, now) = (times(), 1_000_000);
        let fine = State { signed_in: true, plan: Some("max".into()), ..State::default() };
        // its event names the window it started: looked at again after that window's reset, its windows on file
        let mut s = fine.clone();
        assert_eq!(look(&mut s, &t, now), (None, true));
        let info = json!({
            "status": "allowed", "resetsAt": now + 18_000, "rateLimitType": "five_hour", "utilization": 0.01,
            "unifiedWindows": {"five_hour": {"utilization": 0.01, "resetsAt": now + 18_000}, "seven_day": {"utilization": 0.06, "resetsAt": now + 400_000, "status": "allowed"}},
        });
        let said = opened(&mut s, Ok(printed(info)), &t, now + 5);
        assert!(said.is_some_and(|w| w.starts_with("window opened (max): window open until")));
        assert_eq!((s.five_hour.as_ref().map(|w| w.resets_at), s.five_hour_limit, s.limits_from.as_str()), (Some(now + 18_000), Some(true), "rate_limit_event"));
        assert_eq!(s.seven_day.as_ref().map(|w| (w.used_percent.round(), w.resets_at)), Some((6.0, now + 400_000)));
        assert_eq!(s.window_look, Some(now + 18_000 + 30));
        // without `unifiedWindows`: the one window its top level names
        let mut top = fine.clone();
        opened(&mut top, Ok(printed(json!({"status": "allowed_warning", "resetsAt": now + 9_000, "rateLimitType": "five_hour", "utilization": 0.8}))), &t, now);
        assert_eq!((top.five_hour.map(|w| (w.used_percent.round(), w.resets_at)), top.five_hour_limit), (Some((80.0, now + 9_000)), Some(true)));
        // a failed opening: looked at again after the failed wait, and no second one within the idle wait
        let mut failed = fine.clone();
        look(&mut failed, &t, now);
        assert!(opened(&mut failed, Err("claude exited 1".into()), &t, now + 5).is_some_and(|w| w.starts_with("opening failed (claude exited 1)")));
        assert_eq!(failed.window_look, Some(now + 5 + 600));
        assert!(!look(&mut failed, &t, now + 605).1);
        // an answer with no event, or one of another window, through the round of the owner's coordds: none
        // claims the account again inside the window the opening is taken to have started, and the next opening
        // comes after it
        let store = Tmp::new("opener-no-event");
        let h = Harness::Claude;
        store.store.create(h, "a", "a", 10, Kind::Signin, "x").unwrap();
        for answer in ["{\"type\":\"result\",\"subtype\":\"success\"}".to_string(), printed(json!({"status": "allowed", "resetsAt": now + 400_000, "rateLimitType": "seven_day_opus"}))] {
            store.store.update(h, "a", |s| *s = fine.clone()).unwrap();
            assert_eq!(due(&store.store, &t, now), ["a"]);
            store.store.update(h, "a", |s| assert_eq!(look(s, &t, now), (None, true))).unwrap();
            let mut said = None;
            store.store.update(h, "a", |s| said = opened(s, Ok(answer.clone()), &t, now + 5)).unwrap();
            assert!(said.is_some_and(|w| w.contains("said no 5-hour window of it")), "{answer}");
            let s = store.store.state(h, "a").unwrap();
            assert_eq!((s.five_hour, s.five_hour_limit, s.window_look), (None, None, Some(now + 5 + 18_030)));
            for later in [120, 1_801, 18_000] {
                assert!(due(&store.store, &t, now + later).is_empty(), "{later} s after it");
            }
            assert_eq!(due(&store.store, &t, now + 5 + 18_030), ["a"]);
            store.store.update(h, "a", |s| assert_eq!(look(s, &t, now + 5 + 18_030), (None, true), "once it would have ended")).unwrap();
        }
    }

    #[test]
    fn an_account_with_no_5_hour_limit_gets_no_opening_and_one_on_file_from_before_gets_one_that_says() {
        let (t, now) = (times(), 1_000_000);
        // his `Check limits` read it with no 5-hour window: no opening, and none after
        let mut none = State { signed_in: true, plan: Some("team".into()), ..State::default() };
        let weekly = json!({"subscription_type": "team", "rate_limits_available": true, "rate_limits": {"five_hour": null, "seven_day": {"utilization": 3.0, "resets_at": "2026-11-24T09:00:00Z"}}});
        limits::apply(&mut none, &limits::from_usage(&weekly).unwrap(), now);
        assert_eq!(none.five_hour_limit, Some(false));
        let (said, open) = look(&mut none, &t, now);
        assert!(!open && said.is_some_and(|w| w == "no 5-hour limit on it (team)"));
        // a session's 5-hour window says it has one, whatever was read before
        let mut seen = none.clone();
        limits::apply(&mut seen, &limits::from_statusline(&json!({"rate_limits": {"five_hour": {"used_percentage": 4.0, "resets_at": now + 600}}})), now);
        assert_eq!(seen.five_hour_limit, Some(true));
        // state.json as coordd wrote it before it kept whether there is a 5-hour limit (1c391137): its window ran out
        // a while ago. The first round after a start looks at it and opens it once; the opening's event, whose
        // windows hold no 5-hour one, says it has none, and it is not opened again
        let tmp = Tmp::new("opener-before");
        let h = Harness::Claude;
        tmp.store.create(h, "a", "a", 10, Kind::Signin, "x").unwrap();
        let before: State = serde_json::from_value(json!({
            "signed_in": true, "signed_in_seen": 1, "plan": "team",
            "five_hour": {"used_percent": 40.0, "resets_at": now - 3_600}, "limits_seen": now - 20_000, "limits_from": "statusline",
            "window_look": now - 10, "window_said": "no 5-hour limit on it (team)",
        }))
        .unwrap();
        assert_eq!(before.five_hour_limit, None);
        tmp.store.update(h, "a", |s| *s = before.clone()).unwrap();
        assert_eq!(due(&tmp.store, &t, now), ["a"]);
        tmp.store.update(h, "a", |s| assert_eq!(look(s, &t, now), (None, true))).unwrap();
        let info = json!({"status": "allowed", "resetsAt": now + 400_000, "rateLimitType": "seven_day", "utilization": 0.03, "unifiedWindows": {"seven_day": {"utilization": 0.03, "resetsAt": now + 400_000}}});
        let mut said = None;
        tmp.store.update(h, "a", |s| said = opened(s, Ok(printed(info.clone())), &t, now + 5)).unwrap();
        assert_eq!(said.as_deref(), Some("window opened (team): no 5-hour limit on it (team)"));
        let s = tmp.store.state(h, "a").unwrap();
        assert_eq!((s.five_hour_limit, s.window_look), (Some(false), Some(now + 5 + 1800)));
        assert_eq!(due(&tmp.store, &t, now + 5 + 1800), ["a"]);
        tmp.store.update(h, "a", |s| assert!(!look(s, &t, now + 5 + 1800).1, "none again")).unwrap();
    }

    #[test]
    fn the_read_asks_get_usage_with_no_prompt_and_takes_only_its_answer() {
        let cmd = read_command(Path::new("claude"), Path::new("/acc/claude/a/config"));
        let args: Vec<_> = cmd.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--strict-mcp-config"]);
        let env: Vec<_> = cmd.as_std().get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        assert!(env.contains(&("CLAUDE_CONFIG_DIR".into(), Some("/acc/claude/a/config".into()))));
        let asked: serde_json::Value = serde_json::from_str(USAGE_REQUEST).unwrap();
        assert_eq!(asked, json!({"type": "control_request", "request_id": "coordd-usage", "request": {"subtype": "get_usage", "skip_behaviors": true}}));
        assert!(USAGE_REQUEST.ends_with('\n') && USAGE_REQUEST.matches('\n').count() == 1, "one line");
        // its answer as 2.1.330 gave it with no login; other lines and other requests' answers are not it
        let answer = r#"{"type":"control_response","response":{"subtype":"success","request_id":"coordd-usage","response":{"session":{"total_cost_usd":0},"subscription_type":null,"rate_limits_available":false,"rate_limits":null,"behaviors":null}}}"#;
        assert_eq!(usage_answer(answer), Some(Ok(json!({"session": {"total_cost_usd": 0}, "subscription_type": null, "rate_limits_available": false, "rate_limits": null, "behaviors": null}))));
        let failed = r#"{"type":"control_response","response":{"subtype":"error","request_id":"coordd-usage","error":"get_usage failed"}}"#;
        assert_eq!(usage_answer(failed), Some(Err("get_usage failed".into())));
        for other in [r#"{"type":"control_response","response":{"subtype":"success","request_id":"other","response":{}}}"#, r#"{"type":"system","subtype":"init"}"#, "not json"] {
            assert_eq!(usage_answer(other), None, "{other}");
        }
        // the opening, whole: stream-json needs `--verbose` in print mode
        let opening = open_command(Path::new("claude"), Path::new("/acc/claude/a/config"), &times());
        let args: Vec<_> = opening.as_std().get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, ["-p", "Respond with zoo7", "--model", "claude-haiku-5-5", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--strict-mcp-config"]);
    }

    #[test]
    fn a_look_is_claimed_once_among_coordds_and_one_with_a_running_window_is_put_after_its_reset_unread() {
        let t = Tmp::new("opener-due");
        let (h, now, times) = (Harness::Claude, 1_000_000, times());
        // a and b signed in; b's window is running; c is a key account; d is refused, for the owner
        for id in ["a", "b"] {
            t.store.create(h, id, id, 10, Kind::Signin, "x").unwrap();
            t.store.update(h, id, |s| s.signed_in = true).unwrap();
        }
        t.store.update(h, "b", |s| s.five_hour = Some(Window { used_percent: 3.0, resets_at: now + 100 })).unwrap();
        t.store.create(h, "c", "C", 10, Kind::Key, "x").unwrap();
        t.store.update(h, "c", |s| s.signed_in = true).unwrap();
        t.store.create(h, "d", "D", 10, Kind::Signin, "x").unwrap();
        t.store.update(h, "d", |s| (s.signed_in, s.out_until, s.out_for) = (true, Some(crate::accounts::limits::UNKNOWN_RESET), Some(crate::accounts::limits::REFUSED.into()))).unwrap();
        assert_eq!(due(&t.store, &times, now), ["a"]);
        assert_eq!(t.store.state(h, "a").unwrap().window_look, Some(now + 600));
        assert_eq!(t.store.state(h, "b").unwrap().window_look, Some(now + 130));
        // another coordd's round a moment later finds nothing due
        assert!(due(&t.store, &times, now + 5).is_empty());
        // a look that never wrote back is taken up after a failed read's wait; b's window has reset by then, and b is
        // read after it
        assert_eq!(due(&t.store, &times, now + 600), ["a", "b"]);
        let mut s = t.store.state(h, "a").unwrap();
        assert_eq!(after(&mut s, 1800, "no 5-hour limit on it (max)".into(), now), Some("no 5-hour limit on it (max)".to_string()));
        assert_eq!(after(&mut s, 1800, "no 5-hour limit on it (max)".into(), now + 1800), None, "said once");
        assert_eq!(s.window_look, Some(now + 3600));
    }
}
