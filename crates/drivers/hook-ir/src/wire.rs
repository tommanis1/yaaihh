//! The two lines between the relay and a driver: the hook's native name and input in, the harness's answer out.

use serde_json::{json, Value as Json};

/// The relay's line to the driver: which native hook ran, and its input as the harness gave it, whole.
pub fn ask(hook: &str, input: &str) -> String {
    json!({"hook": hook, "input": input}).to_string()
}

/// The driver's reading of `ask`'s line: the native hook's name and its input.
pub fn read_ask(line: &str) -> Result<(String, String), String> {
    let v: Json = serde_json::from_str(line).map_err(|e| format!("the relay's line is not JSON: {e}"))?;
    match (v["hook"].as_str(), v["input"].as_str()) {
        (Some(h), Some(i)) => Ok((h.to_string(), i.to_string())),
        _ => Err("the relay's line has no hook or no input".into()),
    }
}

/// The driver's line back: what the relay prints for the harness, and the exit code it ends with.
pub fn answer(stdout: &str, code: i32) -> String {
    json!({"stdout": stdout, "code": code}).to_string()
}

/// The relay's reading of `answer`'s line.
pub fn read_answer(line: &str) -> Result<(String, i32), String> {
    let v: Json = serde_json::from_str(line).map_err(|e| format!("the driver's line is not JSON: {e}"))?;
    match (v["stdout"].as_str(), v["code"].as_i64()) {
        (Some(s), Some(c)) => Ok((s.to_string(), c as i32)),
        _ => Err("the driver's line has no stdout or no code".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_reads_back_whole_whatever_the_input_holds() {
        let input = "{\"tool_name\":\"Bash\",\"x\":\"a\\nb \u{e9}\"}\n";
        assert_eq!(read_ask(&ask("PreToolUse", input)).unwrap(), ("PreToolUse".into(), input.into()));
        assert!(!ask("PreToolUse", input).contains('\n'), "one line");
        assert_eq!(read_answer(&answer("{}", 0)).unwrap(), ("{}".into(), 0));
        assert!(read_ask("{\"hook\":1}").is_err() && read_answer("no").is_err());
    }
}
