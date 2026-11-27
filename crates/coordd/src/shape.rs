//! What the minder is shown when it fills a ticket's hole (paper-research m1162, 7; design-two-systems §9): the
//! ticket and its posts, their texts resolved (a model reads no Loc), what it owes, its program and the variables in
//! scope at the hole, the kinds it may apply and the calls a step may make, the team and who fills which role, and
//! its own last answer here if that was refused. Each part is one template variable of the call `shape`
//! the rendering agent-base/renderings/shape.tmpl (its holes are
//! not the call's parameters, so not a template of shape.call); this file makes them.
//!
//! And the minder itself, where nobody holds the role (ticket::run_task fills it with MINDER): coordd answers the
//! shape step with one stateless call, as it does the watcher's intake (`hand_minder`, `answer`). A Program that does
//! not check (`check_shape`) is asked again once with why; a second one goes to the machine, which refuses it and
//! keeps the refusal. Anything that fails is Rest.

use docket_machine::{CallSig, Defs, Delivery, TicketView, Type, ViewStatus};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The worker coordd answers the minder's step as, where no agent holds role `minder`.
pub const MINDER: &str = "minder";

/// What one shape step needs off the lock: the prompt's variables, and what its answer is checked against.
#[derive(Clone)]
pub struct Job {
    pub t: u64,
    pub step: u32,
    pub ticket: String,
    pub ctx: Value,
    pub kind: docket_machine::syntax::Kind,
    pub scope: BTreeMap<String, Type>,
    pub calls: BTreeMap<String, CallSig>,
    pub kinds: BTreeMap<String, docket_machine::syntax::Kind>,
    pub head: Type,
    pub places: crate::policy::Places,
}

impl crate::core::Core {
    /// Hand what is due to MINDER (the shape steps) to its job, which answers them (`Job::Minder`).
    pub fn hand_minder(&mut self) {
        let Some(jobs) = self.jobs.clone() else { return };
        let due = self.ticket_due(MINDER);
        if !due.is_empty() {
            let deliveries = self.ticket_handing(MINDER, due);
            let _ = jobs.send(crate::core::Job::Minder { deliveries });
        }
    }

    /// Delivery D of ticket V to MINDER, given: for a shape step, what the call needs; None for anything else (a
    /// withdrawal or a notice needs nothing).
    pub fn minder_job(&mut self, d: &Delivery, v: &TicketView) -> Option<Job> {
        self.ticket_given(MINDER, d);
        let docket_machine::What::Step { step, call, .. } = &d.what else { return None };
        if call != docket_machine::SHAPE {
            return None;
        }
        let defs = &self.tickets.m.defs;
        let kind = defs.kind_as(&v.kind, &v.program)?.into_owned();
        let head = head_for(defs, &kind)?;
        // each one's roles as the record says them (its own and the policy's: `Machine::roles_of`), else the registry's
        let m = &self.tickets.m;
        let mut team: Vec<(String, String)> = self.agents.iter().filter(|(_, a)| a.running()).map(|(n, a)| (n.clone(), Some(m.roles_of(n)).filter(|r| !r.is_empty()).map_or_else(|| a.role.clone(), |r| r.join(" and ")))).collect();
        team.sort();
        Some(Job {
            t: d.t,
            step: *step,
            ticket: d.ticket.clone(),
            ctx: context(v, defs, &team),
            kind,
            scope: v.hole.clone().unwrap_or_default(),
            calls: defs.calls.clone(),
            kinds: defs.kinds.clone(),
            head,
            places: self.places(),
        })
    }

    /// The minder's answer HEAD to JOB, with the DECISION that chose its model: the decision first, then the answer.
    pub fn minder_answered(&mut self, job: &Job, head: Value, decision: Option<coord_types::Decision>, failed: Option<String>) {
        if let Some(d) = &decision {
            self.ticket_decision(job.t, job.step, d);
        }
        // said on the ticket, not only in coordd's log, unless it found no model to run on (`own_call_failed`)
        self.own_call_failed(&job.ticket, "minder", decision.as_ref(), failed, |e| format!("the minder did not look: the call failed ({e})"));
        match self.ticket_answer(MINDER, job.t, head, docket_machine::Text::New(String::new())) {
            Ok(r) => eprintln!("coordd: tickets: minder on {}: {r}", job.ticket),
            Err(e) => eprintln!("coordd: tickets: minder on {}: {e}", job.ticket),
        }
    }
}

/// The shape call's head for a ticket of KIND: Close(result: Any) as KIND has it. A strict schema needs a type for
/// every field (the first run: "schema must have a 'type' key"), and the result is then held to the kind's results
/// before it is sent.
pub fn head_for(defs: &Defs, kind: &docket_machine::syntax::Kind) -> Option<Type> {
    Some(match defs.calls.get(docket_machine::SHAPE)?.head.clone() {
        Type::Variant(cs) => Type::Variant(cs.into_iter().map(|(c, fs)| (c, fs.into_iter().map(|(f, t)| (f, if t == Type::Any { kind.result.clone() } else { t })).collect())).collect()),
        other => other,
    })
}

/// The head that answers JOB's shape step, and the decision that chose the model (the last call's). Blocking: off
/// the lock. Each answer goes to coordd's log as it came, for reading (paper-research m1200).
pub fn answer(job: &Job) -> crate::clerk::Answered {
    let rest = json!({"head": "Rest"});
    let engine = crate::policy::Engine::start(job.places.clone());
    let mut ctx = job.ctx.clone();
    let mut decision = None;
    for attempt in 1..=2 {
        let prompt = match crate::render::template(&job.places, "shape", &ctx) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("coordd: tickets: minder on {}: no prompt ({e}); Rest", job.ticket);
                return (rest, decision, None);
            }
        };
        let check = |a: &Value| job.head.conforms(a);
        let got = match engine.call_prompt(docket_machine::SHAPE, docket_machine::SHAPE, &prompt, &job.head.schema(), MINDER, &check) {
            Ok(c) => {
                // why, in the one copy's words (policy `unanswered_words`), as route and the cleaning say it
                let why = crate::policy::unanswered_words(&c.decision);
                decision = Some(c.decision);
                let Some(a) = c.answer else {
                    eprintln!("coordd: tickets: minder on {}: the call failed ({why}); Rest", job.ticket);
                    return (rest, decision, Some(why));
                };
                a
            }
            Err(e) => {
                eprintln!("coordd: tickets: minder on {}: the call failed ({e}); Rest", job.ticket);
                return (rest, decision, Some(e.to_string()));
            }
        };
        eprintln!("coordd: tickets: minder on {}: answer {attempt}: {got}", job.ticket);
        if got["head"] == "Program" && attempt == 1 {
            let text = got["text"].as_str().unwrap_or_default();
            if let Err(why) = docket_machine::check::check_shape(&job.kind, &job.scope, text, &job.calls, &job.kinds) {
                eprintln!("coordd: tickets: minder on {}: refused, asked again: {why}", job.ticket);
                ctx["refused"] = json!(format!("{text}\nrefused: {why}"));
                continue;
            }
        }
        return (got, decision, None);
    }
    (rest, decision, None)
}

/// The most characters of posts shown in full; past it the oldest are cut to their first line (said in the text).
pub const POSTS_BUDGET: usize = 24_000;

/// Calls a minder does not use as steps (m1225 c): a question's (answer, reply), whose kind it opens instead, and the
/// owner's decide, which a decision asks. To ask an agent for something it writes an inline step.
const HIDDEN_CALLS: &[&str] = &["answer", "reply", "decide"];

/// Kinds a minder does not apply: the ways in (a thread, a filing, a note), the machine's own (a request) and base,
/// which every kind is made from.
const NOT_APPLIED: &[&str] = &["base", "thread", "filing", "note", "request"];

/// The shape call's variables for ticket V. TEAM: each agent and its role ("" for none), in order.
pub fn context(v: &TicketView, defs: &Defs, team: &[(String, String)]) -> Value {
    // the minder is shown no post an agent may not see: none held while a handler decides whom it is for, none
    // dropped (design §13; paper-research m1302, 3)
    let mut seen = v.clone();
    seen.posts.retain(|p| p.held.is_none() && !p.dropped);
    let v = &seen;
    json!({
        "ticket": ticket(v),
        "results": results(v, defs),
        "posts": posts(v, POSTS_BUDGET, team),
        "owner_spoke": owner_spoke(v),
        "awaiting_owner": awaiting_owner(v),
        "held": held(v),
        "program": format!("{}{}", program(&v.program), written(v)),
        "scope": scope(v),
        "kinds": kinds(defs),
        "calls": calls(defs),
        "team": team_lines(team),
        "refused": refused(v),
    })
}

fn ticket(v: &TicketView) -> String {
    let mut out = vec![format!("{} ({} kind), opened by {} at {}", v.id, v.kind, v.opener.as_deref().unwrap_or("?"), v.opened_at)];
    if let Some(o) = v.form.as_object() {
        for (k, x) in o {
            let x = crate::render::resolved(x);
            let shown = match &x {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.push(format!("{k}: {shown}"));
        }
    }
    out.join("\n")
}

/// Whether the owner wrote last (the machine's fact, TicketView.owner: one source, paper-research m1233).
fn owner_spoke(v: &TicketView) -> bool {
    v.owner.as_ref().is_some_and(|o| o.spoke_last)
}

/// The agents whose replies wait on the owner (the machine's fact), or null when none.
fn awaiting_owner(v: &TicketView) -> Value {
    match &v.owner {
        Some(o) if !o.unanswered.is_empty() => json!(o.unanswered.join(", ")),
        _ => Value::Null,
    }
}

/// Each post as `#N by WHO [ROLE] (AT):` and its text (TEAM gives the roles); over BUDGET the oldest are cut to their
/// first line, newest kept whole. Under a post whose asks are open, what it asks the owner, as the watcher marked it
/// (design-two-systems §16, 8): `  asks the owner: "..."`, and `    options: "...", "..."` when it offers some. A post
/// that answers a step a program of the minder's handed says so on its header, by the program's time.
fn posts(v: &TicketView, budget: usize, team: &[(String, String)]) -> String {
    if v.posts.is_empty() {
        return "(none)".into();
    }
    let asks: Vec<String> = v.posts.iter().map(|p| {
        if !p.asks_open {
            return String::new();
        }
        let text = match crate::render::resolved(&serde_json::to_value(&p.text).unwrap_or(Value::Null)) {
            Value::String(s) => s,
            _ => return String::new(),
        };
        let cut = |s: &[usize; 2]| text.get(s[0]..s[1]).map(|w| format!("\"{}\"", w.split_whitespace().collect::<Vec<_>>().join(" ")));
        p.asks.iter().filter_map(|m| {
            let ask = cut(&m.at)?;
            let options: Vec<String> = m.options.iter().filter_map(cut).collect();
            Some(if options.is_empty() { format!("\n  asks the owner: {ask}") } else { format!("\n  asks the owner: {ask}\n    options: {}", options.join(", ")) })
        }).collect()
    }).collect();
    let full: Vec<(String, String)> = v
        .posts
        .iter()
        .map(|p| {
            let text = match crate::render::resolved(&serde_json::to_value(&p.text).unwrap_or(Value::Null)) {
                Value::String(s) => s,
                other => other.to_string(),
            };
            let moved = p.moved.as_ref().map(|t| format!(" (moved to {t})")).unwrap_or_default();
            let role = match team.iter().find(|(n, _)| *n == p.by) {
                Some((_, r)) if !r.is_empty() => format!(" [{r}]"),
                _ if p.by == "owner" => " [owner]".to_string(),
                _ => String::new(),
            };
            // an answer to a step a program of the minder's handed (one not undone): which, so it does not tell
            // the agent the same again after a Cannot (paper-research m1464, 2)
            let mine = p.step.and_then(|s| {
                let given = v.given.iter().find(|g| g.delivery == format!("{}.s{s}", v.id))?;
                v.shaped.iter().rev().find(|x| !x.undone && x.refused.is_none() && x.answer["head"] == "Program" && x.at <= given.at)
            });
            let note = mine.map(|x| format!(", its answer to the step your program of {} gave it", hm(&x.at))).unwrap_or_default();
            (format!("#{} by {}{role} ({}){moved}{note}:", p.n, p.by, p.at), text.trim().to_string())
        })
        .collect();
    // keep the newest whole while the budget lasts; the rest, oldest first, to their first line
    let mut keep = vec![false; full.len()];
    let mut used = 0;
    for i in (0..full.len()).rev() {
        let n = full[i].0.len() + full[i].1.len();
        if used + n > budget && used > 0 {
            break;
        }
        used += n;
        keep[i] = true;
    }
    let cut = keep.iter().filter(|k| !**k).count();
    let mut out = Vec::new();
    if cut > 0 {
        out.push(format!("(the {cut} oldest posts are cut to their first line, to keep this short)"));
    }
    for (((head, text), whole), asks) in full.iter().zip(&keep).zip(&asks) {
        let body = if *whole { text.clone() } else { format!("{} …", text.lines().next().unwrap_or("")) };
        out.push(format!("{head}\n{body}{asks}"));
    }
    out.join("\n\n")
}

/// Who holds what on V now: each agent given its posts, up to which, and whether it has answered since (a post of
/// its own after them); then the steps out. "nobody" when nothing is with anyone.
fn held(v: &TicketView) -> String {
    let mut out: Vec<String> = v
        .given
        .iter()
        .map(|g| {
            let answer = v.posts.iter().find(|p| p.by == g.to && p.n > g.upto).map_or("has not answered yet".to_string(), |p| format!("answered in #{}", p.n));
            format!("{} was given the posts up to #{} at {}; {answer}", g.to, g.upto, g.at)
        })
        .collect();
    out.extend(v.waits_on.iter().map(|w| match w.post {
        Some(n) if w.call == "asks" => format!("what #{n} asks is the owner's to answer"),
        _ => format!("{} owes {} on {}", w.agent, w.call, w.ticket),
    }));
    if let ViewStatus::Awaits { agent, call, .. } = &v.status {
        out.push(format!("{agent} owes {call} on {}", v.id));
    }
    if out.is_empty() { "nobody holds anything of it".into() } else { out.join("\n") }
}

/// The ticket's own program without its comments, its one hole marked (a program with two shows them unmarked).
/// HH:MM of AT (UTC, as the posts' times are).
fn hm(at: &str) -> &str {
    at.get(11..16).unwrap_or(at)
}

/// The programs the minder wrote at the hole, oldest first, each under its time, as "how it runs" shows them to the
/// owner (paper-research m1466, 2): `// written by the minder at the hole, HH:MM`, and `, undone by the owner` or
/// `, refused by the check`; the last that ran gets `<- here` at its one hole. Empty when there are none, so the
/// prompt is then what it was.
pub fn written(v: &TicketView) -> String {
    let ran = v.shaped.iter().rposition(|s| !s.undone && s.refused.is_none() && s.answer.get("text").is_some());
    v.shaped
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let t = s.answer.get("text")?.as_str()?;
            let note = if s.undone { ", undone by the owner" } else if s.refused.is_some() { ", refused by the check" } else { "" };
            let shown = if ran == Some(i) { program(t) } else { t.trim_end().to_string() };
            Some(format!("\n\n// written by the minder at the hole, {}{note}\n{shown}", hm(&s.at)))
        })
        .collect()
}

fn program(text: &str) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with("//")).map(|l| l.split(" //").next().unwrap_or(l).trim_end()).filter(|l| !l.is_empty()).collect();
    let holes = lines.iter().filter(|l| l.trim() == "...").count();
    lines.iter().map(|l| if holes == 1 && l.trim() == "..." { format!("{l}   <- here") } else { l.to_string() }).collect::<Vec<_>>().join("\n")
}

/// V's kind's results, one a line, each with its doc, from the text V runs.
fn results(v: &TicketView, defs: &Defs) -> String {
    let Some(k) = defs.kind_as(&v.kind, &v.program) else { return String::new() };
    docket_machine::doc::results(&k)
}

/// The names at the hole, then its posts by number (`post N`, a Text that is post #N's words, passed on by reference:
/// a step given it gets the words, so they need not be copied into a program).
fn scope(v: &TicketView) -> String {
    let mut out: Vec<String> = v.hole.iter().flatten().map(|(n, t)| format!("{n}: {t}")).collect();
    out.extend(v.posts.iter().map(|p| format!("post {}: Text, the words of post #{} (by {})", p.n, p.n, p.by)));
    if out.is_empty() { "(nothing)".into() } else { out.join("\n") }
}

/// Each kind a minder may open, as its signature, what it raises, and what it is for: `fix(what: Text, ...) -> Ref`.
fn kinds(defs: &Defs) -> String {
    defs.kinds
        .values()
        .filter(|k| !NOT_APPLIED.contains(&k.name.as_str()))
        .map(docket_machine::doc::kind_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The calls a step may make: those the kinds give to a role, the owner or a fresh agent (not the watcher's or the
/// clerk's, nor the runner's, whose check is opened as a kind), each as its signature.
fn calls(defs: &Defs) -> String {
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for k in defs.kinds.values() {
        let words: Vec<&str> = k.text.split(|c: char| c.is_whitespace() || c == '(' || c == '{' || c == ',').filter(|w| !w.is_empty()).collect();
        for (i, w) in words.iter().enumerate() {
            if *w != "step" && *w != "hold" {
                continue;
            }
            let call = match words.get(i + 1) {
                Some(&"role") => words.get(i + 3),
                Some(&"owner") | Some(&"fresh") => words.get(i + 2),
                _ => None,
            };
            if let Some(c) = call {
                used.insert(c.to_string());
            }
        }
    }
    defs.calls
        .values()
        .filter(|c| c.name != "shape" && !HIDDEN_CALLS.contains(&c.name.as_str()) && used.contains(&c.name))
        .map(docket_machine::doc::call_sig)
        .collect::<Vec<_>>()
        .join("\n")
}

fn team_lines(team: &[(String, String)]) -> String {
    if team.is_empty() {
        return "(nobody)".into();
    }
    team.iter().map(|(n, r)| if r.is_empty() { n.clone() } else { format!("{n}: role {r}") }).collect::<Vec<_>>().join("\n")
}

/// The last answer here, if it was refused: what it was and why, so the next one does not repeat it.
/// Null when it was not (a template's `if` takes an empty text as true).
fn refused(v: &TicketView) -> Value {
    match v.shaped.last() {
        Some(s) if s.refused.is_some() => json!(format!("{}\nrefused: {}", s.answer, s.refused.as_deref().unwrap_or(""))),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(posts: Value) -> TicketView {
        serde_json::from_value(json!({"id": "N-2", "n": 2, "kind": "thread", "opener": "owner", "ctx": {}, "title": "t",
            "form": {"to": "lead", "names": [], "text": "I want to design this: add median(xs)", "addressed": "Unnamed"},
            "status": {"is": "resting"}, "children": [], "program": "kind thread\n  ...", "bound": 3, "spent": 1,
            "opened_at": "2026-11-11T19:16:00Z", "waits_on": [], "given": [], "posts": posts,
            "hole": {"to": "Agent", "text": "Text"}, "owner": {"spoke_last": false, "unanswered": ["lead"]}}))
        .unwrap()
    }

    #[test]
    fn the_minder_sees_the_ticket_its_posts_its_scope_and_the_team() {
        let v = view(json!([{"n": 1, "at": "2026-11-11T19:16:00Z", "by": "owner", "text": "I want to design this", "names": ["lead"]},
                            {"n": 2, "at": "2026-11-11T19:16:30Z", "by": "lead", "text": "Design: add median to calc.py"}]));
        let c = context(&v, &Defs::default(), &[("lead".into(), "lead".into()), ("fixer".into(), "fixer".into()), ("runner".into(), String::new())]);
        let t = c["ticket"].as_str().unwrap();
        assert!(t.starts_with("N-2 (thread kind), opened by owner"), "{t}");
        assert!(t.contains("to: lead"), "{t}");
        let p = c["posts"].as_str().unwrap();
        assert!(p.contains("#1 by owner [owner] (2026-11-11T19:16:00Z):\nI want to design this") && p.contains("#2 by lead [lead]"), "{p}");
        assert_eq!(c["owner_spoke"], false);
        assert_eq!(c["awaiting_owner"], "lead");
        assert_eq!(c["scope"], "text: Text\nto: Agent\npost 1: Text, the words of post #1 (by owner)\npost 2: Text, the words of post #2 (by lead)");
        assert_eq!(c["held"], "nobody holds anything of it");
        assert_eq!(c["team"], "lead: role lead\nfixer: role fixer\nrunner");
        assert_eq!(c["refused"], Value::Null);
    }

    #[test]
    fn a_long_ticket_keeps_its_newest_posts_whole_and_says_the_rest_are_cut() {
        let long = "x".repeat(400);
        let v = view(json!([{"n": 1, "at": "a", "by": "owner", "text": format!("first line\n{long}")},
                            {"n": 2, "at": "b", "by": "lead", "text": format!("second\n{long}")},
                            {"n": 3, "at": "c", "by": "owner", "text": "the newest"}]));
        let p = posts(&v, 300, &[]);
        assert!(p.starts_with("(the 2 oldest posts are cut to their first line"), "{p}");
        assert!(p.contains("#1 by owner [owner] (a):\nfirst line …") && p.contains("#3 by owner [owner] (c):\nthe newest"), "{p}");
        assert_eq!(owner_spoke(&v), false);
        assert_eq!(awaiting_owner(&v), "lead");
        assert!(!p.contains(&long));
    }

    #[test]
    fn under_a_post_with_open_asks_the_minder_sees_what_it_asks_and_the_options() {
        let text = "Done.\nShould it raise ValueError or return None?\nAlso: keep the  old\n name?";
        let ask = text.find("Should").unwrap();
        let o1 = text.find("raise ValueError").unwrap();
        let o2 = text.find("return None").unwrap();
        let k = text.find("keep").unwrap();
        let mut v = view(json!([{"n": 1, "at": "a", "by": "lead", "text": text, "asks_open": true,
            "asks": [{"at": [ask, ask + 42], "options": [[o1, o1 + 16], [o2, o2 + 11]]}, {"at": [k, text.len()]}]},
                                {"n": 2, "at": "b", "by": "lead", "text": text, "asks": [{"at": [ask, ask + 42]}]}]));
        v.waits_on = serde_json::from_value(json!([{"ticket": "N-2", "agent": "owner", "call": "asks", "post": 1}])).unwrap();
        let p = posts(&v, 10_000, &[]);
        assert!(p.contains("name?\n  asks the owner: \"Should it raise ValueError or return None?\"\n    options: \"raise ValueError\", \"return None\"\n  asks the owner: \"keep the old name?\"\n\n#2"), "{p}");
        assert_eq!(p.matches("asks the owner").count(), 2, "{p}");
        assert_eq!(held(&v), "what #1 asks is the owner's to answer");
    }
}

/// The examples in agent-base/renderings/shape.tmpl are programs the machine takes at a thread's hole: the minder
/// copies their form, so a wrong one would be copied (the indented lines after `(a)` and `(b)`, each up to a blank).
#[cfg(test)]
mod examples {
    #[test]
    fn the_prompts_examples_pass_the_check_at_a_threads_hole() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-shape-ex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = crate::core::Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let defs = &core.tickets.m.defs;
        let k = &defs.kinds["thread"];
        let scope = k.form.iter().cloned().collect();
        let tmpl = std::fs::read_to_string(format!("{base}/renderings/shape.tmpl")).unwrap();
        let mut found = 0;
        for head in ["(a)", "(b)", "(c)"] {
            let at = tmpl.lines().position(|l| l.starts_with(head)).unwrap_or_else(|| panic!("no example {head}"));
            let text: String = tmpl.lines().skip(at + 1).take_while(|l| !l.trim().is_empty()).map(|l| format!("{}\n", l.strip_prefix("    ").unwrap_or(l))).collect();
            docket_machine::check::check_shape(k, &scope, &text, &defs.calls, &defs.kinds).unwrap_or_else(|e| panic!("example {head}: {e}\n{text}"));
            found += 1;
        }
        assert_eq!(found, 3);
        // the call's schema has a type at every place (strict structured output refuses one without)
        let schema = super::head_for(defs, k).unwrap().schema();
        fn typed(v: &serde_json::Value) -> bool {
            match v {
                serde_json::Value::Object(o) if o.contains_key("properties") || o.contains_key("anyOf") || o.contains_key("items") => o.values().all(typed),
                serde_json::Value::Object(o) if o.keys().all(|k| k != "type" && k != "enum" && k != "const") => o.values().all(|x| x.is_object() && typed(x)),
                serde_json::Value::Object(o) => o.values().all(typed),
                serde_json::Value::Array(xs) => xs.iter().all(typed),
                _ => true,
            }
        }
        assert!(typed(&schema) && !schema.to_string().contains("{}"), "{schema}");
        let _ = std::fs::remove_dir_all(&coord);
    }
}

/// Renders the shape prompt for a median thread with the real kinds and calls, to /tmp/shape-prompt.txt, for a person
/// to read (paper-research m1162: the rendered prompt before it is tuned). `cargo test --release -p coordd -- --ignored
/// shape_prompt_for_reading`.
#[cfg(test)]
mod reading {
    use super::*;

    /// The watcher's prompt for the run's N-2 (paper-research m1225 f): the opinion question that was placed on the
    /// median thread N-1, which was then resting with only its first post. To /tmp/intake-prompt.txt.
    #[test]
    #[ignore]
    fn intake_prompt_for_reading() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "title": "I want to design this: add median(xs) to calc, with a test. Give me a short design first.",
            "form": {"to": "lead", "names": [], "text": "I want to design this: add median(xs) to calc, with a test. Give me a short design first.", "addressed": "Unnamed"},
            "status": {"is": "resting"}, "children": [], "bound": 3, "spent": 1, "opened_at": "2026-11-11T20:41:46Z", "waits_on": [], "given": [], "program": "",
            "posts": [{"n": 1, "at": "2026-11-11T20:41:46Z", "by": "owner", "text": "I want to design this: add median(xs) to calc, with a test. Give me a short design first.", "names": ["lead"]}]}))
        .unwrap();
        let open = crate::clerk::open_lines_of(&[v], "lead");
        let calls = crate::policy::call::Calls::load(&[format!("{base}/calls")], &Default::default()).unwrap();
        let text = "@lead what do you think of the name calc.py for this module, is it fine or should it be stats.py? just your opinion.";
        let prompt = calls.render_step("sort", &json!({"text": text, "to": "lead", "open": open})).unwrap();
        std::fs::write("/tmp/intake-prompt.txt", prompt).unwrap();
    }

    /// The minder's words on §16's three cases (paper-research m1387, 2), by `Router::call`: picture 3 A (the owner
    /// already said "no need to check with me first", lead asks "Shall I go ahead?") wants a Program with a step to lead;
    /// picture 3 B (lead asks raise or return None, nothing in the thread answers it) Rest; "what is 5+5" answered "10"
    /// Close; D (N-5 after lead's Cannot to the minder's step) Rest. MINDER_MODEL (default gpt-5.6-luna), MINDER_RUNS (3),
    /// MINDER_CASES the cases run (default ABCD),
    /// MINDER_COORD the coord folder whose proxy key is used; each prompt to /tmp/minder-case-X.txt.
    #[test]
    #[ignore]
    fn minder_on_asks_cases() {
        let Ok(coord_dir) = std::env::var("MINDER_COORD") else { return };
        let model = std::env::var("MINDER_MODEL").unwrap_or_else(|_| "gpt-5.6-luna".into());
        let runs: usize = std::env::var("MINDER_RUNS").ok().and_then(|n| n.parse().ok()).unwrap_or(3);
        let only = std::env::var("MINDER_CASES").unwrap_or_else(|_| "ABCD".into());
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let router = crate::accounts::router::Router::of(&crate::policy::Places { base: base.clone(), coord: coord_dir.clone(), ..Default::default() });
        let coord = std::env::temp_dir().join(format!("docket-minder-cases-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = crate::core::Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let defs = &core.tickets.m.defs;
        let kind = defs.kinds["thread"].clone();
        let scope: std::collections::BTreeMap<String, docket_machine::Type> = kind.form.iter().cloned().collect();
        let head = head_for(defs, &kind).unwrap();
        let a3 = "Confirmed: `mean([])` divides by `len(xs)`, which is 0 for an empty list. Plan: raise `ValueError(\"mean of an empty list\")` in `calc.py` and add a test for it in `test_calc.py`. Shall I go ahead?";
        let b3 = "Confirmed: `mean([])` divides by `len(xs)`, which is 0 for an empty list. There are two ways to fix it, and callers will see the difference. Should it raise ValueError or return None?";
        let d_owner = "@lead add range(xs) to calc.py, returning max minus min, ValueError on an empty list, with a test. No need to check with me first.";
        let d_plan = r#"Plan:
1. Add `range(xs)` to `calc.py`. It returns `max(xs) - min(xs)` and raises `ValueError` on an empty list. Naming it `range` hides Python's built-in `range` inside `calc.py`, and in any file that does `from calc import range`. Nothing in `calc.py` uses the built-in today, so it works, but tell me if you'd rather it be called `spread`.
2. Add tests to `test_calc.py` for an unsorted list, a one-element list (which gives 0), and an empty list. Then run `./check.sh` and commit with `Item: N-5`.

Shall I go ahead?"#;
        let d_cannot = r#"Cannot: the owner told me in note N-4 to post my plan for `range(xs)`, then wait for their own answer before editing anything. This step is from coord, not the owner, and it only repeats the thread's original "no need to check with me first". It isn't the owner answering "Shall I go ahead?". So `calc.py` and `test_calc.py` are untouched. Once the owner replies on N-5, I'll add `range(xs)` (max minus min, `ValueError` on an empty list) with its tests, run `./check.sh`, and commit with `Item: N-5`."#;
        let d_program = r#"r <- step role lead "The owner requested `range(xs)`, not `spread`, and authorized proceeding without a check: 'add range(xs) to calc.py, returning max minus min, ValueError on an empty list, with a test. No need to check with me first.'" -> Done | Cannot(why: Text);
..."#;
        let cases = [
            ("A", "@lead bug: mean([]) crashes with ZeroDivisionError; it should raise ValueError with a clear message. Fix it and add a test, no need to check with me first.", a3, json!([{"ask": "Shall I go ahead?", "options": []}])),
            ("B", "@lead bug: mean([]) crashes with ZeroDivisionError", b3, json!([{"ask": "Should it raise ValueError or return None?", "options": ["raise ValueError", "return None"]}])),
            ("C", "what is 5+5", "10", json!([])),
            // N-5 of the scratch run after lead's Cannot (paper-research m1464, 2): the minder told lead the owner's
            // words, lead answered Cannot; wanted Rest, not the same again
            ("D", d_owner, d_plan, json!([{"ask": r#"1. Add `range(xs)` to `calc.py`. It returns `max(xs) - min(xs)` and raises `ValueError` on an empty list. Naming it `range` hides Python's built-in `range` inside `calc.py`, and in any file that does `from calc import range`. Nothing in `calc.py` uses the built-in today, so it works, but tell me if you'd rather it be called `spread`."#, "options": ["spread"]}, {"ask": r#"2. Add tests to `test_calc.py` for an unsorted list, a one-element list (which gives 0), and an empty list. Then run `./check.sh` and commit with `Item: N-5`."#, "options": []}, {"ask": "Shall I go ahead?", "options": []}])),
        ];
        for (name, owner, reply, items) in cases.into_iter().filter(|c| only.contains(c.0)) {
            let (marks, _) = docket_machine::asks::spans(reply, &items);
            let mut v: TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "title": owner,
                "form": {"to": "lead", "typed": ["lead"], "names": if owner.starts_with('@') { json!(["lead"]) } else { json!([]) }, "text": owner, "addressed": if owner.starts_with('@') { "Named" } else { "Unnamed" }},
                "status": {"is": "resting"}, "children": [], "bound": 3, "spent": 1, "opened_at": "2026-11-12T11:02:00Z", "waits_on": [], "given": [], "program": "",
                "posts": [{"n": 1, "at": "2026-11-12T11:02:00Z", "by": "owner", "text": owner, "names": ["lead"]},
                          {"n": 2, "at": "2026-11-12T11:03:00Z", "by": "lead", "text": reply, "told": ["owner"], "auto": true, "asks": marks, "asks_open": !marks.is_empty()}]}))
            .unwrap();
            v.program = kind.text.clone();
            v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-1.p1", "upto": 1, "at": "2026-11-12T11:02:01Z"}])).unwrap();
            v.hole = Some(scope.clone());
            if name == "D" {
                v.id = "N-5".into();
                v.n = 5;
                v.spent = 3;
                v.bound = 5;
                v.posts[1].asks_open = false;
                v.posts.push(serde_json::from_value(json!({"n": 3, "at": "2026-11-12T11:04:00Z", "by": "lead", "text": d_cannot, "told": ["owner"], "auto": true, "step": 3})).unwrap());
                v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "N-5.p1", "upto": 1, "at": "2026-11-12T11:02:01Z"},
                    {"to": "minder", "delivery": "N-5.s2", "upto": 2, "at": "2026-11-12T11:03:10Z"},
                    {"to": "lead", "delivery": "N-5.s3", "upto": 2, "at": "2026-11-12T11:03:20Z"},
                    {"to": "minder", "delivery": "N-5.s4", "upto": 3, "at": "2026-11-12T11:04:05Z"}])).unwrap();
                v.shaped = serde_json::from_value(json!([{"n": 1, "at": "2026-11-12T11:03:20Z", "answer": {"head": "Program", "text": d_program}, "undone": false}])).unwrap();
            }
            v.owner = Some(docket_machine::view::ViewOwner::of(&v.posts));
            let ctx0 = context(&v, defs, &[("lead".into(), "lead".into()), ("fixer".into(), "fixer".into()), ("runner".into(), String::new())]);
            std::fs::write(format!("/tmp/minder-case-{name}.txt"), crate::render::template(&core.places(), "shape", &ctx0).unwrap()).unwrap();
            for i in 1..=runs {
                // as `answer` does: a Program the check refuses is asked again once, with why
                let mut ctx = ctx0.clone();
                for attempt in 1..=2 {
                    let prompt = crate::render::template(&core.places(), "shape", &ctx).unwrap();
                    let t0 = std::time::Instant::now();
                    let got = router.call("shape", &prompt, &head.schema(), &model, &|a| head.conforms(a)).0;
                    let refused = match &got {
                        Ok(a) if a["head"] == "Program" => docket_machine::check::check_shape(&kind, &scope, a["text"].as_str().unwrap_or_default(), &defs.calls, &defs.kinds).err(),
                        _ => None,
                    };
                    let checked = match (&got, &refused) {
                        (Ok(a), None) if a["head"] == "Program" => " (checks)".to_string(),
                        (_, Some(e)) => format!(" (refused: {e})"),
                        _ => String::new(),
                    };
                    println!("=== case {name} run {i} answer {attempt} ({model}) {:.1} s{checked}\n{}", t0.elapsed().as_secs_f64(), match &got { Ok(a) => a.to_string(), Err(e) => format!("failed: {e:?}") });
                    match (got, refused) {
                        (Ok(a), Some(why)) if attempt == 1 => ctx["refused"] = json!(format!("{}\nrefused: {why}", a["text"].as_str().unwrap_or_default())),
                        _ => break,
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    #[ignore]
    fn shape_prompt_for_reading() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let coord = std::env::temp_dir().join(format!("docket-shape-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let mut core = crate::core::Core::load(&coord.display().to_string(), &base);
        core.load_tickets();
        let v: TicketView = serde_json::from_value(json!({"id": "N-2", "n": 2, "kind": "thread", "opener": "owner", "ctx": {}, "title": "t",
            "form": {"to": "lead", "names": [], "text": "I want to design this: add median(xs) to calc, with a test. Give me a short design first.", "addressed": "Unnamed"},
            "status": {"is": "resting"}, "children": [], "bound": 3, "spent": 1, "opened_at": "2026-11-11T19:16:56Z", "waits_on": [], "given": [],
            "program": "",
            "posts": [{"n": 1, "at": "2026-11-11T19:16:56Z", "by": "owner", "text": "I want to design this: add median(xs) to calc, with a test. Give me a short design first.", "names": ["lead"]},
                      {"n": 2, "at": "2026-11-11T19:17:20Z", "by": "lead", "text": "Design: add median(xs) to calc.py after mean; odd length the middle element, even length the mean of the two middle ones; empty raises ValueError. test_calc.py gets asserts for odd, even, single, unsorted input not mutated, empty raises. Checked by ./check.sh printing ok. I have not edited any files yet. Say go and I will make the change."}]}))
        .unwrap();
        let mut v = v;
        v.program = core.tickets.m.defs.kinds["thread"].text.clone();
        v.given = serde_json::from_value(json!([{"to": "lead", "delivery": "d-3", "upto": 1, "at": "2026-11-11T19:16:57Z"}])).unwrap();
        v.hole = Some(core.tickets.m.defs.kinds["thread"].form.iter().cloned().collect());
        let ctx = context(&v, &core.tickets.m.defs, &[("lead".into(), "lead".into()), ("fixer".into(), "fixer".into()), ("runner".into(), String::new())]);
        let prompt = crate::render::template(&core.places(), "shape", &ctx).unwrap();
        std::fs::write("/tmp/shape-prompt.txt", &prompt).unwrap();
        let _ = std::fs::remove_dir_all(&coord);
    }
}
