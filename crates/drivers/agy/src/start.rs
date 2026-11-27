//! How an agy session's start takes the switches, through t3code or over ACP: its system text appended only.
//!
//! agy 1.5.18 takes a system text only appended, through ACP's `_meta.antigravity.systemPrompt.append`, which
//! agy-rpc sets from the file AGY_STANDING names. It has no replace, and no tool switch over ACP.

use docket_hooks::start::{Launch, Prompt, Switches};

/// The start's environment for `s`, with the system text in file `prompt`; a replace is refused before anything
/// starts.
pub fn launch(s: &Switches, prompt: Option<(&str, Prompt)>) -> Result<Launch, String> {
    let mut l = Launch::default();
    match prompt {
        Some((_, Prompt::Replace)) => return Err("agy takes a system text only appended, after its own; it has no replace".into()),
        Some((file, Prompt::Append)) => l.env.push(("AGY_STANDING".into(), file.into())),
        None => {}
    }
    l.tools_on.extend(s.tools_off.iter().map(|t| format!("tool {t}: agy takes no tool switch")));
    l.not_taken.extend(s.parts_off.iter().map(|p| format!("part {:?}: agy's own prompt has no parts to leave out", p.chars().take(40).collect::<String>())));
    if s.safety_check {
        l.not_taken.push("safety_check: agy has no check of its own to keep on".into());
    }
    // with the check off the program answers its asks (std.hooks 6); with it on, nothing here runs it unasked
    if s.unasked && s.safety_check {
        l.not_taken.push("unasked: agy has no switch to run commands unasked, so it asks as before".into());
    }
    Ok(l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_start_appends_only_and_says_each_switch_it_cannot_take() {
        assert_eq!(launch(&Switches { safety_check: false, ..Default::default() }, None).unwrap(), Default::default());
        assert!(launch(&Switches::default(), Some(("/a/p.md", Prompt::Replace))).is_err());
        let l = launch(&Switches { tools_off: vec!["web_search".into()], parts_off: vec!["x".into()], safety_check: true, unasked: false }, Some(("/a/p.md", Prompt::Append))).unwrap();
        assert_eq!(l.env, vec![("AGY_STANDING".to_string(), "/a/p.md".to_string())]);
        assert_eq!((l.tools_on.len(), l.not_taken.len()), (1, 2), "{:?}", l);
        assert!(launch(&Switches { safety_check: false, ..Default::default() }, None).unwrap().not_taken.is_empty());
    }
}
