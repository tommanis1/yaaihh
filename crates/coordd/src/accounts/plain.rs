//! A direct plain completion on an OpenCode Go account (walk #26): one chat completion to Go's OpenAI-compatible
//! endpoint with the call's prompt as the one user message, its answer held to a JSON schema. No agent system prompt,
//! no tools, no loop, and no opencode process: a free Go model answers in seconds where an opencode agent takes tens.
//!
//! The key is the account's usage-key file ([`Store::usage_key_file`]), one `Authorization` header that curl reads
//! itself (`--header @FILE`), so coordd never holds it, as [`super::opencode::go_usage`] does. Go routes a request
//! only with an `x-opencode-session` header, as opencode 1.18.34 sends it (`ses_` and a fresh id per call).

use super::{Entry, Harness, Store};
use crate::policy::call::{self, Failure};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// Go's chat completions (the `api` of `opencode-go` in opencode's models.json, 1.18.34).
pub const GO_CHAT_URL: &str = "https://opencode.ai/inference/go/openai/v1/chat/completions";

/// The longest one plain call may take; then curl is ended and the policy's next way is tried.
pub const PLAIN_BOUND: Duration = Duration::from_secs(10);

/// After this long into a task's chain, no new Go model is begun (`policy::chain_skips`): two get their turn, and the
/// chain's other models theirs.
pub const PLAIN_ALL: Duration = Duration::from_secs(20);

/// The answer's room: Go's free models reason before they answer, and that counts against `max_tokens`.
const MAX_TOKENS: u64 = 8000;

/// The id Go takes for MODEL (`opencode-go/ID` or `opencode-go/ID:EFFORT`): ID, when MODEL is one of Go's.
pub fn go_model(model: &str) -> Option<&str> {
    call::model_effort(model).0.strip_prefix(super::opencode::GO).and_then(|m| m.strip_prefix('/')).filter(|m| !m.is_empty())
}

/// The plain request for PROMPT on Go model ID: the gateway's body ([`call::request_body`]): one user message and a
/// strict `response_format`, with no `tools`, `tool_choice` or system message, and room for the model's reasoning.
pub fn body(name: &str, prompt: &str, schema: &Value, id: &str, effort: Option<&str>) -> Value {
    let model = effort.map_or_else(|| id.to_string(), |e| format!("{id}:{e}"));
    let mut b = call::request_body(name, prompt, schema, &model);
    b["max_tokens"] = MAX_TOKENS.into();
    b
}

/// The Go accounts that can take a plain call on model ID now: signed in, not switched off or leaving, with a usage
/// key; a paid model only on an account that is not out (a free one costs Go's limits nothing).
pub fn accounts(store: &Store, id: &str, now: i64) -> Vec<String> {
    let free = id.ends_with("-free");
    let ok = |e: &Entry| {
        e.account.provider.as_deref() == Some(super::opencode::GO)
            && e.state.signed_in
            && e.state.off.is_none()
            && e.state.leaving.is_none()
            && (free || e.state.out_until.is_none_or(|t| t <= now))
            && store.usage_key_set(&e.id)
    };
    store.list(Harness::Opencode).into_iter().filter(ok).map(|e| e.id).collect()
}

/// A session id as opencode makes one: `ses_` and 26 hex digits, fresh for each call.
fn session() -> String {
    let mut b = [0u8; 13];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        b.copy_from_slice(&(t ^ u128::from(std::process::id())).to_le_bytes()[..13]);
    }
    format!("ses_{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())
}

/// BODY sent by CURL with the header file KEY, ended at DEADLINE; its answer held to CHECK, and the tokens Go counted.
/// curl is told the time left too, so a slow answer ends with curl's own words before the deadline kills it.
pub fn call(curl: &Path, key: &Path, body: &Value, check: &dyn Fn(&Value) -> Result<(), String>, deadline: Instant) -> Result<(Value, Option<u64>), Failure> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Failure::Error("no time was left for the plain call".into()));
    }
    let mut header = std::ffi::OsString::from("@");
    header.push(key);
    let mut cmd = std::process::Command::new(curl);
    cmd.args(["--disable", "--silent", "--show-error", "--proto", "=https", "--max-time", &format!("{:.1}", left.as_secs_f64())]);
    cmd.args(["--write-out", "\n%{http_code}", "--header", "Content-Type: application/json", "--header", &format!("x-opencode-session: {}", session())]);
    cmd.arg("--header").arg(header).args(["--data-binary", "@-", GO_CHAT_URL]);
    cmd.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).process_group(0);
    let ran = |e: std::io::Error| Failure::Error(format!("curl: {e}"));
    let mut child = cmd.spawn().map_err(ran)?;
    let mut input = child.stdin.take().ok_or_else(|| Failure::Error("curl: no input".into()))?;
    let sent = call::head_first(body);
    std::thread::spawn(move || input.write_all(sent.as_bytes()));
    let Some(out) = super::router::output_by(child, deadline).map_err(ran)? else {
        return Err(Failure::Error(format!("Go gave no answer within the plain call's {:.0} s, and curl was ended", left.as_secs_f64())));
    };
    if !out.status.success() {
        return Err(Failure::Error(format!("curl ({}): {}", out.status, String::from_utf8_lossy(&out.stderr).trim())));
    }
    read(&String::from_utf8_lossy(&out.stdout), check)
}

/// curl's output, the body then a line with the HTTP code, as an answer held to CHECK and its tokens. A code other than
/// 200 is an error with Go's words: a 429 keeps its code first, as the gateway's does, so a limit reads as one.
fn read(out: &str, check: &dyn Fn(&Value) -> Result<(), String>) -> Result<(Value, Option<u64>), Failure> {
    let (body, code) = out.rsplit_once('\n').unwrap_or(("", out));
    let v: Value = serde_json::from_str(body).unwrap_or_default();
    match code.trim() {
        "200" => call::answer_with(&v, check).map(|a| (a, call::completion_tokens(&v))),
        c => {
            let said = v["error"]["message"].as_str().map_or_else(|| body.trim().chars().take(200).collect(), str::to_string);
            let kind = v["error"]["type"].as_str().map_or(String::new(), |t| format!(" {t}"));
            Err(Failure::Error(format!("{c}{kind}: {said}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        json!({"type": "object", "properties": {"summary": {"type": "string"}}, "required": ["summary"], "additionalProperties": false})
    }

    fn any(_: &Value) -> Result<(), String> {
        Ok(())
    }

    /// A stand-in curl in DIR: it keeps its arguments and stdin beside it, then runs SCRIPT. A child `sh` writes it, so
    /// no fd of this process (inherited by another test's fork) holds it open for writing at its exec (ETXTBSY).
    fn curl(dir: &Path, script: &str) -> std::path::PathBuf {
        let p = dir.join("curl");
        let mut w = std::process::Command::new("sh").args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"]).arg(&p).stdin(std::process::Stdio::piped()).spawn().unwrap();
        let text = format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > {d}/args\ncat > {d}/stdin\n{script}\n", d = dir.display());
        w.stdin.take().unwrap().write_all(text.as_bytes()).unwrap();
        assert!(w.wait().unwrap().success());
        p
    }

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("coordd-plain-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn only_gos_models_take_the_plain_way() {
        assert_eq!(go_model("opencode-go/hy4-flash-free"), Some("hy4-flash-free"));
        assert_eq!(go_model("opencode-go/beam-free:low"), Some("beam-free"));
        assert_eq!(go_model("opencode/big-pickle"), None, "Zen's are not Go's");
        assert_eq!(go_model("opencode-go/"), None);
        assert_eq!(go_model("gpt-6.2-sol"), None);
    }

    #[test]
    fn the_request_is_one_user_message_with_a_schema_and_no_tools_or_system() {
        let b = body("ahead", "Annotate this.", &schema(), "hy4-flash-free", None);
        assert_eq!(b["model"], "hy4-flash-free");
        assert_eq!(b["messages"], json!([{"role": "user", "content": "Annotate this."}]), "the prompt alone, no system message");
        for k in ["tools", "tool_choice", "functions", "system", "stream"] {
            assert!(b.get(k).is_none(), "no {k}: {b}");
        }
        assert_eq!(b["response_format"]["json_schema"]["strict"], true);
        assert_eq!(b["response_format"]["json_schema"]["schema"]["properties"]["answer"], schema());
        assert_eq!(b["max_tokens"], MAX_TOKENS);
        assert_eq!(body("a", "p", &schema(), "beam-free", Some("low"))["reasoning_effort"], "low");
    }

    #[test]
    fn curl_reads_the_key_from_its_file_and_the_body_from_stdin() {
        let d = dir("args");
        let answer = json!({"choices": [{"finish_reason": "stop", "message": {"content": "{\"answer\":{\"summary\":\"s\"}}"}}], "usage": {"total_tokens": 42}});
        let c = curl(&d, &format!("printf '%s\\n200' '{answer}'"));
        let b = body("ahead", "the prompt", &schema(), "beam-free", None);
        let got = call(&c, Path::new("/k/usage-key"), &b, &any, Instant::now() + Duration::from_secs(10)).unwrap();
        assert_eq!(got, (json!({"summary": "s"}), Some(42)), "the answer and the tokens Go counted");
        let args: Vec<String> = std::fs::read_to_string(d.join("args")).unwrap().lines().map(str::to_string).collect();
        assert!(args.contains(&"@/k/usage-key".into()), "the key's header from its file: {args:?}");
        assert!(args.iter().any(|a| a.starts_with("x-opencode-session: ses_") && a.len() == "x-opencode-session: ses_".len() + 26), "{args:?}");
        assert!(args.contains(&"=https".into()) && args.contains(&GO_CHAT_URL.into()) && args.contains(&"@-".into()));
        assert!(args.windows(2).any(|w| w[0] == "--max-time" && w[1].parse::<f64>().is_ok_and(|s| s > 0.0 && s <= 10.0)), "{args:?}");
        let sent: Value = serde_json::from_str(&std::fs::read_to_string(d.join("stdin")).unwrap()).unwrap();
        assert_eq!(sent, b, "the body as built");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn an_error_says_gos_code_and_words_and_a_bad_answer_is_invalid() {
        let d = dir("errors");
        let b = body("ahead", "p", &schema(), "beam-free", None);
        let soon = || Instant::now() + Duration::from_secs(10);
        let c = curl(&d, r#"printf '%s\n429' '{"error":{"type":"FreeUsageLimitError","message":"Free usage limit reached."}}'"#);
        assert_eq!(call(&c, Path::new("k"), &b, &any, soon()), Err(Failure::Error("429 FreeUsageLimitError: Free usage limit reached.".into())));
        let c = curl(&d, r#"printf '%s\n200' '{"choices":[{"finish_reason":"stop","message":{"content":"sure, here it is"}}]}'"#);
        assert!(matches!(call(&c, Path::new("k"), &b, &any, soon()), Err(Failure::Invalid(e)) if e.starts_with("not JSON")));
        let c = curl(&d, r#"printf '%s\n200' '{"choices":[{"finish_reason":"length","message":{"content":"{"}}]}'"#);
        assert_eq!(call(&c, Path::new("k"), &b, &any, soon()), Err(Failure::Truncated));
        let c = curl(&d, "echo 'curl: (6) Could not resolve host' >&2; exit 6");
        assert!(matches!(call(&c, Path::new("k"), &b, &any, soon()), Err(Failure::Error(e)) if e.contains("Could not resolve host")));
        let refuse = |_: &Value| Err("no summary".to_string());
        let c = curl(&d, r#"printf '%s\n200' '{"choices":[{"finish_reason":"stop","message":{"content":"{\"answer\":{\"summary\":1}}"}}]}'"#);
        assert_eq!(call(&c, Path::new("k"), &b, &refuse, soon()), Err(Failure::Invalid("no summary".into())));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_call_still_running_at_its_deadline_is_ended_and_says_so() {
        let d = dir("slow");
        let c = curl(&d, "sleep 30");
        let start = Instant::now();
        let got = call(&c, Path::new("k"), &body("a", "p", &schema(), "beam-free", None), &any, Instant::now() + Duration::from_millis(500));
        assert!(matches!(&got, Err(Failure::Error(e)) if e.contains("no answer within the plain call")), "{got:?}");
        assert!(start.elapsed() < Duration::from_secs(5), "ended at its deadline, not curl's sleep");
        assert!(matches!(call(&c, Path::new("k"), &json!({}), &any, Instant::now()), Err(Failure::Error(e)) if e.contains("no time was left")));
        let _ = std::fs::remove_dir_all(&d);
    }
}
