//! The verifier's model asked for a plan, and the version control's diff it is asked with.

use std::io::Write;
use std::process::{Command, Stdio};

/// The most of a diff a plan is asked with: past it the model reads the files' list and the start.
pub const DIFF_BYTES: usize = 48 * 1024;

/// The change's diff: `git show` of a commit, `git diff` of a range `A..B`.
pub fn diff(repo: &std::path::Path, change: &str) -> Result<String, String> {
    let mut git = Command::new("git");
    git.arg("-C").arg(repo);
    if change.contains("..") { git.args(["diff", "--stat", "--patch", change]) } else { git.args(["show", "--stat", "--patch", "--format=%H%n%s%n%n%b", change]) };
    let out = git.output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git on {change}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let mut d = String::from_utf8_lossy(&out.stdout).into_owned();
    if d.len() > DIFF_BYTES {
        let cut = (0..=DIFF_BYTES).rev().find(|&i| d.is_char_boundary(i)).unwrap_or(0);
        d.truncate(cut);
        d.push_str("\n[the diff goes on past what a plan is asked with]\n");
    }
    Ok(d)
}

/// What the model is asked first: which paths through the window it would look along before it writes its test.
pub fn explore(intent: &str, diff: &str, first: &str) -> String {
    format!(
        "You will write an end-to-end test of the change below on its built window. First you may see the window after a few steps of a person's.\n\
         Intent, in the author's words: {intent}\n\
         A step is one of: `tap TEXT` (press what shows TEXT), `tap @KEY`, `type TEXT`, `key NAME` (Enter, Escape), `wait SECONDS`, `look`.\n\
         Answer with one fenced code block (```), a line for each path from the start, its steps separated by ` ; `;\n\
         or the block holding the word none. Each step is a look on a fresh window, paid from the run's budget.\n\n\
         What the built window shows at its start, one text a line:\n{first}\n\n\
         The diff:\n{diff}"
    )
}

/// The paths in the model's ANSWER to `explore`: each line of its block, its steps split on ` ; `.
pub fn paths(answer: &str) -> Vec<Vec<String>> {
    let body = answer.split_once("```").and_then(|(_, rest)| rest.split_once('\n')).and_then(|(_, b)| b.split_once("```")).map_or(answer, |(b, _)| b);
    let lines = body.lines().map(str::trim).filter(|l| !l.is_empty() && *l != "none");
    lines.map(|l| l.split(" ; ").map(|s| s.trim().to_string()).collect()).collect()
}

/// What the model is asked: the intent, the diff, why earlier plans were too weak, what its paths showed, and the
/// form a plan takes.
pub fn ask(change: &str, intent: &str, diff: &str, weak: &[String], first: &str, explored: &str) -> String {
    let weak = if weak.is_empty() { "none yet".to_string() } else { weak.iter().map(|w| format!("- {w}")).collect::<Vec<_>>().join("\n") };
    format!(
        "You are an independent verifier. Write an end-to-end test that could show the change below does NOT do what its author says.\n\
         Intent, in the author's words: {intent}\n\
         Why earlier plans were too weak:\n{weak}\n\n\
         The test is a Docket block run against the built window, live on a scratch project with no agent and no thread:\n\
         - `seen <- perform Interact(\"{change}\", \"STEP\");` does one step and gives the screen's texts after it, one a line.\n\
           STEP is one of: `tap TEXT` (press what shows TEXT), `tap @KEY`, `type TEXT`, `key NAME` (Enter, Escape), `wait SECONDS`, `look`.\n\
         - `ok <- perform Expect(\"{change}\", seen, \"WANTED\");` is true when each line of WANTED is in seen and no `!TEXT` line is;\n\
           a WANTED with no line that must be shown never holds, as a blank screen would meet it.\n\
         - Delivered attests nothing unless some line that must be shown was not on the screen before the first step:\n\
           a text the window shows at its start, below, proves nothing the change did.\n\
         - The block ends `if ok {{ return Delivered }} else {{ return Falsified(\"what was wrong\") }}`; chain several checks with nested ifs.\n\
         Answer with the block alone in one fenced code block (```), its first line `{{` and its last `}}`.\n\n\
         What the built window shows at its start, one text a line:\n{first}\n\n\
         What it showed at the end of each path you looked along:\n{explored}\n\n\
         The diff:\n{diff}"
    )
}

/// The block in the model's ANSWER: its first fenced code block, or the whole answer when it has none; a block
/// that does not open with `{` and close with `}` is none.
pub fn block(answer: &str) -> Option<String> {
    let fenced = answer.split_once("```").and_then(|(_, rest)| rest.split_once('\n')).and_then(|(_, body)| body.split_once("```")).map(|(b, _)| b);
    let b = fenced.unwrap_or(answer).trim();
    (b.starts_with('{') && b.ends_with('}')).then(|| b.to_string())
}

/// CMD (the model's command line) run with PROMPT on its stdin; when it exits 0, the answer and the tokens it cost, as
/// `coordd call` prints them: `{"answer": TEXT, "tokens": N}`. An answer with no count is refused, never free.
pub fn run(cmd: &[String], prompt: &str) -> Result<(String, i64), String> {
    let (exe, args) = cmd.split_first().ok_or("no planner command")?;
    let mut child = Command::new(exe).args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("{exe}: {e}"))?;
    child.stdin.take().ok_or("no stdin")?.write_all(prompt.as_bytes()).map_err(|e| format!("{exe}: {e}"))?;
    let out = child.wait_with_output().map_err(|e| format!("{exe}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{exe} exited {}: {}", out.status, String::from_utf8_lossy(&out.stderr).trim()));
    }
    let said = String::from_utf8_lossy(&out.stdout);
    let j: serde_json::Value = serde_json::from_str(said.trim()).map_err(|e| format!("{exe} answered no {{\"answer\", \"tokens\"}}: {e}"))?;
    match (j["answer"].as_str(), j["tokens"].as_i64()) {
        (Some(a), Some(t)) => Ok((a.to_string(), t)),
        _ => Err(format!("{exe} answered no text with a count of its tokens: {}", said.chars().take(300).collect::<String>())),
    }
}
