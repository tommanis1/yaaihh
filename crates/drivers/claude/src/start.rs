//! How a Claude Code session's start takes the switches: its system text from a file, tools and parts of its own
//! prompt left out, and its check on dangerous commands, each as Claude Code 2.1.332 was seen to take it on the wire.

use docket_hooks::start::{Launch, Prompt, Switches};

/// The start's arguments and environment for `s`, with the system text in file `prompt` given as `how`. A resumed
/// session keeps the text of its first request unless the snapshot is off, so it is turned off with either.
pub fn launch(s: &Switches, prompt: Option<(&str, Prompt)>) -> Launch {
    let mut l = Launch::default();
    if let Some((file, how)) = prompt {
        let flag = if how == Prompt::Replace { "--system-prompt-file" } else { "--append-system-prompt-file" };
        l.args.extend([flag, file, "--system-prompt-snapshot", "off"].map(String::from));
    }
    if !s.tools_off.is_empty() {
        l.args.extend(["--disallowedTools".to_string(), s.tools_off.join(",")]);
    }
    if !s.parts_off.is_empty() {
        l.env.push(("CLAUDE_CODE_REMOVE_PROMPT_STRINGS".into(), serde_json::to_string(&s.parts_off).unwrap()));
    }
    if s.unasked {
        l.args.push("--dangerously-skip-permissions".into());
    }
    // its own check, lifted only under bypass permissions
    if !s.safety_check {
        l.env.push(("CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS".into(), "1".into()));
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_start_takes_each_switch_as_seen_on_the_wire() {
        assert_eq!(launch(&Switches::default(), None), Launch::default(), "its check kept on");
        let l = launch(&Switches { safety_check: false, tools_off: vec!["WebFetch".into(), "Task".into()], parts_off: vec!["a \"b\"".into()], ..Default::default() }, Some(("/a/p.md", Prompt::Append)));
        assert_eq!(l.args, ["--append-system-prompt-file", "/a/p.md", "--system-prompt-snapshot", "off", "--disallowedTools", "WebFetch,Task"]);
        assert_eq!(l.env, [("CLAUDE_CODE_REMOVE_PROMPT_STRINGS".to_string(), r#"["a \"b\""]"#.to_string()), ("CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS".to_string(), "1".to_string())]);
        assert_eq!(launch(&Switches::default(), Some(("/a/p.md", Prompt::Replace))).args[0], "--system-prompt-file");
        let guarded = launch(&Switches { unasked: true, ..Default::default() }, None);
        assert_eq!((guarded.args, guarded.env), (vec!["--dangerously-skip-permissions".to_string()], vec![]), "no asking, its check kept");
    }

    /// The three choices' starts against agent-base/tests/harness-wire-test.sh, whose safety-on case is the real
    /// Claude Code refusing `rm -rf ~` under `--dangerously-skip-permissions` with no other switch: Guarded starts it
    /// exactly so, Yolo with only the check's lifting added, Safe with neither.
    #[test]
    fn guarded_starts_as_the_wire_tests_safety_on_case_and_yolo_adds_only_the_lifted_check() {
        let start = |unasked, safety_check| launch(&Switches { unasked, safety_check, ..Default::default() }, None);
        let safety_on = (vec!["--dangerously-skip-permissions".to_string()], vec![]);
        let lifted = ("CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS".to_string(), "1".to_string());
        let guarded = start(true, true);
        assert_eq!((guarded.args, guarded.env), safety_on, "Guarded");
        let yolo = start(true, false);
        assert_eq!((yolo.args, yolo.env), (safety_on.0.clone(), vec![lifted]), "Yolo");
        assert_eq!(start(false, true), Launch::default(), "Safe");
    }
}
