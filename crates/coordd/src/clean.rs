//! The owner's typing cleaned by fresh calls, one a paragraph, at once (`Request::CleanText`, protocol 7; the owner N-481 p10, p11; N-487):
//! his text and a fixed instruction, no ticket or other context, the model the policy chooses for the task `clean`
//! (`policy::Engine::call_prompt`, the clerk's own way to a model). It fixes misspellings and words run together or
//! split, and nothing else; ids, hashes, paths, names and code are checked to come back letter for letter, and an
//! answer that changed one is refused, never handed on. Nothing is posted, written or recorded: what the window does
//! with the answer is the window's.

use serde_json::{json, Value};

/// The longest paragraph one call cleans: its answer has to fit the call's `max_tokens` (call::request_body, 2000).
pub const MAX: usize = 6000;

/// The task and the call's name, as the policy and the proxy's request see them.
pub const TASK: &str = "clean";

/// The fixed instruction with TEXT in it: everything the model is given.
pub fn prompt(text: &str) -> String {
    format!(
        "Fix the typing errors in the text below (a paragraph of a message typed fast), and nothing else.\n\
         \n\
         - Correct misspelled words, and words run together or split apart by a missed or extra space (\"woth\" becomes \"worth\", \"nolonger\" becomes \"no longer\").\n\
         - Keep every other word, the word order, the punctuation, the line breaks, the capitals and the tone exactly as they are. Do not reword, shorten, expand, translate, answer or add anything. Do not fix grammar or style.\n\
         - Keep exactly as written: ids (N-487, p13, Q-464), commit hashes, file names and paths, @names and other names, commands, code, anything in backticks, quoted lines (starting with >), numbers and URLs.\n\
         \n\
         Answer one of:\n\
         - Sure(cleaned): you are certain of every fix you made. CLEANED is the whole text with them; the text itself when nothing needs fixing.\n\
         - Borderline(cleaned, doubts): CLEANED has only the fixes you are certain of. Each place you are not certain of is a doubt: TYPED, the words exactly as they stand in CLEANED, and WOULD, the one or more things it could be meant as (\"either this or that\"). Do not make those fixes: a doubt is left as typed.\n\
         - Unreadable(why): you cannot tell what a sentence says. WHY is one line saying which.\n\
         \n\
         The text:\n\
         <<<\n\
         {text}\n\
         >>>"
    )
}

/// The answer's shape: Sure(cleaned) | Borderline(cleaned, doubts) | Unreadable(why), `head` first in each.
pub fn schema() -> Value {
    let case = |head: &str, fields: Value| {
        let mut props = serde_json::Map::new();
        props.insert("head".into(), json!({"type": "string", "enum": [head]}));
        let mut required = vec![json!("head")];
        for (k, v) in fields.as_object().unwrap() {
            props.insert(k.clone(), v.clone());
            required.push(json!(k));
        }
        json!({"type": "object", "properties": props, "required": required, "additionalProperties": false})
    };
    let doubt = json!({"type": "object", "properties": {"typed": {"type": "string"}, "would": {"type": "array", "items": {"type": "string"}}}, "required": ["typed", "would"], "additionalProperties": false});
    json!({"anyOf": [
        case("Sure", json!({"cleaned": {"type": "string"}})),
        case("Borderline", json!({"cleaned": {"type": "string"}, "doubts": {"type": "array", "items": doubt}})),
        case("Unreadable", json!({"why": {"type": "string"}})),
    ]})
}

/// One answer of the call held to TEXT: a cleaning keeps what [`check`] keeps, each doubt's TYPED stands in it as
/// typed (a doubt is a fix not made); Unreadable says why.
pub fn check_answer(text: &str, a: &Value, names: &[String]) -> Result<(), String> {
    match a["head"].as_str() {
        Some("Sure") | Some("Borderline") => {
            let cleaned = a["cleaned"].as_str().ok_or("no cleaned text")?;
            check(text, cleaned, names)?;
            for d in a["doubts"].as_array().into_iter().flatten() {
                let typed = d["typed"].as_str().unwrap_or("");
                if typed.trim().is_empty() || !cleaned.contains(typed) {
                    return Err(format!("a doubt's words are not in the cleaned text: {typed}"));
                }
                if d["would"].as_array().is_none_or(|w| w.is_empty() || w.iter().any(|x| x.as_str().is_none_or(|x| x.trim().is_empty()))) {
                    return Err(format!("a doubt names nothing it could be meant as: {typed}"));
                }
            }
            Ok(())
        }
        Some("Unreadable") => a["why"].as_str().filter(|w| !w.trim().is_empty()).map(|_| ()).ok_or_else(|| "Unreadable says no why".to_string()),
        _ => Err("not Sure, Borderline or Unreadable".into()),
    }
}

/// The marks around a word that are not part of it ("(see", "lib.rs,", "\"yes\"").
const AROUND: &[char] = &['"', '\'', '(', ')', '[', ']', '{', '}', '<', '>', ',', '.', ';', ':', '!', '?', '*'];

/// TEXT's words as checked: split at white space, the marks around each taken off; empty ones dropped.
fn words(text: &str) -> Vec<&str> {
    text.split_whitespace().map(|w| w.trim_matches(AROUND)).filter(|w| !w.is_empty()).collect()
}

/// Whether word W must come back letter for letter: it holds anything but letters and an apostrophe (a digit, `-`,
/// `/`, `_`, `.`, `@`, `#`, `:`, a backtick ...), it has a capital after its first letter (PROTOCOL, CleanText), or
/// it is one of NAMES in any case (the coord's agents, the project's [`KEEP`] words, his own: `yaaihh` and `agda` are
/// names, not misspellings).
fn kept(w: &str, names: &[String]) -> bool {
    w.chars().any(|c| !(c.is_alphabetic() || c == '\'' || c == '’')) || w.chars().skip(1).any(char::is_uppercase) || names.iter().any(|n| n.to_lowercase() == w.to_lowercase())
}

/// The file of a project's own words (each folder of `project::folders`: agent-base's, the library's, the project's):
/// one word a line, `//` a comment. They come back letter for letter, as agents' names do, and are never a doubt
/// (paper-research N-487 p28: `Agda`, `Tovrin`, `eFLINT`). What the owner adds himself comes with his words instead.
pub const KEEP: &str = "keep-words";

/// The words that stay as typed for a clean in PLACES: the coord's AGENTS, the project's [`KEEP`] files, and HIS own
/// (the window's list, with his words), each once.
pub fn kept_words(places: &crate::policy::Places, agents: impl IntoIterator<Item = String>, his: &[String]) -> Vec<String> {
    let mut out: Vec<String> = agents.into_iter().collect();
    for d in crate::project::folders(&places.coord, &places.base, places.project.as_deref()) {
        let text = std::fs::read_to_string(format!("{d}/{KEEP}")).unwrap_or_default();
        out.extend(text.lines().map(|l| l.split("//").next().unwrap_or("").trim()).filter(|l| !l.is_empty()).map(String::from));
    }
    out.extend(his.iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()));
    let mut seen = std::collections::BTreeSet::new();
    out.retain(|w| seen.insert(w.to_lowercase()));
    out
}

/// A paragraph's answer A without its doubts on kept words (a name the model did not know, "agda" → "agenda"): those
/// stay as typed either way, so they are no question for him. Borderline with none left is Sure.
pub fn without_kept_doubts(mut a: Value, names: &[String]) -> Value {
    if a["head"] != "Borderline" {
        return a;
    }
    let left: Vec<Value> = a["doubts"].as_array().cloned().unwrap_or_default().into_iter().filter(|d| !words(d["typed"].as_str().unwrap_or("")).iter().all(|w| kept(w, names))).collect();
    if left.is_empty() {
        return json!({"head": "Sure", "cleaned": a["cleaned"]});
    }
    a["doubts"] = json!(left);
    a
}

/// The spans in backticks (`x` and ``` fenced ```), each to come back as it is.
fn quoted(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(a) = rest.find('`') {
        let fence = rest[a..].chars().take_while(|&c| c == '`').count();
        let open = &rest[a..a + fence];
        let after = &rest[a + fence..];
        match after.find(open) {
            Some(b) => {
                out.push(&rest[a..a + fence + b + fence]);
                rest = &after[b + fence..];
            }
            None => break,
        }
    }
    out
}

/// What must not change, in the order the text has them: the backtick spans, then each kept word as often as it comes.
pub fn protected<'a>(text: &'a str, names: &[String]) -> Vec<&'a str> {
    let mut out = quoted(text);
    out.extend(words(text).into_iter().filter(|w| kept(w, names)));
    out
}

/// Whether LINE quotes someone (`> …`): it comes back letter for letter.
fn quote_line(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

/// Refused unless GOT keeps all of TEXT's protected parts (each as often as TEXT has it), its lines (as many, a quoted
/// `> …` line letter for letter where it stands, no blank line added) and about its length (a rewording, an answer to
/// the message or a cut is not a cleaning). TEXT is one paragraph ([`paragraphs`]).
pub fn check(text: &str, got: &str, names: &[String]) -> Result<(), String> {
    let (ours, theirs): (Vec<&str>, Vec<&str>) = (text.trim().lines().collect(), got.trim().lines().collect());
    if ours.len() != theirs.len() {
        return Err(format!("the call's answer has {} lines for {}", theirs.len(), ours.len()));
    }
    for (o, t) in ours.iter().zip(&theirs) {
        if quote_line(o) && o.trim() != t.trim() {
            return Err(format!("the call changed a quoted line: {}", o.trim()));
        }
        if t.trim().is_empty() != o.trim().is_empty() {
            return Err("the call's answer has a paragraph break where the text has none".into());
        }
    }
    for q in quoted(text) {
        if got.matches(q).count() < text.matches(q).count() {
            return Err(format!("the call changed what is in backticks: {q}"));
        }
    }
    let mut have: Vec<&str> = words(got);
    for w in words(text).into_iter().filter(|w| kept(w, names)) {
        match have.iter().position(|h| *h == w) {
            Some(i) => {
                have.remove(i);
            }
            None => return Err(format!("the call changed a code, id or name: `{w}`")),
        }
    }
    let (n, m) = (text.chars().count(), got.chars().count());
    if m * 10 < n * 7 || m * 10 > n * 13 + 200 {
        return Err(format!("the call's answer is not the text cleaned ({m} characters for {n})"));
    }
    Ok(())
}

/// GOT with TEXT's own white space before and after it (a model adds or drops a closing newline).
pub fn framed(text: &str, got: &str) -> String {
    let lead = &text[..text.len() - text.trim_start().len()];
    let trail = &text[text.trim_end().len()..];
    format!("{lead}{}{trail}", got.trim())
}

/// TEXT in pieces that join back to it: each paragraph (`true`: to be cleaned) and the blank lines between them
/// (`false`: kept). A fenced block (```) that has blank lines in it stays one paragraph; so does a run of quoted lines.
pub fn paragraphs(text: &str) -> Vec<(&str, bool)> {
    let mut out: Vec<(&str, bool)> = Vec::new();
    let (mut at, mut start, mut fenced) = (0, 0, false);
    let mut blank_from: Option<usize> = None;
    for line in text.split_inclusive('\n') {
        let blank = line.trim().is_empty();
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if blank && !fenced {
            if blank_from.is_none() {
                if at > start {
                    out.push((&text[start..at], true));
                }
                blank_from = Some(at);
            }
        } else if let Some(b) = blank_from.take() {
            out.push((&text[b..at], false));
            start = at;
        }
        at += line.len();
    }
    match blank_from {
        Some(b) => out.push((&text[b..at], false)),
        None if at > start => out.push((&text[start..at], true)),
        None => {}
    }
    out
}

/// Whether paragraph P needs no call: only quoted lines, or only a fenced block.
fn left_alone(p: &str) -> bool {
    let t = p.trim();
    t.lines().all(quote_line) || (t.starts_with("```") && t.ends_with("```"))
}

/// N doubts in a text of WORDS words, per 100 words, rounded up (one doubt in a long message is still one): what the
/// policy's `how-many` reads (`call.per100`), the same unit as the box's marks (N-481 p45: "depens out of total").
pub fn per100(n: usize, words: usize) -> u64 {
    if n == 0 { 0 } else { (n * 100).div_ceil(words.max(1)) as u64 }
}

/// The answers of the paragraphs called, in order, as one: Unreadable when one is (the first, with which), else
/// Borderline when one is (every doubt), else Sure; CLEANED the whole text, paragraphs not called as he typed them.
pub fn joined(parts: &[(&str, bool)], called: &[(usize, Value)]) -> Value {
    let of = called.len();
    if let Some((k, (_, a))) = called.iter().enumerate().find(|(_, (_, a))| a["head"] == "Unreadable") {
        let why = a["why"].as_str().unwrap_or("").trim();
        return json!({"head": "Unreadable", "why": if of > 1 { format!("paragraph {} of {of}: {why}", k + 1) } else { why.to_string() }});
    }
    let mut out: Vec<String> = parts.iter().map(|(p, _)| p.to_string()).collect();
    let mut doubts = Vec::new();
    for (i, a) in called {
        out[*i] = framed(parts[*i].0, a["cleaned"].as_str().unwrap_or(parts[*i].0));
        doubts.extend(a["doubts"].as_array().cloned().unwrap_or_default());
    }
    let cleaned = out.concat();
    if called.iter().any(|(_, a)| a["head"] == "Borderline") {
        let per100 = per100(doubts.len(), parts.iter().map(|(p, _)| words(p).len()).sum());
        json!({"head": "Borderline", "cleaned": cleaned, "doubts": doubts, "per100": per100})
    } else {
        json!({"head": "Sure", "cleaned": cleaned})
    }
}

/// The reply's `out`: the outcome, with the models and the time beside it.
pub fn reply(mut outcome: Value, models: &[String], ms: u128) -> String {
    outcome["model"] = json!(models.join(", "));
    outcome["ms"] = json!(ms as u64);
    outcome.to_string()
}

/// One paragraph's answer from the model the policy chooses (blocking), and the model.
fn clean_one(engine: &crate::policy::Engine, text: &str, names: &[String]) -> Result<(Value, String), String> {
    let check = |a: &Value| check_answer(text, a, names);
    let c = engine.call_prompt(TASK, TASK, &prompt(text.trim()), &schema(), "owner", &check)?;
    match c.answer {
        Some(a) => Ok((without_kept_doubts(a, names), c.decision.chosen.clone().unwrap_or_default())),
        // why, in the one copy's words (policy `unanswered_words`): a limit with its time, else the fault and the last
        // refusal (what the call changed, when a check refused it)
        None => {
            let refused = c.decision.attempts.last().and_then(|a| a.failure.as_deref()).is_some_and(|f| !failure_words(f).is_empty());
            let why = crate::policy::unanswered_words(&c.decision);
            Err(if refused { format!("not cleaned: {why}") } else { format!("no model answered: {why}") })
        }
    }
}

/// TEXT cleaned paragraph by paragraph, the calls at once (blocking): the reply's `out`, or why there is none. His
/// paragraph breaks are kept as he typed them; quoted and fenced paragraphs go to no call. One paragraph over [`MAX`]
/// is refused; a long text is not.
pub fn clean(engine: &crate::policy::Engine, text: &str, names: &[String]) -> Result<String, String> {
    let parts = paragraphs(text);
    let todo: Vec<usize> = (0..parts.len()).filter(|&i| parts[i].1 && !left_alone(parts[i].0)).collect();
    for &i in &todo {
        let n = parts[i].0.chars().count();
        if n > MAX {
            return Err(format!("a paragraph is too long to clean in one call ({n} characters, at most {MAX})"));
        }
    }
    let n = text.chars().count();
    let start = std::time::Instant::now();
    let done: Vec<Result<(Value, String), String>> = std::thread::scope(|s| {
        let calls: Vec<_> = todo
            .iter()
            .map(|&i| {
                let p = parts[i].0;
                s.spawn(move || clean_one(engine, p, names))
            })
            .collect();
        calls.into_iter().map(|h| h.join().unwrap_or_else(|_| Err("the call's thread failed".into()))).collect()
    });
    let ms = start.elapsed().as_millis();
    let mut called = Vec::new();
    let mut models: Vec<String> = Vec::new();
    for (k, (&i, r)) in todo.iter().zip(done).enumerate() {
        match r {
            Ok((a, m)) => {
                called.push((i, a));
                if !m.is_empty() && !models.contains(&m) {
                    models.push(m);
                }
            }
            Err(e) => {
                eprintln!("coordd: clean: {n} characters, {} paragraphs, not cleaned in {ms} ms: {e}", todo.len());
                return Err(if todo.len() > 1 { format!("paragraph {} of {}: {e}", k + 1, todo.len()) } else { e });
            }
        }
    }
    let outcome = joined(&parts, &called);
    eprintln!("coordd: clean: {n} characters, {} paragraphs, {}, {}, {ms} ms", todo.len(), outcome["head"].as_str().unwrap_or(""), models.join(", "));
    Ok(reply(outcome, &models, ms))
}

/// An attempt's failure as recorded (`Invalid("the call changed ...")`, the Debug form) in words.
pub(crate) fn failure_words(f: &str) -> String {
    match f.split_once('(') {
        Some((_, inner)) => inner.strip_suffix(')').unwrap_or(inner).trim_matches('"').replace("\\\"", "\"").replace("\\`", "`"),
        None => f.to_string(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{Read, Write};

    fn names() -> Vec<String> {
        ["yaaihh", "paper-research", "docket-core"].map(String::from).to_vec()
    }

    /// The owner's typing from N-481, as he wrote it, and as it should come back.
    const TYPED: &str = "defofo possible and should be doen certainly if a lot of the message is bad. then it is woth it. the ui should show this happening if it takes more than say 1 or 2 s";
    const CLEANED: &str = "definitely possible and should be done certainly if a lot of the message is bad. then it is worth it. the ui should show this happening if it takes more than say 1 or 2 s";

    #[test]
    fn a_cleaning_of_the_spelling_alone_passes() {
        assert_eq!(check(TYPED, CLEANED, &names()), Ok(()));
        assert_eq!(check("so maybe harper and symspell for user facing suggestion siagnistiocs", "so maybe harper and symspell for user-facing suggestion diagnostics", &names()), Ok(()));
        // run-together words split: more words, the same text
        assert_eq!(check("wh yare feautres psushes so slwo", "why are features pushes so slow", &names()), Ok(()));
    }

    #[test]
    fn ids_paths_names_and_code_come_back_letter_for_letter_or_the_answer_is_refused() {
        let text = "@docket-core8 see N-487 p13 and src/lib.rs, commit 6fb1f8c, run `cargo tset -p coordd` on yaaihh";
        let good = "@docket-core8 see N-487 p13 and src/lib.rs, commit 6fb1f8c, run `cargo tset -p coordd` on yaaihh";
        assert_eq!(check(text, good, &names()), Ok(()));
        for (bad, what) in [
            (good.replace("N-487", "N-478"), "N-487"),
            (good.replace("src/lib.rs", "src/lib"), "src/lib.rs"),
            (good.replace("6fb1f8c", "6fb1f8"), "6fb1f8c"),
            (good.replace("@docket-core8", "@docket-core"), "@docket-core8"),
            (good.replace(" yaaihh", " yeah"), "yaaihh"),
        ] {
            let e = check(text, &bad, &names()).unwrap_err();
            assert!(e.contains(what), "{bad}: {e}");
        }
        // inside backticks nothing is fixed, not even a misspelling
        let e = check(text, &good.replace("cargo tset", "cargo test"), &names()).unwrap_err();
        assert!(e.contains("backticks"), "{e}");
        // a capital after the first letter is a name or a constant: PROTOCOL, CleanText
        assert!(check("raise PROTOCOL for CleanText", "raise Protocol for CleanText", &names()).is_err());
    }

    #[test]
    fn an_answer_that_is_not_the_message_cleaned_is_refused() {
        assert!(check("then it is woth it and the ui should show this happening when it takes more than a second", "Sure! Here is the corrected message.", &names()).unwrap_err().contains("not the text cleaned"));
        assert!(check("teh fix", "The fix is in. Let me know if you want more changes, or a rewording of the whole message.", &names()).is_err());
    }

    #[test]
    fn the_answer_keeps_his_own_white_space_around_it() {
        assert_eq!(framed("teh text\n", "the text"), "the text\n");
        assert_eq!(framed("  teh", "the\n\n"), "  the");
        let r: Value = serde_json::from_str(&reply(json!({"head": "Sure", "cleaned": "the x"}), &["gpt-6-luna".to_string()], 812)).unwrap();
        assert_eq!(r, json!({"head": "Sure", "cleaned": "the x", "model": "gpt-6-luna", "ms": 812}));
    }

    #[test]
    fn a_failure_reads_as_words() {
        assert_eq!(failure_words(r#"Invalid("the call changed a code, id or name: `N-487`")"#), "the call changed a code, id or name: `N-487`");
        assert_eq!(failure_words("Truncated"), "Truncated");
    }

    /// A stand-in proxy on loopback: its models, and each chat completion answered with ANSWER (the requests kept); an
    /// ANSWER of null is the gateway refusing the call (400, as it does a model it will not serve), and an ANSWER
    /// `{"refused": 429, "error": E}` is the gateway turning it away at a limit with E (tonight's 429s).
    pub(crate) fn stand_in(answer: fn(&str) -> Value) -> std::sync::Arc<std::sync::Mutex<Vec<Value>>> {
        const MODELS: [(&str, &str); 5] = [("gpt-6-luna", "openai"), ("gpt-6-sol", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6.2-sol", "openai"), ("claude-haiku-5-5", "anthropic")];
        stand_in_serving(&MODELS, move |req| {
            let asked = req["messages"][0]["content"].as_str().unwrap_or("");
            let text = asked.split_once("<<<\n").and_then(|(_, t)| t.rsplit_once("\n>>>")).map_or("", |(t, _)| t);
            match answer(text) {
                Value::Null => ("400 Bad Request", json!({"error": {"message": "unknown provider for model"}})),
                a if a["refused"] == 429 => ("429 Too Many Requests", json!({"error": a["error"]})),
                a => ("200 OK", json!({"choices": [{"finish_reason": "stop", "message": {"content": json!({"answer": a}).to_string()}}]})),
            }
        })
    }

    /// The stand-in proxy serving MODELS (id, owner): each chat completion's request kept and given to REPLY, which
    /// says the status and body it is answered with (models tests' gateway refuses a model at its limit with it).
    pub(crate) fn stand_in_serving(models: &'static [(&'static str, &'static str)], reply: impl Fn(&Value) -> (&'static str, Value) + Send + 'static) -> std::sync::Arc<std::sync::Mutex<Vec<Value>>> {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", l.local_addr().unwrap());
        let kept = seen.clone();
        std::thread::spawn(move || {
            for mut s in l.incoming().flatten() {
                let mut got = Vec::new();
                let mut buf = [0u8; 65536];
                // the head, then the body to its Content-Length
                loop {
                    let n = s.read(&mut buf).unwrap_or(0);
                    got.extend_from_slice(&buf[..n]);
                    let t = String::from_utf8_lossy(&got).to_string();
                    let Some(h) = t.find("\r\n\r\n") else { if n == 0 { break } else { continue } };
                    let len = t[..h].lines().find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if got.len() >= h + 4 + len || n == 0 {
                        break;
                    }
                }
                let t = String::from_utf8_lossy(&got).to_string();
                let (status, body) = if t.starts_with("GET /v1/models") {
                    let data: Vec<Value> = models.iter().map(|(m, p)| json!({"id": m, "owned_by": p})).collect();
                    ("200 OK", json!({"data": data}).to_string())
                } else {
                    let req: Value = t.split_once("\r\n\r\n").and_then(|(_, b)| serde_json::from_str(b).ok()).unwrap_or_default();
                    let (status, body) = reply(&req);
                    kept.lock().unwrap().push(req);
                    (status, body.to_string())
                };
                let _ = s.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
        seen
    }

    /// The worktree's agent-base, and a scratch coord under the temp folder.
    pub(crate) fn places() -> crate::policy::Places {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("clean-{}", std::process::id()));
        std::fs::create_dir_all(&coord).unwrap();
        crate::policy::Places { base, coord: coord.display().to_string(), ..Default::default() }
    }

    fn engine() -> crate::policy::Engine {
        crate::policy::Engine::start(places())
    }

    /// One call, his text and the fixed instruction only, on the model the policy chooses (codex, the cheapest).
    #[test]
    fn one_fresh_call_with_his_text_alone_on_the_policys_model() {
        let seen = stand_in(|_| json!({"head": "Sure", "cleaned": CLEANED}));
        let out = clean(&engine(), TYPED, &names()).unwrap();
        let r: Value = serde_json::from_str(&out).unwrap();
        assert_eq!((r["head"].as_str(), r["cleaned"].as_str(), r["model"].as_str()), (Some("Sure"), Some(CLEANED), Some("gpt-6.2-sol")), "medium, not the cheapest: decisions.eflint");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one call");
        let msgs = seen[0]["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1, "no context: one message");
        assert_eq!(msgs[0]["content"].as_str(), Some(prompt(TYPED).as_str()));
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    /// An answer that changed an id is never handed on: each model the policy allows is tried, then a refusal.
    #[test]
    fn an_answer_that_changed_an_id_is_refused_with_why() {
        let seen = stand_in(|_| json!({"head": "Sure", "cleaned": "see N-478 now"}));
        let e = clean(&engine(), "se N-487 now", &names()).unwrap_err();
        assert!(e.contains("`N-487`"), "{e}");
        assert!(!seen.lock().unwrap().is_empty());
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    #[test]
    fn too_long_or_empty_is_answered_without_a_call() {
        let seen = stand_in(|t| json!({"head": "Sure", "cleaned": t}));
        assert!(clean(&engine(), &"a".repeat(MAX + 1), &names()).unwrap_err().contains("too long"));
        // quoted paragraphs and fenced blocks go to no call
        let r: Value = serde_json::from_str(&clean(&engine(), "> teh post\n> quoted\n\n```\nteh\n\ncode\n```\n", &names()).unwrap()).unwrap();
        assert_eq!((r["head"].as_str(), r["cleaned"].as_str()), (Some("Sure"), Some(r["cleaned"].as_str().unwrap())));
        let r: Value = serde_json::from_str(&clean(&engine(), "  \n", &names()).unwrap()).unwrap();
        assert_eq!((r["head"].as_str(), r["cleaned"].as_str()), (Some("Sure"), Some(r["cleaned"].as_str().unwrap())));
        assert!(seen.lock().unwrap().is_empty());
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    #[test]
    fn a_text_is_cut_at_its_paragraphs_and_joins_back_as_it_was() {
        let text = "teh first\nline two\n\n\n> quoted\n> more\n\n```\na\n\nb\n```\nlast  \n\n";
        let parts = paragraphs(text);
        assert_eq!(parts.iter().map(|(p, _)| *p).collect::<String>(), text);
        let shown: Vec<(&str, bool)> = parts.clone();
        assert_eq!(shown, vec![("teh first\nline two\n", true), ("\n\n", false), ("> quoted\n> more\n", true), ("\n", false), ("```\na\n\nb\n```\nlast  \n", true), ("\n", false)]);
        assert!(left_alone("> quoted\n> more\n") && left_alone("```\na\n\nb\n```\n") && !left_alone("teh first\n"));
        assert_eq!(paragraphs("one"), vec![("one", true)]);
    }

    #[test]
    fn lines_and_quoted_lines_come_back_as_they_were() {
        let text = "> yuo said this\nand i aswer it";
        assert_eq!(check(text, "> yuo said this\nand i answer it", &names()), Ok(()));
        assert!(check(text, "> you said this\nand i answer it", &names()).unwrap_err().contains("quoted line"));
        assert!(check(text, "> yuo said this and i answer it", &names()).unwrap_err().contains("lines"));
        assert!(check("one line here", "one line\n\nhere", &names()).is_err());
    }

    /// A long message: one call a paragraph, at once, his breaks and quotes kept; the answer is the whole text.
    #[test]
    fn a_long_message_is_cleaned_paragraph_by_paragraph() {
        let seen = stand_in(|t| json!({"head": "Sure", "cleaned": t.replace("teh", "the")}));
        let para = "teh words of a paragraph that is long enough to count. ".repeat(60);
        let text = format!("{para}\n\n> teh quote stays\n\n{para}\n\n\n{para}\n");
        assert!(text.chars().count() > MAX);
        let r: Value = serde_json::from_str(&clean(&engine(), &text, &names()).unwrap()).unwrap();
        assert_eq!(r["cleaned"].as_str().unwrap(), text.replace("teh words", "the words"));
        assert_eq!(seen.lock().unwrap().len(), 3, "three paragraphs called, the quote not");
        crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
    }

    #[test]
    fn a_doubt_is_a_fix_not_made_and_unreadable_says_why() {
        let text = "i ment the woth of it";
        let ok = json!({"head": "Borderline", "cleaned": "i meant the woth of it", "doubts": [{"typed": "woth", "would": ["worth", "with"]}]});
        assert_eq!(check_answer(text, &ok, &names()), Ok(()));
        let made = json!({"head": "Borderline", "cleaned": "i meant the worth of it", "doubts": [{"typed": "woth", "would": ["worth"]}]});
        assert!(check_answer(text, &made, &names()).unwrap_err().contains("doubt"));
        let none = json!({"head": "Borderline", "cleaned": "i meant the woth of it", "doubts": [{"typed": "woth", "would": []}]});
        assert!(check_answer(text, &none, &names()).unwrap_err().contains("names nothing"));
        assert!(check_answer(text, &json!({"head": "Unreadable", "why": " "}), &names()).is_err());
        assert_eq!(check_answer(text, &json!({"head": "Unreadable", "why": "the second half"}), &names()), Ok(()));
        assert!(check_answer(text, &json!({"head": "Fine"}), &names()).is_err());
    }

    #[test]
    fn a_doubt_on_a_kept_word_is_no_doubt() {
        let names = vec!["Agda".to_string(), "eh".to_string()];
        let a = json!({"head": "Borderline", "cleaned": "the agda timings eh", "doubts": [{"typed": "agda", "would": ["agenda"]}, {"typed": "eh", "would": ["the"]}]});
        assert_eq!(without_kept_doubts(a, &names), json!({"head": "Sure", "cleaned": "the agda timings eh"}));
        let a = json!({"head": "Borderline", "cleaned": "agda meny", "doubts": [{"typed": "agda", "would": ["agenda"]}, {"typed": "meny", "would": ["menu", "many"]}]});
        assert_eq!(without_kept_doubts(a, &names)["doubts"], json!([{"typed": "meny", "would": ["menu", "many"]}]));
        // in any case, and a kept word comes back letter for letter
        assert!(check("the agda timings", "the Agda timings", &names).is_err());
    }

    #[test]
    fn the_kept_words_are_the_agents_the_projects_and_his() {
        let dir = std::env::temp_dir().join(format!("clean-keep-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(KEEP), "// the project's words\nAgda\nTovrin  // the system\n\nyaaihh\n").unwrap();
        let places = crate::policy::Places { base: "/nonexistent".into(), project: Some(dir.display().to_string()), coord: "/nonexistent".into(), ..Default::default() };
        let got = kept_words(&places, ["yaaihh".to_string()], &["eh".into(), " ".into(), "agda".into()]);
        assert_eq!(got, vec!["yaaihh", "Agda", "Tovrin", "eh"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn doubts_count_per_100_words_rounded_up() {
        assert_eq!((per100(0, 50), per100(1, 300), per100(3, 100), per100(2, 7)), (0, 1, 3, 29));
    }

    #[test]
    fn paragraphs_answer_as_their_worst() {
        let parts = paragraphs("teh a\n\nteh b\n");
        let sure = |c: &str| json!({"head": "Sure", "cleaned": c});
        let d = json!({"head": "Borderline", "cleaned": "teh b", "doubts": [{"typed": "teh", "would": ["the"]}]});
        assert_eq!(joined(&parts, &[(0, sure("the a")), (2, sure("the b"))]), json!({"head": "Sure", "cleaned": "the a\n\nthe b\n"}));
        assert_eq!(joined(&parts, &[(0, sure("the a")), (2, d)]), json!({"head": "Borderline", "cleaned": "the a\n\nteh b\n", "doubts": [{"typed": "teh", "would": ["the"]}], "per100": 25}));
        let u = json!({"head": "Unreadable", "why": "no idea"});
        assert_eq!(joined(&parts, &[(0, sure("the a")), (2, u)]), json!({"head": "Unreadable", "why": "paragraph 2 of 2: no idea"}));
    }

    /// The owner's own messages, cleaned on the real proxy (N-487 item 3): CLEAN_COORD the coord whose route and key
    /// are used (`$COORD/.proxy-key`, never printed), CLEAN_MESSAGES a file of his messages, one JSON string a line,
    /// CLEAN_MODELS the models, by comma, CLEAN_NAMES the kept words, by comma (as `kept_words` gives them). Prints a table: each message, its cleaning and the time, on each model, and
    /// fails if a cleaning was refused or not the message cleaned. Nothing leaves the machine but through the proxy.
    #[test]
    #[ignore]
    fn the_owners_messages_cleaned() {
        let (Ok(coord), Ok(file)) = (std::env::var("CLEAN_COORD"), std::env::var("CLEAN_MESSAGES")) else { return };
        let models: Vec<String> = std::env::var("CLEAN_MODELS").unwrap_or("gpt-6.1-sol,gpt-6.2-sol".into()).split(',').map(String::from).collect();
        let names: Vec<String> = std::env::var("CLEAN_NAMES").unwrap_or_default().split(',').filter(|n| !n.is_empty()).map(String::from).collect();
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let router = crate::accounts::router::Router::of(&crate::policy::Places { base, coord: coord.clone(), ..Default::default() });
        let texts: Vec<String> = std::fs::read_to_string(&file).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let mut refused = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            println!("\n### {}\n\ntyped:\n```\n{text}\n```", i + 1);
            for model in &models {
                // as clean() does, on MODEL: each paragraph a call, at once
                let parts = paragraphs(text);
                let start = std::time::Instant::now();
                let done: Vec<(usize, Result<Value, String>)> = std::thread::scope(|s| {
                    let calls: Vec<_> = (0..parts.len())
                        .filter(|&j| parts[j].1 && !left_alone(parts[j].0))
                        .map(|j| {
                            let (p, router, names) = (parts[j].0, &router, &names);
                            (j, s.spawn(move || {
                                let check = |a: &Value| check_answer(p, a, names);
                                router.call(TASK, &prompt(p.trim()), &schema(), model, &check).0.map(|a| without_kept_doubts(a, names)).map_err(|f| format!("{f:?}"))
                            }))
                        })
                        .collect();
                    calls.into_iter().map(|(j, h)| (j, h.join().unwrap())).collect()
                });
                let ms = start.elapsed().as_millis();
                let mut called = Vec::new();
                let mut failed = None;
                for (j, r) in done {
                    match r {
                        Ok(a) => called.push((j, a)),
                        Err(f) => failed = Some(f),
                    }
                }
                match failed {
                    None => {
                        let o = joined(&parts, &called);
                        let head = o["head"].as_str().unwrap_or("");
                        let body = o["cleaned"].as_str().or(o["why"].as_str()).unwrap_or("");
                        let doubts: Vec<String> = o["doubts"].as_array().into_iter().flatten().map(|d| format!("{} → {}", d["typed"].as_str().unwrap_or(""), d["would"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(" / "))).collect();
                        println!("\n{model}, {ms} ms, {head}{}{}:\n```\n{body}\n```", if body == text { ", unchanged" } else { "" }, if doubts.is_empty() { String::new() } else { format!(", {} doubts, {} per 100 words: {}", doubts.len(), o["per100"], doubts.join("; ")) });
                    }
                    Some(f) => {
                        println!("\n{model}, {ms} ms: refused: {f}");
                        refused.push(format!("{} on {model}: {f}", i + 1));
                    }
                }
            }
        }
        assert!(refused.is_empty(), "{refused:?}");
    }
}
