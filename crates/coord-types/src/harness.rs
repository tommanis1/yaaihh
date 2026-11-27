//! What each harness lets the owner's switches do (walk items 2, 3, 17 and 30): one table per harness, in a box and
//! outside one, that coordd sends the window (`Request::HarnessCapabilities`) so a switch a harness cannot take is
//! shown as such and never silently ignored. Each row was read in the harness's own program text, at the version it
//! names; the rows marked `wire` were also seen changing the request the harness sends, against a stand-in model API
//! on loopback (Claude Code: agent-base's tests/harness-wire-test.sh; Codex and opencode: captures of their requests made
//! the same way). How a start gives the switches is
//! agent-base's bin/harness-settings and roster; coordd's tests hold this table and that program to the same answers.
//!
//! The models an agent may run travel with what people guess of a model whose maker is not named
//! (`Request::ModelList`; walk item 29): one file at the flavor's root, `model-guesses.toml`, for every project.

use serde::{Deserialize, Serialize};

/// The harnesses agents run in, as roster and the switches' tables name them.
pub const HARNESSES: [&str; 4] = ["claude", "codex", "opencode", "agy"];

/// What an agy model's name begins with, for an agent (`agy/gemini-…`: roster's `agy/*`, [`kind_of_model`]).
/// t3code and agy name the model without it.
pub const AGY_MODEL_PREFIX: &str = "agy/";

/// The harness a roster agent on MODEL runs in, as roster chooses it: `gpt-*` Codex, an agy model
/// ([`AGY_MODEL_PREFIX`]) agy, an opencode model id (`PROVIDER/MODEL`, as opencode names its models: no Claude or Codex
/// model name holds a `/`) opencode, any other Claude Code. coordd's and the window's one rule.
pub fn kind_of_model(model: &str) -> &'static str {
    if model.starts_with("gpt-") {
        "codex"
    } else if model.starts_with(AGY_MODEL_PREFIX) {
        "agy"
    } else if model.contains('/') {
        "opencode"
    } else {
        "claude"
    }
}

/// Claude Code's effort levels (`claude --help`, --effort; 2.1.323).
pub const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

/// The effort levels a KIND of agent takes where they are fixed: Claude Code's, and agy 1.5.18's (`agy --help`,
/// --effort; its `thought_level`, which a model offers agy checks at the turn); none for Codex, whose levels are its
/// model's in its catalog.
pub fn efforts(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "claude" => Some(CLAUDE_EFFORTS),
        "agy" => Some(&["low", "medium", "high", "xhigh", "max"]),
        _ => None,
    }
}

/// One thing a harness may be asked to do with the owner's switches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum Capability {
    /// It does it, and HOW. WIRE: seen changing the request the harness sends, not only read in its text.
    Supported { how: String, wire: bool },
    /// It does part of it: HOW, and what it does not do (MISSING).
    Partial { how: String, missing: String, wire: bool },
    /// It cannot, and WHY; a switch set for it is said not taken at the agent's start.
    Unsupported { why: String },
}

impl Capability {
    fn yes(how: &str, wire: bool) -> Self {
        Capability::Supported { how: how.to_string(), wire }
    }
    fn part(how: &str, missing: &str, wire: bool) -> Self {
        Capability::Partial { how: how.to_string(), missing: missing.to_string(), wire }
    }
    fn no(why: &str) -> Self {
        Capability::Unsupported { why: why.to_string() }
    }
    /// Whether a switch set for it is taken at all, in whole or in part.
    pub fn taken(&self) -> bool {
        !matches!(self, Capability::Unsupported { .. })
    }
}

/// What HARNESS lets the switches do, in a box (BOXED) or outside one; READ_IN, the program and version it was read in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessCapabilities {
    pub harness: String,
    pub boxed: bool,
    pub read_in: String,
    /// Its system prompt given as a file, so no size of one argument bounds it.
    pub prompt_file: Capability,
    /// A text of his after the harness's own system prompt.
    pub prompt_append: Capability,
    /// A text of his in place of the harness's own system prompt.
    pub prompt_replace: Capability,
    /// A part of the harness's own system prompt left out (`parts_off`).
    pub parts_off: Capability,
    /// A tool the agent is not given (`tools_off`).
    pub tools_off: Capability,
    /// The harness's own safety check lifted (`safety_check = false`).
    pub safety_check_off: Capability,
}

impl HarnessCapabilities {
    /// HARNESS's table (one of [`HARNESSES`]) in a box or outside one; None for a harness it does not know.
    pub fn of(harness: &str, boxed: bool) -> Option<Self> {
        let c = |read_in: &str, f: [Capability; 6]| {
            let [prompt_file, prompt_append, prompt_replace, parts_off, tools_off, safety_check_off] = f;
            HarnessCapabilities { harness: harness.to_string(), boxed, read_in: read_in.to_string(), prompt_file, prompt_append, prompt_replace, parts_off, tools_off, safety_check_off }
        };
        let no_parts = |who: &str| Capability::no(&format!("{who}'s own prompt has no parts to leave out"));
        let no_check = |who: &str| Capability::no(&format!("{who} has no check of its own that our settings lift"));
        Some(match harness {
            "claude" => c("Claude Code 2.1.332", [
                Capability::yes("--system-prompt-file and --append-system-prompt-file: roster writes the text in the agent's folder, or its box's, at each start", true),
                Capability::yes("--append-system-prompt-file: the standing text, or his version of it", true),
                if boxed {
                    Capability::yes("--system-prompt-file: roster writes his version into the box at each start (the flag seen on the wire outside a box only)", false)
                } else {
                    Capability::yes("--system-prompt-file: his version in place of Claude Code's main text (its identity line changes with it)", true)
                },
                Capability::yes("CLAUDE_CODE_REMOVE_PROMPT_STRINGS: each part's exact text taken out of Claude Code's own prompt, and nothing else", true),
                Capability::yes("--disallowedTools: each tool taken out of the request's tools", true),
                if boxed {
                    Capability::yes("CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS: its check on dangerous rm commands lifted under bypass permissions; off by default in a box", true)
                } else {
                    Capability::Partial {
                        how: "CLAUDE_CODE_EVEN_MORE_DANGEROUSLY_SKIP_PERMISSIONS, for an agent in Yolo (bypass permissions)".into(),
                        missing: "in his own permission mode on the host Claude Code asks him in the terminal instead".into(),
                        wire: true,
                    }
                },
            ]),
            "codex" => c("Codex 0.160.0", [
                Capability::part(
                    "-c model_instructions_file=FILE: his version in place of Codex's own, from the agent's file",
                    "an append goes as one argument (developer_instructions), so at most 128 KiB",
                    true,
                ),
                Capability::yes("-c developer_instructions: the standing text or his version of it, from the agent's file", true),
                Capability::yes("-c model_instructions_file=FILE: Codex's whole instructions replaced", true),
                no_parts("Codex"),
                Capability::part(
                    "-c web_search=\"disabled\" takes out web_search; features.goals=false takes out get_goal, create_goal and update_goal together; features.multi_agent=false takes out multi_agent_v1",
                    "exec_command, write_stdin, request_user_input and view_image have no switch",
                    true,
                ),
                no_check("Codex"),
            ]),
            "opencode" if boxed => {
                let not = || Capability::no("an opencode agent in a box is not started yet");
                c("opencode 1.18.34", [not(), not(), not(), not(), not(), not()])
            }
            "opencode" => c("opencode 1.18.34", [
                Capability::yes("instructions and agent.build.prompt name the agent's file ({file:FILE}), in its config", true),
                Capability::yes("instructions: [FILE], after its own prompt", true),
                Capability::yes("agent.build.prompt = \"{file:FILE}\" with no instructions: its own prompt replaced", true),
                no_parts("opencode"),
                Capability::yes("its permission rules: each tool denied by name takes it out of the request (its old `tools` switch does not)", true),
                no_check("opencode"),
            ]),
            "agy" => c("agy 1.5.18 (ACP)", [
                Capability::yes("AGY_STANDING=FILE: agy-rpc reads the agent's file and sends it", false),
                Capability::yes("ACP's _meta.antigravity.systemPrompt.append, or the head of the thread's first message (through t3code, neon gets none of ours: t3code sets the append itself)", false),
                Capability::no("agy 1.5.18 takes an append only; a replace version is refused"),
                no_parts("agy"),
                Capability::no("agy has no switch for its own tools over ACP"),
                no_check("agy"),
            ]),
            _ => return None,
        })
    }

    /// Every harness's table, outside a box and in one.
    pub fn all() -> Vec<Self> {
        HARNESSES.iter().flat_map(|h| [false, true].map(|b| Self::of(h, b).expect("a known harness"))).collect()
    }
}

/// One model an agent of the project may run, as `Request::ModelList` gives it: its id, and where its maker is not
/// named, what people guess it is (GUESS) and where that was read (READ), from the flavor's `model-guesses.toml`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guess: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_harness_has_a_table_in_and_out_of_a_box_and_says_why_where_it_cannot() {
        let all = HarnessCapabilities::all();
        assert_eq!(all.len(), HARNESSES.len() * 2);
        for c in &all {
            for cap in [&c.prompt_file, &c.prompt_append, &c.prompt_replace, &c.parts_off, &c.tools_off, &c.safety_check_off] {
                match cap {
                    Capability::Supported { how, .. } | Capability::Partial { how, .. } => assert!(!how.is_empty(), "{} says how", c.harness),
                    Capability::Unsupported { why } => assert!(!why.is_empty(), "{} says why not", c.harness),
                }
            }
        }
        assert!(HarnessCapabilities::of("kiro", false).is_none());
    }

    #[test]
    fn what_was_seen_on_the_wire_and_claude_codes_check_whole_only_in_a_box() {
        let out = HarnessCapabilities::of("claude", false).unwrap();
        let boxed = HarnessCapabilities::of("claude", true).unwrap();
        assert!(matches!(out.safety_check_off, Capability::Partial { .. }));
        assert!(matches!(boxed.safety_check_off, Capability::Supported { wire: true, .. }));
        assert!(matches!(out.parts_off, Capability::Supported { wire: true, .. }));
        assert!(matches!(out.tools_off, Capability::Supported { wire: true, .. }));
        let seen = |c: &Capability| matches!(c, Capability::Supported { wire: true, .. } | Capability::Partial { wire: true, .. });
        for h in ["codex", "opencode", "agy"] {
            let c = HarnessCapabilities::of(h, false).unwrap();
            assert!(!c.parts_off.taken(), "{h} takes no part off");
            assert_eq!(seen(&c.tools_off), h != "agy", "{h}: tools off seen on the wire where it is taken");
            assert_eq!(seen(&c.prompt_replace), h != "agy", "{h}: a replace seen on the wire where it is taken");
        }
        assert!(matches!(HarnessCapabilities::of("codex", false).unwrap().tools_off, Capability::Partial { .. }), "Codex's shell tools have no switch");
        assert!(!HarnessCapabilities::of("agy", false).unwrap().prompt_replace.taken());
    }

    #[test]
    fn a_table_and_a_model_entry_go_over_the_wire_and_back() {
        let all = HarnessCapabilities::all();
        let back: Vec<HarnessCapabilities> = serde_json::from_str(&serde_json::to_string(&all).unwrap()).unwrap();
        assert_eq!(back, all);
        let m = ModelEntry { id: "opencode-go/beam-free".into(), guess: Some("Beam 1.0".into()), read: Some("a page".into()) };
        assert_eq!(serde_json::from_str::<ModelEntry>(&serde_json::to_string(&m).unwrap()).unwrap(), m);
        assert_eq!(serde_json::to_string(&ModelEntry { id: "claude-haiku-5-5".into(), ..Default::default() }).unwrap(), r#"{"id":"claude-haiku-5-5"}"#);
    }
}
