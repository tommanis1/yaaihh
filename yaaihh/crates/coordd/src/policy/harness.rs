//! Harness sessions: a net's
//! session is a real agent harness session, Codex or Claude Code, started on its first call and resumed by its id
//! on every later one, so the harness keeps the context (not a chat history coordd replays). The harness is the
//! model's (`harness(model, H)`, a system fact from the model's provider); both talk to the models through the
//! proxy with the coord folder's proxy key, with their own home under `$COORD/nets/` (no agent's hooks or settings
//! run), no tools, and the call's return type as the answer's JSON schema.

use super::call::Failure;
use super::facts::Proxy;
use serde_json::Value;
use std::process::{Command, Stdio};

/// One turn of a harness session: PROMPT to MODEL on HARNESS, in session RESUME (none: a new session), the answer
/// held to SCHEMA. Its answer (the JSON the harness returned) and the session's id.
pub fn turn(harness: &str, coord: &str, proxy: &Proxy, model: &str, prompt: &str, schema: &Value, resume: Option<&str>) -> Result<(Value, String), Failure> {
    let dir = format!("{coord}/nets/{harness}");
    let work = format!("{dir}/work");
    std::fs::create_dir_all(&work).map_err(|e| Failure::Error(format!("{work}: {e}")))?;
    let out = match harness {
        "codex" => codex(&dir, &work, proxy, model, prompt, schema, resume, coord)?,
        "claude" => claude(&dir, &work, proxy, model, prompt, schema, resume)?,
        other => return Err(Failure::Error(format!("no harness `{other}` (codex, claude)"))),
    };
    Ok(out)
}

fn ran(mut cmd: Command, what: &str) -> Result<String, Failure> {
    let o = cmd.stdin(Stdio::null()).output().map_err(|e| Failure::Error(format!("{what}: {e}")))?;
    if !o.status.success() {
        let err = String::from_utf8_lossy(&o.stderr);
        let tail: String = err.lines().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        return Err(Failure::Error(format!("{what} exited {}: {tail}", o.status)));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// `codex exec [resume ID] --json --output-schema`: the thread id from its `thread.started` event (a resumed one
/// keeps RESUME), the answer from its last message. The box's Codex is `$COORD/opt/codex` (bin/codex), the host's the
/// one on PATH.
#[allow(clippy::too_many_arguments)]
fn codex(dir: &str, work: &str, proxy: &Proxy, model: &str, prompt: &str, schema: &Value, resume: Option<&str>, coord: &str) -> Result<(Value, String), Failure> {
    let home = format!("{dir}/home");
    std::fs::create_dir_all(&home).map_err(|e| Failure::Error(format!("{home}: {e}")))?;
    // files of this turn only (turns of different agents' sessions run at the same time)
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (schema_file, last) = (format!("{dir}/schema-{}-{n}.json", std::process::id()), format!("{dir}/last-{}-{n}.txt", std::process::id()));
    std::fs::write(&schema_file, schema.to_string()).map_err(|e| Failure::Error(e.to_string()))?;
    let boxed = format!("{coord}/opt/codex/bin/codex");
    let bin = if std::path::Path::new(&boxed).exists() { boxed } else { "codex".into() };
    let mut cmd = Command::new(bin);
    cmd.arg("exec");
    if let Some(id) = resume {
        cmd.args(["resume", id]);
    }
    let provider = format!("model_providers.nets={{name=\"proxy\",base_url=\"{}/v1\",wire_api=\"responses\",env_key=\"CPA_API_KEY\"}}", proxy.url);
    cmd.args(["--json", "--skip-git-repo-check", "-c", "model_provider=\"nets\"", "-c", &provider, "-c", "sandbox_mode=\"read-only\"", "-m", model])
        .args(["--output-schema", &schema_file, "-o", &last])
        .arg(prompt)
        .current_dir(work)
        .env("CODEX_HOME", &home)
        .env("CPA_API_KEY", &proxy.key);
    let stdout = ran(cmd, "codex exec");
    let answer = std::fs::read_to_string(&last);
    let _ = (std::fs::remove_file(&last), std::fs::remove_file(&schema_file));
    let stdout = stdout?;
    let id = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|e| (e["type"] == "thread.started").then(|| e["thread_id"].as_str().map(String::from)).flatten())
        .or(resume.map(String::from))
        .ok_or(Failure::Invalid("codex exec gave no thread id".into()))?;
    let text = answer.map_err(|e| Failure::Invalid(format!("codex exec gave no answer: {e}")))?;
    let v = serde_json::from_str(text.trim()).map_err(|e| Failure::Invalid(format!("not JSON: {e}")))?;
    Ok((v, id))
}

/// `claude -p [--resume ID] --output-format json --json-schema`, no tools and no settings: the session id and the
/// structured output from its result.
fn claude(dir: &str, work: &str, proxy: &Proxy, model: &str, prompt: &str, schema: &Value, resume: Option<&str>) -> Result<(Value, String), Failure> {
    let mut cmd = Command::new("claude");
    cmd.args(["-p", prompt, "--model", model, "--output-format", "json", "--tools", "", "--setting-sources", ""]).args(["--json-schema", &schema.to_string()]);
    if let Some(id) = resume {
        cmd.args(["--resume", id]);
    }
    cmd.current_dir(work).env("CLAUDE_CONFIG_DIR", format!("{dir}/home")).env("ANTHROPIC_BASE_URL", &proxy.url).env("ANTHROPIC_API_KEY", &proxy.key).env_remove("ANTHROPIC_AUTH_TOKEN");
    let out: Value = serde_json::from_str(&ran(cmd, "claude -p")?).map_err(|e| Failure::Invalid(format!("claude -p: not JSON: {e}")))?;
    if out["is_error"] == true {
        return Err(Failure::Error(format!("claude -p: {}", out["result"].as_str().unwrap_or("an error"))));
    }
    let id = out["session_id"].as_str().ok_or(Failure::Invalid("claude -p gave no session id".into()))?.to_string();
    let v = out.get("structured_output").filter(|v| !v.is_null()).cloned().ok_or(Failure::Invalid("claude -p gave no structured output".into()))?;
    Ok((v, id))
}
