//! Forming a step's answer from an agent's turn (owner m613, plan/delta-m613.md): the last message of the turn is the
//! answer; one quick stateless call reads which constructor of the step's head type it gives, and the short fields that
//! are not text (a commit, a number). A field of type `Text` is never written by the model: it is the turn's output
//! itself, by reference (texts are passed by reference, never copied). No head that types: None, and the machine
//! reminds the agent once (Remind).

use crate::policy::Engine;
use docket_machine::Type;
use serde_json::{Value, json};

/// Whether T is a text (or an optional text): a field the output itself fills.
fn is_text(t: &Type) -> bool {
    match t {
        Type::Text => true,
        Type::Opt(t) => is_text(t),
        _ => false,
    }
}

#[path = "form_parts.rs"]
pub(crate) mod parts;

/// Whether constructor fields FS have two or more parts (N-101): each is then cut from the output, not the whole of it.
fn many(fs: &[(String, Type)]) -> bool {
    parts::parts(fs).len() >= 2
}

/// The fields of FS the model is asked for: those not cut from the output (not a text, or with two or more parts, not
/// a part).
fn for_model(fs: &[(String, Type)]) -> Vec<(String, Type)> {
    let cut = many(fs);
    fs.iter().filter(|(_, t)| if cut { parts::part(t).is_none() } else { !is_text(t) }).cloned().collect()
}

/// The head of type HEAD that the turn output TEXT (at LOC) gives, as AGENT's answer (None when it gives none), with the
/// decision that chose the model read it with, when one was called (recorded on the ticket as a `model-for` line).
/// Blocking: called off the lock.
/// CONTEXT: for the owner's words typed against a step owed to the owner (§12), the step as the owner was shown it; the
/// prompt is then the owner's reply to that question, not an agent's turn, and a text field is the words themselves
/// (an Again(text) is the message said again; paper-research m1339, b1).
/// A constructor with two or more parts (N-101) has each cut from TEXT: at its headings when every required one is
/// there, with no model call for them; otherwise from the marks the reading call gives (form_parts.rs).
/// Err: the forming call itself faulted (no model could be asked, or none answered): coordd's own failure, for its
/// journal, never a post (N-159.p16); not an answer that has no form.
/// SHOWN: for an agent's answer, the step as the agent was shown it (the place its first delivery was written in its
/// session, else the call rendered again): the forming call reads the answer against it (the owner, N-246.p1: turning a
/// free answer into fields is a fresh call's job, and it gets the question). Every answer is read by that call: its
/// first word decides nothing.
/// GIVES: what the step gives with its docs (`doc::gives`), the text the agent was shown, for the marks reading.
/// NAMES: the lists of the answer whose entries name other elements of it (`Defs.names` of the step's call: run 4's
/// `stands_on` names `parts[].name`, paper-research N-101 p1141): each entry is taken as the names it gives, none cut
/// at a comma, and an answer with an entry that names none is not taken, the agent told which and the names there are.
#[allow(clippy::too_many_arguments)]
pub fn form(engine: &Engine, agent: &str, head: &Type, text: &str, loc: &Value, context: Option<&str>, shown: Option<&str>, gives: Option<&str>, names: &[(String, String)]) -> Formed {
    form_by(engine, agent, head, text, loc, context, shown, gives, names, true)
}

/// `form`, with the cut at the headings or (HEADINGS false: the marks try, paper-research N-101 p648) without it.
#[allow(clippy::too_many_arguments)]
fn form_by(engine: &Engine, agent: &str, head: &Type, text: &str, loc: &Value, context: Option<&str>, shown: Option<&str>, gives: Option<&str>, names: &[(String, String)], headings: bool) -> Formed {
    let with = if context.is_some() { json!(text) } else { json!({"loc": loc}) };
    let Type::Variant(all) = head else {
        let h = whole(head, &with);
        let not_taken = h.is_none().then(|| format!("The step's answer is a {head}, which is not read from an answer."));
        return Ok(Reading { head: h, not_taken, ..Default::default() });
    };
    let cs: Vec<(String, Vec<(String, Type)>)> = all.clone();
    let place = context.is_none().then_some(loc);
    // the parts at their headings: of the one constructor whose every heading is there
    let cut = |fs: &[(String, Type)]| if headings && many(fs) { parts::by_headings(text, &parts::parts(fs)).ok() } else { None };
    let hits: Vec<(String, Vec<(String, Type)>, Vec<(String, parts::Found)>)> = cs.iter().filter_map(|(c, fs)| Some((c.clone(), fs.clone(), cut(fs)?))).collect();
    if let [(c, fs, found)] = hits.as_slice() {
        let finish = |mut a: Value| -> Result<Value, String> {
            parts::fill_with(a.as_object_mut().ok_or("the reading is no record")?, &parts::parts(fs), found, text, place, &parts::whole(names, c));
            head.conforms(&a).map(|_| a)
        };
        if cs.len() == 1 {
            let asked = for_model(fs);
            if asked.is_empty() {
                return Ok(taken_named(Reading::default(), finish(json!({"head": c})), names));
            }
            let mut r = read(engine, agent, &[(c.clone(), asked)], text, context, shown)?;
            return Ok(match r.head.take() {
                Some(a) => taken_named(r, finish(a), names),
                None => r,
            });
        }
        // other outcomes beside it (a Design and an Ask): the reading call says which one the answer gives, over all of
        // them with their parts left out; the one whose headings are all there has its parts cut at them, with no model
        // (since 14972f1 every such answer went to the tagged call; paper-research N-101 p602, docket-core p617)
        let asked: Vec<(String, Vec<(String, Type)>)> = cs.iter().map(|(c, fs)| (c.clone(), for_model(fs))).collect();
        let mut r = read(engine, agent, &asked, text, context, shown)?;
        let other_in_parts = |a: &Value| a["head"].as_str().is_some_and(|h| h != c && cs.iter().any(|(d, fs)| d == h && many(fs)));
        return match r.head.take() {
            Some(a) if a["head"].as_str() == Some(c.as_str()) => Ok(taken_named(r, finish(a), names)),
            // another outcome in parts, whose headings are not all there: its parts come from the marks
            Some(a) if other_in_parts(&a) => marked(engine, agent, head, &cs, text, &with, place, shown, gives, names).map(|mut t| {
                t.calls.splice(0..0, r.calls);
                t
            }),
            Some(a) => Ok(taken_named(r, fill(head, a, &with).map(|a| own_text(head, a, text, &with)).map(|a| by_repository(head, a, text)).ok_or_else(|| String::from("its text fields could not be filled")), names)),
            None => Ok(r),
        };
    }
    if cs.iter().any(|(_, fs)| many(fs)) {
        return marked(engine, agent, head, &cs, text, &with, place, shown, gives, names);
    }
    let asked: Vec<(String, Vec<(String, Type)>)> = cs.iter().map(|(c, fs)| (c.clone(), for_model(fs))).collect();
    let mut r = read(engine, agent, &asked, text, context, shown)?;
    Ok(match r.head.take() {
        Some(a) => taken_named(r, fill(head, a, &with).map(|a| own_text(head, a, text, &with)).map(|a| by_repository(head, a, text)).ok_or_else(|| String::from("its text fields could not be filled")), names),
        None => r,
    })
}

/// R with HEAD as its head, or, when the reading did not fit the step's outcomes, none and why.
fn taken(mut r: Reading, head: Result<Value, String>) -> Reading {
    match head {
        Ok(h) => r.head = Some(h),
        Err(e) => r.not_taken = Some(format!("The forming call's reading was not taken: {e}.")),
    }
    r
}

/// `taken`, a head whose entries name other elements (NAMES) taken as the names they give (`parts::named`): one that
/// names none is not taken, and WHY tells the agent which, with the names there are.
fn taken_named(r: Reading, head: Result<Value, String>, names: &[(String, String)]) -> Reading {
    let head = head.map(|mut a| (parts::named(&mut a, names), a));
    match head {
        Ok((Ok(()), a)) => taken(r, Ok(a)),
        Ok((Err(why), _)) => {
            let mut r = taken(r, Err(why.clone()));
            r.why = Some(why);
            r
        }
        Err(e) => taken(r, Err(e)),
    }
}

/// One forming call, as coordd keeps it (paper-research N-101 p554, 1): the call, the model that answered, the prompt
/// sent and the reply, whole.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Kept {
    pub call: String,
    pub model: Option<String>,
    /// The model as the policy orders it, its number and class (`gpt-6-astra (2, medium)`), and the rules that left
    /// the other models out: why that model ran (paper-research N-101 p631).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<String>,
    pub prompt: String,
    pub reply: Option<Value>,
}

/// What forming gives: the head (None when the answer gives none of the step's outcomes), the decision that chose the
/// model (the last call's), WHY when there is none (the forming call's one sentence, which the agent is told), NOT_TAKEN
/// (coordd's own reason: a reading that did not fit, parts that could not be cut; for the record only), and the calls
/// made, to be kept.
#[derive(Clone, Debug, Default)]
pub struct Reading {
    pub head: Option<Value>,
    pub decision: Option<coord_types::Decision>,
    pub why: Option<String>,
    pub not_taken: Option<String>,
    pub calls: Vec<Kept>,
}

/// Err when the forming call faulted.
pub type Formed = Result<Reading, String>;

/// Whether CALLED is a fault of the call rather than the model's reading: no model was chosen, or every one failed.
fn faulted(called: &crate::policy::Called) -> Option<String> {
    called.decision.chosen.is_none().then(|| called.decision.fault.clone().unwrap_or_else(|| "no model answered".into()))
}

/// The answer of a step whose head is no variant, WITH the output by place (or the owner's words): a text is the
/// whole of it (`-> Text`, as the library's assemble_plan; N-159); any other is not formed.
fn whole(head: &Type, with: &Value) -> Option<Value> {
    is_text(head).then(|| with.clone())
}

/// The name of the answer that is none of CS's outcomes: `None`, unless an outcome has that name.
fn none_of(cs: &[(String, Vec<(String, Type)>)]) -> String {
    ["None", "NoneOfThem", "NoOutcome"].into_iter().find(|n| !cs.iter().any(|(c, _)| c == n)).unwrap_or("NoOutcome").to_string()
}

/// The forming call NAME with PROMPT, its answer one of CS or none of them (with why): what it reads, kept in R.
fn call(engine: &Engine, agent: &str, name: &str, prompt: String, cs: &[(String, Vec<(String, Type)>)], r: &mut Reading) -> Result<Option<Value>, String> {
    let none = none_of(cs);
    let mut with = cs.to_vec();
    with.push((none.clone(), vec![("why".into(), Type::Text)]));
    let reduced = Type::Variant(with);
    let check = |a: &Value| reduced.conforms(a);
    let called = engine.call_prompt("form", name, &prompt, &reduced.schema(), agent, &check)?;
    if let Some(e) = faulted(&called) {
        return Err(e);
    }
    let mut rules: Vec<String> = called.decision.excluded.iter().filter(|x| x.reason == "only").map(|x| x.rule.clone()).collect();
    rules.sort();
    rules.dedup();
    r.calls.push(Kept { call: name.to_string(), model: called.decision.chosen.clone(), class: None, rules, prompt, reply: called.answer.clone() });
    r.decision = Some(called.decision);
    Ok(match called.answer {
        Some(a) if a.get("head").and_then(Value::as_str) == Some(none.as_str()) => {
            r.why = Some(a.get("why").and_then(Value::as_str).unwrap_or_default().trim().to_string());
            None
        }
        Some(a) => Some(a),
        None => {
            r.not_taken = Some(String::from("The forming call's reply could not be read."));
            None
        }
    })
}

/// The model's reading of TEXT as one of CS (each constructor with the fields it is asked for): the answer without
/// its text fields, the decision that chose the model, and why none.
fn read(engine: &Engine, agent: &str, cs: &[(String, Vec<(String, Type)>)], text: &str, context: Option<&str>, shown: Option<&str>) -> Formed {
    let prompt = prompt(&choices(cs), &none_of(cs), text, context, shown);
    let mut r = Reading::default();
    r.head = call(engine, agent, "form-head", prompt, cs, &mut r)?;
    Ok(r)
}

fn choices(cs: &[(String, Vec<(String, Type)>)]) -> Vec<String> {
    cs.iter().map(|(c, fs)| if fs.is_empty() { c.clone() } else { format!("{c} ({})", fs.iter().map(|(f, t)| format!("{f}: {t}")).collect::<Vec<_>>().join(", ")) }).collect()
}

/// The reading call with marks (the owner, N-101 p648): the model is given the answer once, its lines numbered, and
/// gives the constructor, the fields that are not parts, and for a constructor with parts each part by its first and
/// last line (a name and a list of names as values); the engine cuts the lines out. Asked once more, with what failed,
/// if they cannot be taken; then none (the machine's one reminder), and why says what failed.
#[allow(clippy::too_many_arguments)]
fn marked(engine: &Engine, agent: &str, head: &Type, cs: &[(String, Vec<(String, Type)>)], text: &str, with: &Value, place: Option<&Value>, shown: Option<&str>, gives: Option<&str>, names: &[(String, String)]) -> Formed {
    let asked: Vec<(String, Vec<(String, Type)>)> = cs.iter().map(|(c, fs)| {
        let mut a = for_model(fs);
        if many(fs) {
            a.extend(parts::mark_fields(&parts::parts(fs)));
        }
        (c.clone(), a)
    }).collect();
    let ps: Vec<(String, Vec<(String, parts::Part)>)> = cs.iter().filter(|(_, fs)| many(fs)).map(|(c, fs)| (c.clone(), parts::parts(fs))).collect();
    let choices = choices(&cs.iter().map(|(c, fs)| (c.clone(), for_model(fs))).collect::<Vec<_>>());
    let none = none_of(&asked);
    let mut r = Reading::default();
    let mut failed = None::<String>;
    // the entries naming none, the last time: what the agent is told when the second reading has them too
    let mut missed = None::<String>;
    for _ in 0..2 {
        let prompt = parts::marks_prompt_with(&choices, &none, &ps, text, failed.as_deref(), shown, gives, names);
        let Some(mut a) = call(engine, agent, "form-parts", prompt, &asked, &mut r)? else { return Ok(r) };
        let c = a.get("head").and_then(Value::as_str).unwrap_or_default().to_string();
        let Some(fs) = cs.iter().find(|(n, _)| *n == c).map(|(_, fs)| fs) else { return Ok(taken(r, Err(format!("{c} is none of the step's outcomes")))) };
        if !many(fs) {
            let h = fill(head, a, with).map(|a| own_text(head, a, text, with)).ok_or_else(|| String::from("its text fields could not be filled"));
            return Ok(taken_named(r, h, names));
        }
        let whole = parts::whole(names, &c);
        match parts::by_marks_with(&a, text, &parts::parts(fs), &whole) {
            Ok(found) => {
                if let Some(o) = a.as_object_mut() {
                    parts::fill_with(o, &parts::parts(fs), &found, text, place, &whole);
                }
                let h = head.conforms(&a).map(|_| a);
                let h = match h {
                    Ok(mut a) => parts::named(&mut a, names).map(|_| a).map_err(|why| (why, true)),
                    Err(e) => Err((e, false)),
                };
                match h {
                    Ok(a) => return Ok(taken(r, Ok(a))),
                    // the reading may have written an entry as the answer does ("Part 1"): read once more, told so
                    Err((why, true)) => {
                        missed = Some(why.clone());
                        failed = Some(why);
                    }
                    Err((e, false)) => return Ok(taken(r, Err(e))),
                }
            }
            Err(why) => {
                eprintln!("coordd: form: {agent}'s {c}: the parts' marks were not taken: {why}");
                failed = Some(why);
            }
        }
    }
    r.not_taken = Some(format!("The answer's parts could not be cut from it, twice: {}", failed.as_deref().unwrap_or_default()));
    if missed.is_some() && missed == failed {
        r.why = missed;
    }
    Ok(r)
}

/// The form call's prompt: TEXT, an agent's answer to the step SHOWN (as it was shown it; none when it could not be
/// read), read as one of CHOICES or NONE; or, with CONTEXT (the step as the owner was shown it), the owner's reply to
/// that question.
fn prompt(choices: &[String], none: &str, text: &str, context: Option<&str>, shown: Option<&str>) -> String {
    match context {
        Some(q) => format!(
            "The owner of a software team was asked the question below and typed the reply below. Its answer is one of: {}. \
             Say which of these the reply answers, and fill the fields with what it says (an agent by its name: \
             \"for lead\" is the lead). {}\n\n\
             The question, as the owner was shown it:\n---\n{}\n---\n\nThe owner's reply:\n<<<\n{text}\n>>>",
            choices.join("; "),
            none_said(none, "reply"),
            q.trim()
        ),
        None => format!("{}\n\nThe agent's answer:\n<<<\n{text}\n>>>", agent_intro(choices, none, shown, "Say which outcome the answer gives, and fill that outcome's fields from what the answer says")),
    }
}

/// What the forming call does when the WHAT (answer, reply) gives none of the outcomes, or two: answers NONE, saying
/// which. What the WHAT says is not judged (paper-research N-101 p757, 1: gpt-6-luna refused five minder answers for
/// what they said).
fn none_said(none: &str, what: &str) -> String {
    format!("Answer {none} only when the {what} gives none of the outcomes, or gives two of them. Whether what it says is right or complete is not yours to judge. With `why`: one sentence saying which of the two it is.")
}

/// What an agent's answer is read against, for both forming calls: what the call does, the step's outcomes CHOICES,
/// the answer that is none of them (NONE), what to do (ASK), and the step as the agent was shown it (SHOWN).
pub(crate) fn agent_intro(choices: &[String], none: &str, shown: Option<&str>, ask: &str) -> String {
    let step = match shown {
        Some(s) => format!("\n\nThe step, as the agent was shown it when it was given (the answer may name what the agent made since, such as a new agent or a commit):\n---\n{}\n---", s.trim()),
        None => String::from("\n\n(The step's text could not be read: only its outcomes are known.)"),
    };
    format!(
        "You turn an agent's answer to a step of its work into data. Below is the step the agent was given, then its answer. \
         The step's answer is one of these outcomes, each with its fields: {}. {ask}, each value as the answer gives \
         it (an agent by its name, a commit by its hash, a path as written). {}{step}",
        choices.join("; "),
        none_said(none, "answer")
    )
}

/// A, a head of HEAD without its text fields, with each set to WITH: the output's place (`{"loc": ...}`), or the owner's
/// words as they are.
fn fill(head: &Type, mut a: Value, with: &Value) -> Option<Value> {
    let c = a.get("head")?.as_str()?.to_string();
    for (f, t) in head.constructor(&c)? {
        if is_text(t) {
            a[f.as_str()] = with.clone();
        }
    }
    head.conforms(&a).ok().map(|_| a)
}

/// A, filled, with its one text field the part of TEXT after the form's own labels (form_parts.rs `labelled`): a span
/// of the output's place WITH, or, for the owner's words, those words. Otherwise A as it is: the whole output.
fn own_text(head: &Type, mut a: Value, text: &str, with: &Value) -> Value {
    let Some(c) = a.get("head").and_then(Value::as_str).map(str::to_string) else { return a };
    let Some(fs) = head.constructor(&c) else { return a };
    let texts: Vec<&String> = fs.iter().filter(|(_, t)| is_text(t)).map(|(f, _)| f).collect();
    let [f] = texts.as_slice() else { return a };
    let Some((at, len)) = parts::labelled(text, &c, fs, f) else { return a };
    a[f.as_str()] = match with.get("loc") {
        Some(l) => json!({"span": {"loc": l, "at": at, "len": len}}),
        None => json!(text[at..at + len]),
    };
    a
}

/// A, the answer to TEXT, with its one Ref field by repository when TEXT's first line gives it so after the head:
/// "Commit b f5d6af7" is {"b": "f5d6af7"}, "Commit a 1234567 b 89abcde" both (N-159: the model read "Commit b
/// f5d6af7" as the bare f5d6af7, and a bare commit goes to the first repository). Otherwise A as it is.
fn by_repository(head: &Type, mut a: Value, text: &str) -> Value {
    let Some(fs) = a.get("head").and_then(Value::as_str).and_then(|c| head.constructor(c)) else { return a };
    let refs: Vec<&String> = fs.iter().filter(|(_, t)| matches!(t, Type::Ref) || matches!(t, Type::Opt(x) if **x == Type::Ref)).map(|(f, _)| f).collect();
    let [f] = refs.as_slice() else { return a };
    let words: Vec<&str> = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").split_whitespace().skip(1).map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')).collect();
    let hex = |w: &str| (7..=40).contains(&w.len()) && w.chars().all(|c| c.is_ascii_hexdigit());
    if words.is_empty() || words.len() % 2 != 0 {
        return a;
    }
    let mut by = serde_json::Map::new();
    for p in words.chunks(2) {
        if hex(p[0]) || !hex(p[1]) || p[0].is_empty() {
            return a;
        }
        by.insert(p[0].to_string(), json!(p[1]));
    }
    let mut b = a.clone();
    b[f.as_str()] = Value::Object(by);
    if head.conforms(&b).is_ok() {
        a = b;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A step answering a text (`-> Text`) is answered by the whole output, by place; the owner's words as they are.
    #[test]
    fn a_text_answer_is_the_whole_output() {
        let loc = json!({"file": "/t.jsonl", "offset": 7, "len": 9});
        assert_eq!(whole(&Type::Text, &json!({"loc": loc})), Some(json!({"loc": loc})));
        assert_eq!(whole(&Type::Opt(Box::new(Type::Text)), &json!("the words")), Some(json!("the words")));
        assert_eq!(whole(&Type::Ref, &json!({"loc": loc})), None);
    }

    /// "Commit b f5d6af7": the commit by repository, not the bare sha a project would take as its first repository's.
    #[test]
    fn a_commit_named_with_its_repository_is_kept_by_repository() {
        let head = Type::Variant(vec![("Commit".into(), vec![("commit".into(), Type::Ref)]), ("Cannot".into(), vec![("why".into(), Type::Text)])]);
        let read = json!({"head": "Commit", "commit": "f5d6af7"});
        assert_eq!(by_repository(&head, read.clone(), "Commit b f5d6af7\n\nI checked it."), json!({"head": "Commit", "commit": {"b": "f5d6af7"}}));
        assert_eq!(by_repository(&head, read.clone(), "Commit a 1234567 b 89abcde"), json!({"head": "Commit", "commit": {"a": "1234567", "b": "89abcde"}}));
        assert_eq!(by_repository(&head, read.clone(), "Commit f5d6af7"), read, "a bare sha stays as read");
        assert_eq!(by_repository(&head, read.clone(), "Commit: done, see f5d6af7"), read, "words that are no pairs");
    }

    #[test]
    fn text_fields_are_the_output_by_reference() {
        let head = Type::Variant(vec![("Commit".into(), vec![("commit".into(), Type::Ref)]), ("Cannot".into(), vec![("why".into(), Type::Text)])]);
        let loc = json!({"file": "/t.jsonl", "offset": 7, "len": 9});
        assert_eq!(fill(&head, json!({"head": "Cannot"}), &json!({"loc": loc})), Some(json!({"head": "Cannot", "why": {"loc": loc}})));
        assert_eq!(fill(&head, json!({"head": "Commit"}), &json!({"loc": loc})), None); // its commit is not text: the model must read it
    }

    /// A text field is the answer after the form's own labels (docket-core3's `--say` run, N-101 p981, p992): orch5's
    /// "Direct / agent: writer / words: ..." gives `words` the owner's words only, as a span of the output; an answer
    /// that opens with its own words is whole, and a later line that looks like a label does not cut it.
    #[test]
    fn a_text_field_is_the_answer_after_its_own_labels() {
        let head = Type::Variant(vec![("Talk".into(), vec![]), ("Direct".into(), vec![("agent".into(), Type::Agent), ("words".into(), Type::Text)]), ("Cannot".into(), vec![("why".into(), Type::Text)])]);
        let loc = json!({"file": "/t.jsonl", "offset": 7, "len": 90});
        let with = json!({"loc": loc});
        let answer = "Direct\nagent: writer\nwords: In repository a, add notes/about.txt.\nOne paragraph.\n";
        let a = fill(&head, json!({"head": "Direct", "agent": "writer"}), &with).unwrap();
        let at = answer.find("In repository").unwrap();
        let span = json!({"span": {"loc": loc, "at": at, "len": "In repository a, add notes/about.txt.\nOne paragraph.".len()}});
        assert_eq!(own_text(&head, a.clone(), answer, &with)["words"], span);
        // the label alone on its line, a `**Direct**` heading, and no label for the text: the same words
        for answer in ["**Direct**\n\nagent: writer\n\nWords:\nIn repository a, add notes/about.txt.\nOne paragraph.\n", "Direct\nagent: writer\n\nIn repository a, add notes/about.txt.\nOne paragraph.\n"] {
            let at = answer.find("In repository").unwrap();
            assert_eq!(own_text(&head, a.clone(), answer, &with)["words"]["span"]["at"], json!(at), "{answer}");
        }
        // an answer in its own words is the whole output, as before
        let c = fill(&head, json!({"head": "Cannot"}), &with).unwrap();
        assert_eq!(own_text(&head, c.clone(), "It cannot be mended here.\nWhy: the cut has no such file.\n", &with)["why"], with);
        // the owner's words: the words themselves
        assert_eq!(own_text(&head, c, "Cannot\nwhy: not today", &json!("Cannot\nwhy: not today"))["why"], json!("not today"));
    }

    /// The owner's typed reply to clarify (paper-research m1339, b1): the prompt has the step as the owner was shown it,
    /// WHY and the agents included, and no ticket id; a text field is the words themselves. FORM_PROMPT_OUT=FILE writes
    /// the prompt there, for reading.
    #[test]
    fn the_owners_reply_is_read_against_the_question_shown() {
        let base = format!("{}/../../agent-base/calls", env!("CARGO_MANIFEST_DIR"));
        let calls = crate::policy::call::Calls::load(&[base], &Default::default()).expect("agent-base/calls load");
        let shown = calls.render_step("clarify", &json!({"text": "hmm, I am not sure about this anymore", "why": "It is unclear what \"this\" refers to.", "agents": ["lead", "fixer"]})).unwrap();
        let sigs = docket_machine::syntax::parse_calls("clarify.call", include_str!("../../../agent-base/calls/clarify.call"), &mut Default::default()).unwrap();
        let head = sigs.into_iter().find(|c| c.name == "clarify").unwrap().head;
        let Type::Variant(cs) = &head else { panic!("clarify's head is a variant") };
        let cs: Vec<(String, Vec<(String, Type)>)> = cs.iter().map(|(c, fs)| (c.clone(), for_model(fs))).collect();
        let choices: Vec<String> = cs.iter().map(|(c, fs)| if fs.is_empty() { c.clone() } else { format!("{c} ({})", fs.iter().map(|(f, t)| format!("{f}: {t}")).collect::<Vec<_>>().join(", ")) }).collect();
        let p = prompt(&choices, "None", "for lead", Some(&shown), None);
        assert!(p.contains("Any reply that says more than whom it is for is this"), "the question says what each answer means: {p}");
        assert!(p.contains("The owner of a software team was asked") && p.contains("It is unclear what") && p.contains("- lead") && p.contains("- fixer") && p.contains("for lead"), "{p}");
        assert!(p.contains("Send (to: [Agent])") && p.contains("; Again"), "{p}");
        assert!(!p.contains("O-") && !p.contains("N-"), "no ticket id: {p}");
        assert_eq!(fill(&head, json!({"head": "Again"}), &json!("the median one")), Some(json!({"head": "Again", "text": "the median one"})));
        if let Ok(f) = std::env::var("FORM_PROMPT_OUT") {
            std::fs::write(f, &p).unwrap();
        }
    }

    /// An agent's answer is read against the step as it was shown it, and nothing in the prompt speaks of a first word
    /// (the owner, N-246.p1); without the step's text, the prompt says it could not be read.
    #[test]
    fn the_forming_prompt_has_the_step_as_the_agent_was_shown_it() {
        let choices = vec!["Assigned (picks: [{piece: Text, agent: Agent, why: Text}])".to_string(), "Ask".to_string()];
        let p = prompt(&choices, "None", "Assigned\nmark: mark-c8", None, Some("Assign each part of C-8 to an agent.\nThe parts: mark.\n"));
        assert!(p.starts_with("You turn an agent's answer to a step of its work into data."), "{p}");
        assert!(p.contains("The step, as the agent was shown it when it was given (the answer may name what the agent made since, such as a new agent or a commit):\n---\nAssign each part of C-8 to an agent.\nThe parts: mark.\n---"), "{p}");
        assert!(p.ends_with("The agent's answer:\n<<<\nAssigned\nmark: mark-c8\n>>>"), "{p}");
        assert!(!p.to_lowercase().contains("first word") && !p.contains("end its turn"), "{p}");
        let q = prompt(&choices, "None", "Assigned", None, None);
        assert!(q.contains("(The step's text could not be read: only its outcomes are known.)"), "{q}");
        let t = parts::marks_prompt(&choices, "None", &[], "Design\n## Asked\nx", None, Some("Design the change."), None);
        assert!(t.contains("as the agent was shown it when it was given") && t.contains("---\nDesign the change.\n---") && !t.to_lowercase().contains("first word"), "{t}");
    }

    /// An answer that is none of the outcomes is said so with why, one sentence (paper-research N-101 p554, 1); its name
    /// is never one of the step's own.
    #[test]
    fn none_of_the_outcomes_says_why() {
        let choices = vec!["Assigned (agent: Agent)".to_string(), "Ask".to_string()];
        let p = prompt(&choices, "None", "mark: mark-c8", None, Some("Assign C-8."));
        assert!(p.contains("Answer None only when the answer gives none of the outcomes, or gives two of them. Whether what it says is right or complete is not yours to judge. With `why`: one sentence saying which of the two it is."), "{p}");
        assert!(!p.contains("null"), "{p}");
        let none = |cs: &[&str]| none_of(&cs.iter().map(|c| (c.to_string(), Vec::new())).collect::<Vec<_>>());
        assert_eq!((none(&["Assigned", "Ask"]), none(&["None", "Some"])), ("None".to_string(), "NoneOfThem".to_string()));
    }

    /// C-8's answers formed as they are now (paper-research N-101 p518), by hand only: it calls the forming model
    /// through the coord's proxy. FORM_TRY=FILE, a JSON object {coord, base, project, out, cases: [{name, call, step,
    /// answer}]}, STEP and ANSWER places ({file, offset, len}) as the record has them; the heads as the run's lib.call
    /// and agent-base's assign have them. OUT gets each answer, the prompt sent and what came back.
    #[test]
    #[ignore]
    fn form_try() {
        let Ok(f) = std::env::var("FORM_TRY") else { return };
        let v: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let s = |k: &str| v[k].as_str().unwrap().to_string();
        let places = crate::policy::Places { base: s("base"), project: Some(s("project")), coord: s("coord"), ..Default::default() };
        let engine = Engine::start(places.clone());
        let placed = Type::Record(vec![("piece".into(), Type::Text), ("agent".into(), Type::Agent), ("why".into(), Type::Text)]);
        let ask = ("Ask".to_string(), vec![("question".to_string(), Type::Text)]);
        let part = Type::Record(vec![("name".into(), Type::Text), ("stands_on".into(), Type::List(Box::new(Type::Text))), ("after".into(), Type::Text), ("done".into(), Type::Text)]);
        let design: Vec<(String, Type)> = vec![("asked".into(), Type::Text), ("today".into(), Type::Text), ("parts".into(), Type::List(Box::new(part))), ("technique".into(), Type::Text), ("meaning".into(), Type::Text), ("deleted".into(), Type::Text)];
        let head = |call: &str| match call {
            // the run's calls.call (docket-core3's C-1, N-101 p601)
            "design" => Type::Variant(vec![("Design".into(), design.clone()), ask.clone()]),
            // moves-try's hop (docket-core5 N-101 p628, H-86 on docket-try2)
            "hop" => Type::Variant(vec![("Done".into(), vec![("note".into(), Type::Text)])]),
            // the minder's hand-in look (paper-research2's dry run on docket-try4, paper-research N-101 p662)
            "minder" => Type::Variant(vec![("Rest".into(), vec![]), ("Say".into(), vec![("text".into(), Type::Text)])]),
            "assign" => Type::Variant(vec![("Assigned".into(), vec![("agent".into(), Type::Agent), ("why".into(), Type::Text)]), ask.clone()]),
            _ => Type::Variant(vec![("Assigned".into(), vec![("picks".into(), Type::List(Box::new(placed.clone())))]), ask.clone()]),
        };
        let at = |p: &Value| crate::follow::text_at(p["file"].as_str().unwrap(), p["offset"].as_u64().unwrap(), p["len"].as_u64().unwrap()).unwrap_or_default();
        let mut out = String::new();
        for c in v["cases"].as_array().unwrap() {
            let h = head(c["call"].as_str().unwrap());
            let (step, answer) = (at(&c["step"]), at(&c["answer"]));
            // the answer's place as coordd passes it ({file, offset, len}); form wraps it as {"loc": ...} itself
            // what the step gives, with the docs the try's project has for the call
            let gives = crate::render::gives(&places, c["call"].as_str().unwrap(), &h, None);
            let got = form_by(&engine, "orchm", &h, &answer, &c["answer"], None, Some(&step), Some(&gives), &[], v["headings"].as_bool() != Some(false));
            let got = match got {
                Ok(r) => format!("{}\n(model: {}; why: {}; not taken: {})\n--- the replies\n{}", serde_json::to_string(&r.head).unwrap(), r.decision.and_then(|d| d.chosen).unwrap_or_default(), r.why.unwrap_or_default(), r.not_taken.unwrap_or_default(), r.calls.iter().map(|k| format!("{} on {}\n--- its prompt\n{}\n--- its reply\n{}", k.call, k.model.as_deref().unwrap_or("-"), k.prompt, serde_json::to_string_pretty(&k.reply).unwrap())).collect::<Vec<_>>().join("\n\n")),
                Err(e) => format!("fault: {e}"),
            };
            out += &format!("=== {} ({})\n--- the answer\n{answer}\n--- what came back\n{got}\n\n", c["name"].as_str().unwrap(), c["call"].as_str().unwrap());
        }
        std::fs::write(s("out"), out).unwrap();
    }
}
