//! The owner's message split by point (N-284.p13: "we need to also split up user messages"; paper-research N-696):
//! one fresh call on his message as typed and the threads he is on, through the words program's `split` call (the
//! model the policy chooses for the task `split`, `policy::Engine::call_prompt`, as `clean` does). It proposes and
//! records nothing: each point is a passage of his own words, letter for letter, with the thread it belongs to (one
//! of those given, a new one, or the one he writes on); the window shows them under the box before anything is sent,
//! and he takes, changes or drops each (consult-3, N-590 p9: nothing lands where he did not look). An answer that
//! changed, left out or doubled any of his words, or named a thread not given, is refused, never handed on.

use serde_json::{json, Value};

/// The task and the call's name, as the policy and the proxy's request see them.
pub const TASK: &str = "split";

/// The most points one answer may have, and the most threads one call is given: the policy's facts
/// `split-points-at-most` and `split-threads-at-most` when it says them, else these.
pub const POINTS: usize = 8;
pub const THREADS: usize = 30;

/// A thread the call may put a point on: its id, label and agents, one line.
#[derive(Debug, Clone, PartialEq)]
pub struct Thread {
    pub id: String,
    pub label: String,
    pub on: Vec<String>,
}

/// The fixed instruction with TEXT and THREADS in it: everything the model is given.
pub fn prompt(text: &str, threads: &[Thread]) -> String {
    let list: Vec<String> = threads.iter().map(|t| format!("- {} · {} · {}", t.id, t.label, if t.on.is_empty() { "nobody".to_string() } else { t.on.join(", ") })).collect();
    format!(
        "Below is a message the owner typed to his agents, and the threads he is on. Say whether it holds several \
         separate points that belong to different threads.\n\
         \n\
         - A point is a passage of his message copied exactly, letter for letter, with its own punctuation and line \
         breaks. Do not reword, shorten, fix, translate or answer anything.\n\
         - Every word of the message is in exactly one point, in his order. Do not leave anything out.\n\
         - Each point goes to one thread: the id of a thread below it plainly belongs to, \"new\" when it is a topic \
         none of them has, or \"here\" when it belongs where he is writing or you are not sure.\n\
         \n\
         Answer one of:\n\
         - One: it is one point, or all of it belongs in one place.\n\
         - Points(points): each with WORDS (the passage) and TO (an id below, \"new\" or \"here\").\n\
         \n\
         The threads:\n\
         {}\n\
         \n\
         The message:\n\
         <<<\n\
         {text}\n\
         >>>",
        if list.is_empty() { "(none)".to_string() } else { list.join("\n") }
    )
}

/// The answer's shape: One | Points(points), `head` first in each.
pub fn schema() -> Value {
    let point = json!({"type": "object", "properties": {"words": {"type": "string"}, "to": {"type": "string"}}, "required": ["words", "to"], "additionalProperties": false});
    json!({"anyOf": [
        {"type": "object", "properties": {"head": {"type": "string", "enum": ["One"]}}, "required": ["head"], "additionalProperties": false},
        {"type": "object", "properties": {"head": {"type": "string", "enum": ["Points"]}, "points": {"type": "array", "items": point}}, "required": ["head", "points"], "additionalProperties": false},
    ]})
}

/// Whether C is left between points: white space and the marks a split leaves at a seam.
fn seam(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | ',' | ';' | ':' | '-' | '–' | '—' | '/' | '|')
}

/// One answer A held to TEXT: every point's words stand in TEXT as typed, in order, none overlapping, and all of them
/// together leave only seams ([`seam`]) uncovered; each TO is one of IDS, `new` or `here`; at most AT_MOST points.
/// The points' places in TEXT (characters from, length), when it holds.
pub fn check_answer(text: &str, a: &Value, ids: &[String], at_most: usize) -> Result<Vec<(usize, usize)>, String> {
    match a["head"].as_str() {
        Some("One") => Ok(Vec::new()),
        Some("Points") => {
            let points = a["points"].as_array().ok_or("no points")?;
            if points.len() > at_most {
                return Err(format!("{} points, at most {at_most}", points.len()));
            }
            let mut from = 0usize; // bytes into TEXT
            let mut places = Vec::new();
            let mut covered = 0usize; // bytes covered by points
            for p in points {
                let words = p["words"].as_str().unwrap_or("").trim();
                let to = p["to"].as_str().unwrap_or("");
                if words.is_empty() {
                    return Err("a point with no words".into());
                }
                if !(to == "new" || to == "here" || ids.iter().any(|i| i == to)) {
                    return Err(format!("a point goes to {to:?}, which is no thread it was given"));
                }
                let Some(i) = text[from..].find(words) else {
                    return Err(format!("a point's words are not his as typed, or out of order: {:?}", words.chars().take(60).collect::<String>()));
                };
                if !text[from..from + i].chars().all(seam) {
                    return Err(format!("words of his are in no point: {:?}", text[from..from + i].trim().chars().take(60).collect::<String>()));
                }
                let start = from + i;
                places.push((text[..start].chars().count(), words.chars().count()));
                from = start + words.len();
                covered += words.len();
            }
            if !text[from..].chars().all(seam) {
                return Err(format!("words of his are in no point: {:?}", text[from..].trim().chars().take(60).collect::<String>()));
            }
            debug_assert!(covered <= text.len());
            Ok(places)
        }
        _ => Err("an answer that is neither One nor Points".into()),
    }
}

/// The reply's `out` for TEXT and the checked answer A (its points at PLACES): `{"head": "One"}`, or the points each
/// with its words, place (`from`, `len`, in characters), `to`, and the thread's label when it is one of THREADS; the
/// model and the time beside. All points `here` is One: nothing to show him.
pub fn reply(a: &Value, places: &[(usize, usize)], threads: &[Thread], models: &[String], ms: u128) -> String {
    let points: Vec<Value> = a["points"]
        .as_array()
        .map(|ps| {
            ps.iter()
                .zip(places)
                .map(|(p, (from, len))| {
                    let to = p["to"].as_str().unwrap_or("here");
                    let label = threads.iter().find(|t| t.id == to).map(|t| t.label.clone());
                    json!({"words": p["words"].as_str().unwrap_or("").trim(), "from": from, "len": len, "to": to, "label": label})
                })
                .collect()
        })
        .unwrap_or_default();
    let mut out = if points.is_empty() || points.iter().all(|p| p["to"] == "here") { json!({"head": "One"}) } else { json!({"head": "Points", "points": points}) };
    out["model"] = json!(models.join(", "));
    out["ms"] = json!(ms as u64);
    out.to_string()
}

/// TEXT's points proposed, among THREADS (blocking): the reply's `out`, or why there is none. One call; its answer is
/// held to [`check_answer`] and refused (the policy's next model tried, as for any call) when it does not hold.
pub fn split(engine: &crate::policy::Engine, text: &str, threads: &[Thread], at_most: usize) -> Result<String, String> {
    let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
    let start = std::time::Instant::now();
    let check = |a: &Value| check_answer(text, a, &ids, at_most).map(|_| ());
    let c = engine.call_prompt(TASK, TASK, &prompt(text, threads), &schema(), "owner", &check)?;
    let ms = start.elapsed().as_millis();
    let n = text.chars().count();
    let Some(a) = c.answer else {
        let why = c.decision.fault.clone().unwrap_or_else(|| "no model answered".into());
        eprintln!("coordd: split: {n} characters, not split in {ms} ms: {why}");
        return Err(format!("not split: {why}"));
    };
    let places = check_answer(text, &a, &ids, at_most)?;
    let model = c.decision.chosen.clone().unwrap_or_default();
    let out = reply(&a, &places, threads, &[model.clone()], ms);
    eprintln!("coordd: split: {n} characters, {} points, {model}, {ms} ms", places.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSG: &str = "agda-dev is still not in the window. runner rows: yes, do them. autostart: yes";

    fn ids() -> Vec<String> {
        vec!["N-575".into(), "N-610".into()]
    }

    fn points(ps: &[(&str, &str)]) -> Value {
        json!({"head": "Points", "points": ps.iter().map(|(w, t)| json!({"words": w, "to": t})).collect::<Vec<_>>()})
    }

    #[test]
    fn a_split_holds_only_his_words_whole_in_order_on_threads_it_was_given() {
        let good = points(&[("agda-dev is still not in the window.", "new"), ("runner rows: yes, do them.", "N-575"), ("autostart: yes", "here")]);
        assert_eq!(check_answer(MSG, &good, &ids(), POINTS).unwrap(), [(0, 36), (37, 26), (64, 14)]);
        assert_eq!(check_answer(MSG, &json!({"head": "One"}), &ids(), POINTS).unwrap(), []);
        // a word changed, a passage left out, the order swapped, a thread not given, too many: refused
        let changed = points(&[("agda-dev is still not in the window.", "new"), ("runner rows: yes, do them!", "N-575"), ("autostart: yes", "here")]);
        assert!(check_answer(MSG, &changed, &ids(), POINTS).unwrap_err().contains("not his as typed"));
        let left_out = points(&[("agda-dev is still not in the window.", "new"), ("autostart: yes", "here")]);
        assert!(check_answer(MSG, &left_out, &ids(), POINTS).unwrap_err().contains("in no point"));
        let tail_out = points(&[("agda-dev is still not in the window.", "new"), ("runner rows: yes, do them.", "N-575")]);
        assert!(check_answer(MSG, &tail_out, &ids(), POINTS).unwrap_err().contains("in no point"));
        let swapped = points(&[("runner rows: yes, do them.", "N-575"), ("agda-dev is still not in the window.", "new"), ("autostart: yes", "here")]);
        assert!(check_answer(MSG, &swapped, &ids(), POINTS).is_err());
        let elsewhere = points(&[("agda-dev is still not in the window.", "N-999"), ("runner rows: yes, do them. autostart: yes", "here")]);
        assert!(check_answer(MSG, &elsewhere, &ids(), POINTS).unwrap_err().contains("no thread it was given"));
        assert!(check_answer(MSG, &good, &ids(), 2).unwrap_err().contains("at most 2"));
    }

    #[test]
    fn the_reply_names_each_points_thread_by_label_and_all_here_is_one() {
        let threads = vec![Thread { id: "N-575".into(), label: "Runner rows".into(), on: vec!["docket-core".into()] }];
        let a = points(&[("agda-dev is still not in the window.", "new"), ("runner rows: yes, do them. autostart: yes", "N-575")]);
        let places = check_answer(MSG, &a, &["N-575".to_string()], POINTS).unwrap();
        let r: Value = serde_json::from_str(&reply(&a, &places, &threads, &["m".to_string()], 900)).unwrap();
        assert_eq!(r["head"], "Points");
        assert_eq!(r["points"][1], json!({"words": "runner rows: yes, do them. autostart: yes", "from": 37, "len": 41, "to": "N-575", "label": "Runner rows"}));
        assert_eq!(r["points"][0]["label"], Value::Null);
        let here = points(&[("agda-dev is still not in the window.", "here"), ("runner rows: yes, do them. autostart: yes", "here")]);
        let places = check_answer(MSG, &here, &[], POINTS).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reply(&here, &places, &[], &[], 1)).unwrap()["head"], "One");
        assert!(prompt(MSG, &threads).contains("- N-575 · Runner rows · docket-core"));
    }
}
