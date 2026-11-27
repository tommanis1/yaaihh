//! `coordd call COORD BASE MODEL`, run on the host and never in a box: by clerk-model, the child of coordd-v2's clerk
//! driver under cutover's go unit. The prompt on stdin goes once to MODEL in the one pool (the owner's Q-1908), placed
//! as coordd's own calls are ([`Router::counted`]), and `{"answer", "tokens"}` is printed.

use crate::accounts::router::Router;
use crate::policy::Places;
use serde_json::{json, Value};
use std::io::Read;

/// The call's name, as the gateway's schema names it.
const NAME: &str = "reader";

/// The verb: 0 with the answer on stdout, else 1 with why on stderr.
pub fn run(coord: &str, base: &str, model: &str) -> i32 {
    let mut prompt = String::new();
    let read = std::io::stdin().read_to_string(&mut prompt).map_err(|e| format!("the prompt: {e}"));
    let places = Places { base: base.into(), coord: coord.into(), ..Default::default() };
    match read.and_then(|_| answer(&Router::of(&places), &prompt, model)) {
        Ok(line) => {
            println!("{line}");
            0
        }
        Err(e) => {
            eprintln!("coordd call: {e}");
            1
        }
    }
}

/// PROMPT once to MODEL by ROUTER, its answer a text: `{"answer", "tokens"}`. A place that gave no count fails it,
/// never a call costing nothing.
fn answer(router: &Router, prompt: &str, model: &str) -> Result<String, String> {
    let text = |a: &Value| if a.is_string() { Ok(()) } else { Err(format!("{a} is not a text")) };
    let (got, place) = router.counted(NAME, prompt, &json!({"type": "string"}), model, &text);
    let on = place.map_or_else(|| "the gateway".to_string(), |p| format!("account {p}"));
    match got {
        Ok((a, Some(tokens))) => Ok(json!({"answer": a, "tokens": tokens}).to_string()),
        Ok((_, None)) => Err(format!("{model} on {on} answered with no count of its tokens")),
        Err(f) => Err(format!("{model}: {f:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::{router::TEST_ACCOUNTS, testing::Tmp, Harness, Kind};

    /// A made-up Claude Code: the prompt kept beside it, one answer of the schema on 26 tokens. No model is called.
    const CLAUDE: &str = r#"#!/bin/sh
cat > "$(dirname "$0")/prompt"
echo '{"type":"result","subtype":"success","is_error":false,"structured_output":{"answer":"a text"},"usage":{"input_tokens":3,"cache_creation_input_tokens":5,"cache_read_input_tokens":7,"output_tokens":11}}'
"#;

    /// The gateway's models in these tests: none is Claude Code's, so each goes to the gateway.
    const CODEX: &[(&str, &str)] = &[("gpt-6.2-sol", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6-luna", "openai")];

    /// The worktree's agent-base, and a coord in T.
    fn places(t: &Tmp) -> Places {
        let coord = t.dir.join("coord");
        std::fs::create_dir_all(&coord).unwrap();
        Places { base: format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR")), coord: coord.display().to_string(), ..Default::default() }
    }

    /// A Claude model goes to one of his signed-in accounts, as coordd's own calls do, and its tokens are what Claude
    /// Code's result counts: input, both caches and output.
    #[test]
    fn a_claude_models_call_runs_on_his_account_and_counts_its_tokens() {
        let t = Tmp::new("one-call-account");
        t.store.create(Harness::Claude, "a", "a", 10, Kind::Signin, "x").unwrap();
        t.store.update(Harness::Claude, "a", |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
        let program = t.program("claude", CLAUDE);
        TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), program)));
        let router = Router::of(&places(&t));
        assert_eq!(answer(&router, "the prompt", "claude-sonnet-5-5"), Ok(json!({"answer": "a text", "tokens": 26}).to_string()));
        assert_eq!(std::fs::read_to_string(t.dir.join("prompt")).unwrap(), "the prompt", "the prompt on its input");
        assert!(t.store.get(Harness::Claude, "a").unwrap().bound.is_empty(), "a call binds nothing");
    }

    /// A model no account serves goes to the gateway: its usage is the count. One that gives none, or an answer that
    /// is not a text, is a failure.
    #[test]
    fn a_gateway_call_counts_its_usage_and_one_without_a_count_or_a_text_fails() {
        let t = Tmp::new("one-call-gateway");
        let asked = crate::clean::tests::stand_in_serving(CODEX, |q| {
            let said = |a: Value| json!([{"finish_reason": "stop", "message": {"content": json!({"answer": a}).to_string()}}]);
            match q["model"].as_str() {
                Some("gpt-6.2-sol") => ("200 OK", json!({"choices": said(json!("a text")), "usage": {"total_tokens": 40}})),
                Some("gpt-6.1-sol") => ("200 OK", json!({"choices": said(json!("a text"))})),
                _ => ("200 OK", json!({"choices": said(json!(5)), "usage": {"total_tokens": 9}})),
            }
        });
        let router = Router::of(&places(&t));
        assert_eq!(answer(&router, "the prompt", "gpt-6.2-sol"), Ok(json!({"answer": "a text", "tokens": 40}).to_string()));
        assert_eq!(asked.lock().unwrap()[0]["messages"][0]["content"], "the prompt");
        assert_eq!(answer(&router, "p", "gpt-6.1-sol"), Err("gpt-6.1-sol on the gateway answered with no count of its tokens".into()));
        assert_eq!(answer(&router, "p", "gpt-6-luna"), Err("gpt-6-luna: Invalid(\"5 is not a text\")".into()));
    }
}
