//! The model classes (paper-research N-101; models.rs): the system's order and classes read from the policy, a call's
//! `where` held when a step is given, when coordd makes an agent (`model-for` among the models that meet it; a copy
//! keeps its model), on `coord new` / `coord fork --model`, and at load (no model said: the kind is refused).

use crate::core::{Core, Who};
use crate::models::{self, Asked, Order};
use docket_machine::{Input, Status};
use serde_json::json;
use std::io::{Read, Write};

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

/// What the stand-in proxy serves: three Claude models and a Codex one.
const SERVED: &[(&str, &str)] = &[("claude-haiku-5-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("claude-opus-5-5", "anthropic"), ("gpt-6-luna", "openai")];

fn families() -> Vec<(String, String)> {
    SERVED.iter().map(|(m, p)| (m.to_string(), if *p == "anthropic" { "claude" } else { "codex" }.to_string())).collect()
}

/// A stand-in proxy serving SERVED, for the engines started on this thread (never the real one).
fn proxy() {
    proxy_serving(SERVED)
}

/// A stand-in proxy serving MODELS (id, owner).
fn proxy_serving(models: &'static [(&'static str, &'static str)]) {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut s in l.incoming().flatten() {
            let mut buf = [0u8; 65536];
            let _ = s.read(&mut buf);
            let data: Vec<_> = models.iter().map(|(m, p)| json!({"id": m, "owned_by": p})).collect();
            let body = json!({"data": data}).to_string();
            let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        }
    });
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = Some(url));
}

/// What the stand-in gateway does with a chat completion for a model ([`gateway`]).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gate {
    /// Answers it, with the answer "ok".
    Answer,
    /// Refuses it at its limit: the gateway's 429 to every call coordd made from 20:09Z on 20 Nov, `model_cooldown`
    /// after `usage_limit_reached`, as the record's attempts hold it (paper-research N-1397 p117).
    AtLimit,
    /// Refuses it cooling down after a transient error, not a limit: the gateway's 30 s cooldown (consult-3 N-1397
    /// p125), whose 429 says `model_cooldown` with the error before it.
    Cooling,
    /// Does not serve it: a 404 that is no limit.
    NotServed,
}

/// The stand-in gateway's gate per model (a model it does not name is answered), changed while it runs.
type Gates = std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<&'static str, Gate>>>;

/// A stand-in proxy serving MODELS (id, owner) that answers each chat completion as its model's gate says (clean.rs's
/// stand-in), and the requests it was asked. Made up: no model is called.
fn gateway(models: &'static [(&'static str, &'static str)]) -> (Gates, std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>) {
    let gates = Gates::default();
    let g = gates.clone();
    let seen = crate::clean::tests::stand_in_serving(models, move |req| {
        let model = req["model"].as_str().unwrap_or_default();
        match g.lock().unwrap().get(model).copied().unwrap_or(Gate::Answer) {
            Gate::Answer => ("200 OK", json!({"choices": [{"finish_reason": "stop", "message": {"content": json!({"answer": "ok"}).to_string()}}]})),
            Gate::AtLimit => ("429 Too Many Requests", json!({"error": {"code": "model_cooldown", "last_upstream_error": "usage_limit_reached: The usage limit has been reached", "message": format!("All credentials for model {model} are cooling down (last error: usage_limit_reached: The usage limit has been reached)"), "model": model, "reset_seconds": 683, "reset_time": "11m23s"}})),
            Gate::Cooling => ("429 Too Many Requests", json!({"error": {"code": "model_cooldown", "last_upstream_error": "503 Service Unavailable: upstream connect error", "message": format!("All credentials for model {model} are cooling down (last error: 503 Service Unavailable: upstream connect error)"), "model": model, "reset_seconds": 30, "reset_time": "30s"}})),
            Gate::NotServed => ("404 Not Found", json!({"error": {"code": "model_not_found", "message": format!("unknown provider for model {model}")}})),
        }
    });
    (gates, seen)
}

const CALLS: &str = "\
call design(words: Text) where model >= medium -> Designed(how: Text)
  template
    Design {{ words }}.

call decomp(words: Text) where model.class == small and model.family == claude -> Parts(parts: [Text])
  template
    Split {{ words }}.

call plain(words: Text) -> Done
  template
    Do {{ words }}.
";

const KINDS: &str = "\
kind change
  letter C
  form words: Text, who: Agent
  result Designed(how: Text)
  pattern
    step who design(words)

kind fresh_design
  letter F
  form words: Text
  result Designed(how: Text)
  pattern
    step fresh design(words)

kind fresh_plain
  letter P
  form words: Text
  result Done
  pattern
    step fresh plain(words)

kind let_plain
  letter L
  form words: Text
  result Done
  pattern
    let agent a in { step a plain(words) }

kind fork_plain
  letter K
  form words: Text, from: Agent
  result Done
  pattern
    step fork from plain(words)
";

struct Coord {
    core: Core,
    dir: std::path::PathBuf,
}

impl Drop for Coord {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Coord {
    /// A coord whose project has CALLS and KINDS, and POLICY (a file of its policy) if any; ann runs haiku, bob
    /// sonnet, orch opus.
    fn new(tag: &str, policy: Option<&str>) -> Coord {
        let dir = std::env::temp_dir().join(format!("docket-models-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("files");
        for d in ["kinds", "calls", "policy"] {
            std::fs::create_dir_all(project.join(d)).unwrap();
        }
        let add = |n: &str, i: u32, m: &str| format!("{{\"ev\":\"add\",\"id\":\"AGT-00{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"{m}\",\"at\":\"x\"}}\n");
        std::fs::write(dir.join("registry.jsonl"), format!("{}{}{}", add("ann", 1, "claude-haiku-5-5"), add("bob", 2, "claude-sonnet-5-5"), add("orch", 3, "claude-opus-5-5"))).unwrap();
        std::fs::write(project.join("calls/m.call"), CALLS).unwrap();
        std::fs::write(project.join("kinds/m.kind"), KINDS).unwrap();
        if let Some(p) = policy {
            std::fs::write(project.join("policy/models.eflint"), p).unwrap();
        }
        std::fs::write(project.join("project.toml"), "[project]\nname = \"m\"\n").unwrap();
        std::fs::write(dir.join("project"), project.join("project.toml").display().to_string()).unwrap();
        let mut core = Core::load(&dir.display().to_string(), &base());
        core.project = crate::project::Project::load(&dir.display().to_string()).unwrap();
        core.set_models(families());
        core.load_tickets();
        Coord { core, dir }
    }

    fn open(&mut self, kind: &str, form: serde_json::Value) -> u64 {
        let r = self.core.ticket_input(Input::Open { kind: kind.into(), under: None, opener: "owner".into(), form, text: None, names: vec![], told: vec![] });
        assert!(r.is_ok(), "{r:?}");
        *self.core.tickets.m.tickets.keys().filter(|t| **t < docket_machine::HIDDEN).max().unwrap()
    }
}

/// The system's policy: one order across families, three classes; a `where` held to it.
#[test]
fn a_where_is_held_to_the_order_and_classes_of_the_policy() {
    let places = crate::policy::Places { base: base(), ..Default::default() };
    let o = Order::read(&places, &families()).unwrap();
    assert_eq!((o.tier["claude-haiku-5-5"], o.tier["claude-sonnet-5-5"], o.tier["claude-opus-5-5"]), (1, 2, 3));
    assert_eq!((o.class["small"], o.class["medium"], o.class["large"]), (1, 2, 3));
    let w = |c: &str| docket_machine::syntax::model_cond(c, 1).unwrap();
    for (c, m, want) in [
        ("model >= medium", "claude-haiku-5-5", false),
        ("model >= medium", "claude-sonnet-5-5", true),
        ("model >= medium", "claude-opus-5-5", true),
        ("model > medium", "claude-sonnet-5-5", false),
        ("model.class == small and model.family == claude", "claude-haiku-5-5", true),
        ("model.class == small and model.family == claude", "gpt-6-luna", false),
        ("model.class == small", "gpt-6-luna", true),
        ("model == claude-opus-5-5", "claude-opus-5-5", true),
        ("model == \"claude-opus-5-5\"", "claude-sonnet-5-5", false),
        ("model.family != codex", "claude-haiku-5-5", true),
        ("model <= 2", "claude-sonnet-5-5", true),
    ] {
        assert_eq!(o.meets(&w(c), m), Ok(want), "{c} of {m}");
    }
    // a model with no number cannot be told, said so
    let e = o.meets(&w("model >= medium"), "claude-mystery").unwrap_err();
    assert!(e.contains("claude-mystery has no number in the policy"), "{e}");
    // a misspelt class is found before any model is asked
    let e = o.check(&w("model >= mediun")).unwrap_err();
    assert!(e.contains("`mediun` is no class (large, medium, small)"), "{e}");
    let e = o.check(&w("model == mediun")).unwrap_err();
    assert!(e.contains("`mediun` is no class"), "{e}");
    assert_eq!(o.check(&w("model.family == claude and model >= small")), Ok(()));
    // --model: a model or a class
    assert_eq!(o.asked("medium"), Ok(Asked::Class("medium".into(), 2)));
    assert_eq!(o.asked("claude-opus-5-5"), Ok(Asked::Model("claude-opus-5-5".into())));
    assert!(o.asked("huge").unwrap_err().contains("no class (large, medium, small)"));
}

/// 3: an agent that exists, whose model does not meet the call, is refused the step: it raises NoWorker with the
/// model and the condition said, and the ticket stops there, nothing above it handling that (with no lead, the owner
/// is asked; machine/stop.rs); one whose model meets it has the step due.
#[test]
fn a_step_is_refused_to_an_agent_whose_model_does_not_meet_its_call() {
    let mut c = Coord::new("give", None);
    let t = c.open("change", json!({"words": "the cache", "who": "ann"}));
    let Status::Stopped { raised: r, to, .. } = &c.core.tickets.m.tickets[&t].status else { panic!("{:?}", c.core.tickets.m.tickets[&t].status) };
    assert_eq!(to, "owner");
    let why = "ann runs claude-haiku-5-5 (1, small), and call design needs `where model >= medium`";
    assert_eq!((r.exc.as_str(), &r.value), ("NoWorker", &json!({"head": "NoWorker", "agent": "ann", "why": why})));
    assert!(c.core.ticket_due("ann").is_empty());
    let log = std::fs::read_to_string(c.dir.join("tickets/log.jsonl")).unwrap();
    assert!(log.lines().any(|l| l.starts_with("{\"ev\":\"model_refused\"") && l.contains(why)), "{log}");
    // bob runs sonnet: the step is his
    let u = c.open("change", json!({"words": "the cache", "who": "bob"}));
    assert!(matches!(c.core.tickets.m.tickets[&u].status, Status::Awaits { .. }));
    assert!(c.core.ticket_due("bob").iter().any(|d| d.t == u));
    // the record replays to the same
    let before = (c.core.tickets.m.tickets[&t].status.clone(), c.core.tickets.m.tickets[&u].status.clone());
    c.core.load_tickets();
    assert_eq!((c.core.tickets.m.tickets[&t].status.clone(), c.core.tickets.m.tickets[&u].status.clone()), before);
}

/// 4: a step that would make an agent with no model said refuses its kind at load, naming the call; a `where` on the
/// call, or a model-for rule of the project for agents, says it. A fork keeps its source's model.
#[test]
fn a_kind_that_makes_an_agent_with_no_model_said_is_refused_at_load() {
    let c = Coord::new("load", None);
    let kinds = &c.core.tickets.m.defs.kinds;
    assert!(kinds.contains_key("fresh_design") && kinds.contains_key("change") && kinds.contains_key("fork_plain"), "{:?}", kinds.keys().collect::<Vec<_>>());
    assert!(!kinds.contains_key("fresh_plain") && !kinds.contains_key("let_plain"));
    let errors = &c.core.tickets.errors;
    for k in ["fresh_plain", "let_plain"] {
        assert!(errors.iter().any(|e| e.contains(&format!("kind {k}: its step makes an agent for call plain, and nothing says its model"))), "{k}: {errors:#?}");
    }
    // the system's fix kind makes its reviewers fresh, its calls saying their model: it loads
    assert!(kinds.contains_key("fix"), "{errors:#?}");
    // a model-for rule of the project for agents: the kinds load
    let c = Coord::new("rule", Some("Prefer for model-for When call.place == \"agent\":\n  maximise price(option)\n"));
    assert!(c.core.tickets.m.defs.kinds.contains_key("fresh_plain") && c.core.tickets.m.defs.kinds.contains_key("let_plain"), "{:#?}", c.core.tickets.errors);
    // a rule only for another place is none
    let c = Coord::new("elsewhere", Some("Prefer for model-for When call.place == \"session\":\n  maximise price(option)\n"));
    assert!(!c.core.tickets.m.defs.kinds.contains_key("fresh_plain"));
    // a call whose `where` names no class is left out, said so
    let c = Coord::new("misspelt", None);
    std::fs::write(c.dir.join("files/calls/m.call"), CALLS.replace("model >= medium", "model >= mediun")).unwrap();
    let mut c = c;
    c.core.load_tickets();
    assert!(c.core.tickets.errors.iter().any(|e| e.contains("call design: `where model >= mediun`: ") && e.contains("the call is left out")), "{:#?}", c.core.tickets.errors);
}

/// 3: the model of an agent coordd makes for a step is the policy's model-for among the models that meet the call's
/// `where` (the system's policy: an agent's model runs in Claude Code, the cheapest first); those that do not are
/// left out as `where`. With no `where` and no rule of the project, none is chosen: refused.
#[test]
fn a_fresh_agents_model_is_model_for_among_those_that_meet_the_call() {
    proxy();
    let c = Coord::new("fresh", None);
    let places = c.core.places();
    let d = models::decide(&places, "f1-1", "design", Some("model >= medium")).unwrap();
    assert_eq!(d.chosen.as_deref(), Some("claude-sonnet-5-5"), "{d:?}");
    assert_eq!(d.call.get("place").map(String::as_str), Some("agent"));
    assert!(d.excluded.iter().any(|x| x.option == "claude-haiku-5-5" && x.reason == "where" && x.rule == "where model >= medium"), "{:?}", d.excluded);
    // the Codex model: no Claude Code session runs it
    assert!(d.excluded.iter().any(|x| x.option == "gpt-6-luna" && x.reason == "only"), "{:?}", d.excluded);
    assert_eq!(models::decide(&places, "f1-2", "x", Some("model >= large")).unwrap().chosen.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(models::decide(&places, "f1-3", "decomp", Some("model.class == small and model.family == claude")).unwrap().chosen.as_deref(), Some("claude-haiku-5-5"));
    // none meets: no model, each left out said
    let d = models::decide(&places, "f1-4", "x", Some("model > large")).unwrap();
    assert_eq!(d.chosen, None);
    assert!(models::why_none(&d).contains("claude-opus-5-5 (where model > large)"), "{}", models::why_none(&d));
    // no `where`, no rule: refused
    let e = models::decide(&places, "f1-5", "plain", None).unwrap_err();
    assert!(e.contains("plain says no model (no `where`)"), "{e}");
    // with the project's rule, it decides
    let c = Coord::new("fresh-rule", Some("Prefer for model-for When call.place == \"agent\":\n  maximise price(option)\n"));
    assert_eq!(models::decide(&c.core.places(), "f1-6", "plain", None).unwrap().chosen.as_deref(), Some("claude-opus-5-5"));
    // the clerk's calls stay on Codex only, and medium or above (paper-research N-1397.p89): with luna the one Codex
    // model served, none
    let clerk = crate::policy::Engine::start(c.core.places()).decide("model-for", [("place".to_string(), "call".to_string()), ("task".to_string(), "route".to_string())].into()).unwrap();
    assert_eq!(clerk.chosen, None, "{clerk:?}");
    assert!(clerk.excluded.iter().any(|x| x.option == "gpt-6-luna" && x.rule.contains("tier(option) >= 2")), "{:?}", clerk.excluded);
    // a copy keeps its model: one that does not meet the call is refused, not raised
    let e = models::copy_meets(&places, &families(), "ann", "claude-haiku-5-5", "design", "model >= medium").unwrap_err();
    assert_eq!(e, "a copy keeps the model of what it copies: ann runs claude-haiku-5-5 (1, small), and call design needs `where model >= medium`");
    assert_eq!(models::copy_meets(&places, &families(), "bob", "claude-sonnet-5-5", "design", "model >= medium"), Ok(()));
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// `coord new` and `coord fork` take `--model` as a model or a class; a copy keeps the model of what it copies; with
/// none said and no rule, `coord new` is refused (no default).
#[test]
fn coord_new_and_fork_take_a_model_or_a_class() {
    proxy();
    let c = Coord::new("make", None);
    let asked = |new: &str, fork: bool, model: Option<&str>| crate::make::Asked { new: new.into(), from: None, model: model.map(str::to_string), text: "x".into(), fork };
    let plan = |a| crate::make::plan(&c.core, &Who::Agent("orch".into()), a).unwrap();
    // a class: model-for among its models
    let mut p = plan(asked("d1", false, Some("medium")));
    let d = crate::make::model_of(&mut p).unwrap().expect("a decision");
    assert_eq!((p.model.as_str(), d.call.get("where").map(String::as_str)), ("claude-sonnet-5-5", Some("model.class == medium")));
    // a model: as named
    let mut p = plan(asked("d2", false, Some("claude-opus-5-5")));
    assert!(crate::make::model_of(&mut p).unwrap().is_none());
    assert_eq!(p.model, "claude-opus-5-5");
    // neither
    let mut p = plan(asked("d3", false, Some("huge")));
    assert!(crate::make::model_of(&mut p).unwrap_err().contains("--model huge is no model"));
    // none said, no rule: refused, never a default
    let mut p = plan(asked("d4", false, None));
    let e = crate::make::model_of(&mut p).unwrap_err();
    assert!(e.contains("coord new: say its model with --model, a model or a class (large, medium, small)"), "{e}");
    // a copy of itself (orch, opus): its own model, or its own class, and nothing else
    let mut p = plan(asked("o1", true, None));
    assert!(crate::make::model_of(&mut p).unwrap().is_none());
    assert_eq!(p.model, "claude-opus-5-5");
    let mut p = plan(asked("o2", true, Some("large")));
    assert!(crate::make::model_of(&mut p).unwrap().is_none());
    assert_eq!(p.model, "claude-opus-5-5");
    let mut p = plan(asked("o3", true, Some("small")));
    let e = crate::make::model_of(&mut p).unwrap_err();
    assert_eq!(e, "coord fork: a copy keeps the model of what it copies: orch runs claude-opus-5-5 (3, large), and --model small is another");
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// An agent coordd makes never runs a model a newer one of the same family and number has replaced (paper-research
/// N-101 p611): Sonnet 5 and 5.5 cost the same in the catalog, so the first by name took Sonnet 5 for medium; with the
/// policy preferring the higher price, Opus 5 would be taken for large. An agent that runs one keeps its number.
#[test]
fn an_agent_made_never_runs_a_replaced_model() {
    const WITH_OLD: &[(&str, &str)] = &[("claude-haiku-5-5", "anthropic"), ("claude-sonnet-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("claude-opus-5", "anthropic"), ("claude-opus-5-5", "anthropic"), ("claude-fable-5-1", "anthropic"), ("claude-fable-5-2", "anthropic")];
    proxy_serving(WITH_OLD);
    let c = Coord::new("replaced", None);
    let places = c.core.places();
    let d = models::decide(&places, "f1-1", "design", Some("model.class == medium")).unwrap();
    assert_eq!(d.chosen.as_deref(), Some("claude-sonnet-5-5"), "{d:?}");
    assert!(d.excluded.iter().any(|x| x.option == "claude-sonnet-5" && x.reason == "only"), "{:?}", d.excluded);
    // price cannot decide it: the policy prefers the dearer model, and the old Opus is dearer
    let c = Coord::new("replaced-dear", Some("Prefer for model-for When call.place == \"agent\":\n  maximise price(option)\n"));
    let places = c.core.places();
    assert_eq!(models::decide(&places, "f1-2", "x", Some("model >= large")).unwrap().chosen.as_deref(), Some("claude-fable-5-2"));
    assert_eq!(models::decide(&places, "f1-3", "x", Some("model.class == large")).unwrap().chosen.as_deref(), Some("claude-opus-5-5"));
    // an agent running one keeps its number: a copy of it meets a medium call
    let fam: Vec<(String, String)> = WITH_OLD.iter().map(|(m, _)| (m.to_string(), "claude".to_string())).collect();
    assert_eq!(models::copy_meets(&places, &fam, "old", "claude-sonnet-5", "design", "model >= medium"), Ok(()));
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// A step's call on the Docket (paper-research N-1396 p174, p223, p235; consult-3 p237): the Kinds answer gives each
/// kind's calls as coordd read them, and for a call coordd runs, the model the policy gives its task, solved off the
/// lock: two answers in a row solve nothing, a solve with nothing changed is not made, and a project policy that moves
/// `route` to tier 3 gives its new model at the next solve.
#[test]
fn the_kinds_answer_says_the_model_of_a_call_coordd_runs_and_solves_it_only_when_the_policy_changes() {
    const SERVED_ALL: &[(&str, &str)] = &[("claude-haiku-5-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("gpt-6-luna", "openai"), ("gpt-6-sol", "openai"), ("gpt-6.1-terra", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")];
    proxy_serving(SERVED_ALL);
    // an effort for intake, the project's: the system policy gives none since the watcher's `asks` went (proto-lead
    // N-1479 p247)
    const EFFORT: &str = "+effort-for(\"intake\", \"low\").\n";
    let mut c = Coord::new("kinds-models", Some(EFFORT));
    let solve = |c: &mut Coord| {
        let served = c.core.models().to_vec();
        if let Some(solved) = crate::policy::solve_task_models(c.core.task_models_ask(), &served) {
            c.core.set_task_models(solved);
        }
    };
    let call = |c: &Coord, kind: &str, name: &str| -> serde_json::Value {
        let r = c.core.ticket_kinds();
        assert!(r.ok, "{}", r.out);
        let kinds: Vec<serde_json::Value> = serde_json::from_str(&r.out).unwrap();
        let k = kinds.iter().find(|k| k["kind"] == kind).unwrap_or_else(|| panic!("kind {kind}"));
        k["calls"].as_array().unwrap().iter().find(|x| x["name"] == name).cloned().unwrap_or_else(|| panic!("{kind}'s calls name {name}; they are: {}", k["calls"]))
    };
    // the watcher's tasks, each with the call coordd runs for it, read at load
    let tasks = c.core.task_models_ask().tasks;
    assert_eq!(tasks.get("deliver").map(String::as_str), Some("choose"), "agent-base's delivery steps fold.call's `fold` in `free`, and coordd runs `choose` for it (proto-mind N-1446 p2105): {tasks:?}");
    assert_eq!(tasks.get("route").map(String::as_str), Some("whom"), "{tasks:?}");
    assert_eq!(tasks.get("intake").map(String::as_str), Some("sort"), "{tasks:?}");
    // the watcher's `asks` is gone (proto-lead N-1479 p247): no kind's step calls it; a thread from before that still
    // makes its step is answered Nothing with no call (`clerk_job`)
    assert!(!tasks.contains_key("asks"), "{tasks:?}");
    assert!(!tasks.contains_key("answer"), "a lead's task is not coordd's: {tasks:?}");
    assert_eq!(c.core.task_models.solves, 0, "nothing is solved at load");
    solve(&mut c);
    assert_eq!(c.core.task_models.solves, 1);
    // the call as read, and its task's model
    let route = call(&c, "base", "route");
    assert_eq!(route["task"], "route");
    assert_eq!(route["sig"], "call route(text: Text, to: [Agent], others: [Agent]) -> To(agents: [Agent], joins: Int?) | Split(parts: [Part]) | Unclear(why: Text)", "route: {route}");
    assert!(route["file"].as_str().unwrap().ends_with("route.call") && route["line"].as_u64().unwrap() > 0, "route: {route}");
    assert_eq!(route["runs"]["name"], "whom", "route: {route}");
    assert!(route["runs"]["template"].as_str().unwrap().contains("{{ yours }}"), "route: {route}");
    assert_eq!(route["model"], json!({"model": "gpt-6.2-sol"}), "route: {route}");
    // the delivery's one call: coordd's own, on gpt-6.2-sol as every call of its own (decisions.eflint:29)
    let fold = call(&c, "delivery", "fold");
    assert_eq!((fold["task"].clone(), fold["runs"]["name"].clone(), fold["model"].clone()), (json!("deliver"), json!("choose"), json!({"model": "gpt-6.2-sol"})), "fold: {fold}");
    assert_eq!(call(&c, "thread", "intake")["model"]["effort"], "low", "intake's effort is on its model");
    let answer = call(&c, "question", "answer");
    assert!(answer["template"].as_str().is_some() && answer["model"].is_null() && answer["runs"].is_null(), "the lead's call runs on its own model: {answer}");
    assert_eq!(call(&c, "base", "route")["model"], json!({"model": "gpt-6.2-sol"}));
    // no call coordd runs takes the small model: every one on the Kinds answer is on gpt-6.2-sol (decisions.eflint;
    // luna's Unclear on `whom` asked him whom his message was for, paper-research N-1397.p89)
    let kinds: Vec<serde_json::Value> = serde_json::from_str(&c.core.ticket_kinds().out).unwrap();
    let models: Vec<(&str, &str)> = kinds.iter().flat_map(|k| k["calls"].as_array().unwrap().iter()).filter(|x| !x["model"].is_null()).map(|x| (x["name"].as_str().unwrap_or("?"), x["model"]["model"].as_str().unwrap_or("?"))).collect();
    assert!(models.len() >= 4 && models.iter().all(|(_, m)| *m == "gpt-6.2-sol"), "{models:?}");
    // two answers in a row: the same, and nothing solved; a solve with nothing changed is not made
    let (a, b) = (c.core.ticket_kinds().out, c.core.ticket_kinds().out);
    assert_eq!(a, b);
    assert_eq!(c.core.task_models.solves, 1, "an answer only reads");
    solve(&mut c);
    assert_eq!(c.core.task_models.solves, 1, "nothing changed: nothing solved");
    // the project's policy moves route to tier 3: its new model at the next solve
    std::fs::write(c.dir.join("files/policy/models.eflint"), format!("{EFFORT}Only for model-for When call.task == \"route\":\n  tier(option) >= 3\n")).unwrap();
    solve(&mut c);
    assert_eq!(c.core.task_models.solves, 2);
    assert_eq!(call(&c, "base", "route")["model"], json!({"model": "gpt-6-astra"}));
    assert_eq!(call(&c, "thread", "intake")["model"]["model"], "gpt-6.2-sol", "another task's stays");
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// The forming calls run on astra, every other call on medium (paper-research N-101 p631, p683; N-1397.p89): with
/// gpt-6-sol at 2 and astra at 3 (the owner, N-265.p3), form-head and form-parts take gpt-6-astra, neither luna nor
/// sol; the clerk's calls stay on Codex; no model-for takes a replaced model, for a call either (the prices equal:
/// Sonnet 5 and 5.5 cost the same in the catalog).
#[test]
fn the_forming_calls_run_on_astra() {
    const SERVED_ALL: &[(&str, &str)] = &[("claude-haiku-5-5", "anthropic"), ("claude-sonnet-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("gpt-6-luna", "openai"), ("gpt-6-sol", "openai"), ("gpt-6.1-terra", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")];
    proxy_serving(SERVED_ALL);
    let c = Coord::new("forming", None);
    let engine = crate::policy::Engine::start(c.core.places());
    let ask = |call: &str| {
        let fields = [("place", "call"), ("task", "form"), ("call", call), ("agent", "dev")].map(|(k, v)| (k.to_string(), v.to_string())).into();
        engine.decide("model-for", fields).unwrap()
    };
    for call in ["form-head", "form-parts"] {
        let d = ask(call);
        assert_eq!(d.chosen.as_deref(), Some("gpt-6-astra"), "{call}: {d:?}");
        for low in ["gpt-6-luna", "gpt-6.1-terra", "gpt-6.2-sol"] {
            assert!(d.excluded.iter().any(|x| x.option == low && x.rule.contains("tier(option) >= 3")), "{call}: {low}: {:?}", d.excluded);
        }
    }
    // another call: medium, the cheapest of it (paper-research N-1397.p89), luna and terra left out as small
    let d = ask("clarify");
    assert_eq!(d.chosen.as_deref(), Some("gpt-6.2-sol"), "{d:?}");
    for low in ["gpt-6-luna", "gpt-6.1-terra"] {
        assert!(d.excluded.iter().any(|x| x.option == low && x.rule.contains("tier(option) >= 2")), "clarify: {low}: {:?}", d.excluded);
    }
    // the message runtime's `next` and his typing cleaned: medium, gpt-6.2-sol, now that it has replaced gpt-6.1-sol
    // (runtime-design §5; paper-research N-1397.p20). Under each decider: `both` (no file) takes the solver's answer
    // and journals a `decider both:` line where the picker's differs, so the solver and the picker are each asked
    // alone too and must answer alike (build 31's check, paper-research N-1396.p191). eflint-rs is asked no decision.
    let decider = format!("{}/{}", c.core.places().coord, crate::policy::DECIDER_FILE);
    let mut each = Vec::new();
    for word in [None, Some("solver"), Some("picker")] {
        if let Some(w) = word {
            std::fs::write(&decider, format!("{w}\n")).unwrap();
        }
        for (task, call) in [("deliver", "choose"), ("clean", "clean")] {
            let fields = [("place", "call"), ("task", task), ("call", call), ("agent", docket_machine::WATCHER)].map(|(k, v)| (k.to_string(), v.to_string())).into();
            let d = engine.decide("model-for", fields).unwrap();
            assert_eq!(d.chosen.as_deref(), Some("gpt-6.2-sol"), "{word:?} {task}: {d:?}");
            assert!(d.excluded.iter().any(|x| x.option == "gpt-6.1-sol" && x.rule.contains("not replaced(option)")), "{word:?} {task}: {:?}", d.excluded);
            // medium alone: refused, gpt-6.2-sol leaves no second option, astra neither (paper-research N-1397.p30)
            let left: Vec<&str> = SERVED_ALL.iter().map(|(m, _)| *m).filter(|m| !d.excluded.iter().any(|x| x.option == *m)).collect();
            assert_eq!(left, ["gpt-6.2-sol"], "{word:?} {task}: {:?}", d.excluded);
            assert!(d.excluded.iter().any(|x| x.option == "gpt-6-astra" && x.rule.contains("tier(option)")), "{word:?} {task}: {:?}", d.excluded);
            // what `decider both` compares (pick::differences), runner-up aside: the policy leaves no second option
            let mut excluded: Vec<String> = d.excluded.iter().map(|x| format!("{x:?}")).collect();
            excluded.sort();
            each.push((task, excluded, d.optimal, d.costs, d.decided_by, d.fault));
        }
    }
    std::fs::remove_file(&decider).unwrap();
    for (i, e) in each.iter().enumerate().skip(2) {
        assert_eq!(e, &each[i % 2], "{}", ["solver", "picker"][i / 2 - 1]);
    }
    // with the clerk's Codex rule out (a project that lets calls run on Claude), a call still never takes Sonnet 5
    let c = Coord::new("forming-claude", Some("Prefer for model-for When call.place == \"call\":\n  family(option, \"claude\") > family(option, \"codex\")\n"));
    let engine = crate::policy::Engine::start(c.core.places());
    let d = engine.decide("model-for", [("place", "call"), ("task", "form"), ("call", "form-head")].map(|(k, v)| (k.to_string(), v.to_string())).into()).unwrap();
    assert!(d.chosen.as_deref() != Some("claude-sonnet-5"), "{d:?}");
    assert!(d.excluded.iter().any(|x| x.option == "claude-sonnet-5" && x.rule.contains("not replaced(option)")), "{:?}", d.excluded);
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// Fallback mode (the owner, 20 Nov 21:34; paper-research N-1397 p117), on a stand-in gateway, no model called. Every
/// Codex option refused at its limit (tonight's 429): route, deliver, clean and form-head each go to Claude Sonnet 5.5,
/// and the decision says it went by the fallback and why, the refusals first in its attempts; Sonnet's request carries
/// no effort (clean's `low`, a project's line here, is model-for's; the system's one, the watcher's `asks`, went). A Codex option that answers is taken with no fallback, and the call
/// after a fallback asks Codex first again (no clock). A refusal that is no limit, a cooldown after a transient error
/// (consult-3 p125), or one limit and one not, gives no fallback; nor does a task decisions.eflint names none for (the brain's `attend`, proto-lead p118).
#[test]
fn every_codex_option_refused_at_its_limit_goes_to_sonnet_5_5_and_says_so() {
    const SERVED_ALL: &[(&str, &str)] = &[("claude-haiku-5-5", "anthropic"), ("claude-sonnet-5-5", "anthropic"), ("claude-opus-5-5", "anthropic"), ("gpt-6-luna", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")];
    const CODEX: [&str; 4] = ["gpt-6-luna", "gpt-6.1-sol", "gpt-6.2-sol", "gpt-6-astra"];
    const SONNET: &str = "claude-sonnet-5-5";
    let (gates, asked) = gateway(SERVED_ALL);
    let c = Coord::new("fallback", Some("+effort-for(\"clean\", \"low\").\n"));
    let engine = crate::policy::Engine::start(c.core.places());
    let call = |task: &str, name: &str| engine.call_prompt(task, name, "made up", &json!({"type": "string"}), docket_machine::WATCHER, &|_| Ok(())).unwrap();
    let set = |gate: Gate, models: &[&'static str]| {
        let mut g = gates.lock().unwrap();
        for m in models {
            g.insert(*m, gate);
        }
    };
    let take = || std::mem::take(&mut *asked.lock().unwrap());
    let named = |r: &[serde_json::Value]| r.iter().map(|q| q["model"].as_str().unwrap_or_default().to_string()).collect::<Vec<_>>();

    // Codex answers: its option, no fallback
    let got = call("route", "whom");
    assert_eq!((got.answer, got.decision.chosen.as_deref(), got.decision.fallback.as_deref()), (Some(json!("ok")), Some("gpt-6.2-sol"), None));
    assert_eq!(named(&take()), ["gpt-6.2-sol"]);
    assert!(serde_json::to_value(&got.decision).unwrap().get("fallback").is_none(), "a decided line as before");

    // every Codex option at its limit: Sonnet 5.5, saying so
    set(Gate::AtLimit, &CODEX);
    for (task, name, refused) in [("route", "whom", vec!["gpt-6.2-sol", "gpt-6-astra"]), ("deliver", "choose", vec!["gpt-6.2-sol"]), ("clean", "clean", vec!["gpt-6.2-sol"]), ("form", "form-head", vec!["gpt-6-astra"])] {
        let got = call(task, name);
        let d = &got.decision;
        assert_eq!((got.answer.clone(), d.chosen.as_deref(), d.decision.as_str(), d.fault.as_deref()), (Some(json!("ok")), Some(SONNET), "fallback-model", None), "{task}: {d:?}");
        let tried: Vec<&str> = d.attempts.iter().map(|a| a.model.as_str()).collect();
        assert_eq!(tried, refused.iter().copied().chain([SONNET]).collect::<Vec<_>>(), "{task}");
        assert!(d.attempts[..refused.len()].iter().all(|a| a.failure.as_deref().is_some_and(|f| f.contains("429 Too Many Requests") && f.contains("usage_limit_reached"))), "{task}: {:?}", d.attempts);
        assert_eq!(d.attempts.last().unwrap().failure, None);
        let why = d.fallback.as_deref().unwrap_or_default();
        assert!(why.starts_with("fallback mode: every option model-for gave was refused for a limit") && refused.iter().all(|m| why.contains(m)), "{task}: {why}");
        assert_eq!(serde_json::to_value(d).unwrap()["fallback"], json!(why), "{task}: on its decided line");
        let r = take();
        assert_eq!(named(&r), tried, "{task}");
        assert_eq!(r.last().unwrap().get("reasoning_effort"), None, "{task}: Sonnet goes out with no effort");
        if task == "clean" {
            assert!(r[..refused.len()].iter().all(|q| q["reasoning_effort"] == "low"), "{r:?}");
        }
    }

    // the next call Codex answers is Codex's again
    set(Gate::Answer, &CODEX);
    let got = call("route", "whom");
    assert_eq!((got.decision.chosen.as_deref(), got.decision.fallback.as_deref()), (Some("gpt-6.2-sol"), None));
    assert_eq!(named(&take()), ["gpt-6.2-sol"]);

    // a refusal that is no limit: no fallback, the call fails as before
    set(Gate::NotServed, &CODEX);
    let got = call("route", "whom");
    assert_eq!((got.answer, got.decision.chosen.clone(), got.decision.decision.as_str(), got.decision.fallback.clone()), (None, None, "model-for", None));
    assert_eq!(named(&take()), ["gpt-6.2-sol", "gpt-6-astra"]);
    // one at its limit, one not: not every option refused for a limit
    set(Gate::AtLimit, &["gpt-6.2-sol"]);
    let got = call("route", "whom");
    assert_eq!((got.answer, got.decision.chosen.clone(), got.decision.fallback.clone()), (None, None, None));
    assert_eq!(named(&take()), ["gpt-6.2-sol", "gpt-6-astra"]);
    // every option cooling after a transient error, not a limit (consult-3 p125): no fallback, and Sonnet is not asked
    set(Gate::Cooling, &CODEX);
    let got = call("route", "whom");
    assert_eq!((got.answer, got.decision.chosen.clone(), got.decision.decision.as_str(), got.decision.fallback.clone()), (None, None, "model-for", None));
    assert!(got.decision.attempts.iter().all(|a| a.failure.as_deref().is_some_and(|f| f.contains("model_cooldown") && f.contains("503"))), "{:?}", got.decision.attempts);
    assert_eq!(named(&take()), ["gpt-6.2-sol", "gpt-6-astra"]);

    // a task with no fallback line: at the limit it fails as before, and Sonnet is not asked
    set(Gate::AtLimit, &CODEX);
    let got = call("attend", "attend");
    assert_eq!((got.answer, got.decision.chosen.clone(), got.decision.decision.as_str(), got.decision.fallback.clone()), (None, None, "model-for", None));
    assert!(!named(&take()).contains(&SONNET.to_string()));
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// The gateway's Claude half moved to his accounts (proto-lead, I-1469 p22, 3): with the gateway serving Codex alone and
/// one of his Claude Code accounts signed in, every task's `model-for` and `fallback-model` are what they were with the
/// gateway serving its Claude models too (the policy's `claude-code-model`, its own Claude list), and fallback mode's
/// Sonnet 5.5 runs on that account, never at the gateway, the attempt saying so. Made up: a stand-in gateway and a
/// stand-in Claude Code; no model is called.
#[test]
fn with_the_gateways_claude_half_on_his_account_every_task_keeps_its_models_and_sonnet_runs_there() {
    const CODEX: &[(&str, &str)] = &[("gpt-6-luna", "openai"), ("gpt-6.1-sol", "openai"), ("gpt-6.2-sol", "openai"), ("gpt-6-astra", "openai")];
    const SONNET: &str = "claude-sonnet-5-5";
    let c = Coord::new("claude-half", None);
    let places = c.core.places();
    let tasks = c.core.task_models_ask().tasks;
    let claude: Vec<String> = crate::policy::pick::Picker::new(&places.sources().unwrap(), &[]).unwrap().holding("claude-code-model").into_iter().map(|r| r[0].clone()).collect();
    assert!(claude.contains(&SONNET.to_string()), "{claude:?}");
    // what each task takes: model-for (the Kinds answer's), and fallback-model asked with the same fields
    let models = || {
        let engine = crate::policy::Engine::start(places.clone());
        let fallback: std::collections::BTreeMap<String, Option<String>> = tasks
            .iter()
            .map(|(task, call)| {
                let fields = [("place", "call"), ("task", task.as_str()), ("call", call.as_str()), ("agent", docket_machine::WATCHER)].map(|(k, v)| (k.to_string(), v.to_string())).into();
                (task.clone(), engine.decide(crate::policy::FALLBACK, fields).unwrap().chosen)
            })
            .collect();
        (engine.task_models(&tasks), fallback)
    };
    let all: Vec<(&'static str, &'static str)> = CODEX.iter().copied().chain(claude.iter().map(|m| (&*Box::leak(m.clone().into_boxed_str()), "anthropic"))).collect();
    proxy_serving(Box::leak(all.into_boxed_slice()));
    let before = models();
    assert!(before.0.values().all(|m| m.model.is_some()) && before.1.get("route") == Some(&Some(SONNET.to_string())), "{before:?}");

    // the gateway with no Claude login, every Codex option at its limit; his account b signed in
    let (gates, asked) = gateway(CODEX);
    let t = crate::accounts::testing::Tmp::new("claude-half");
    let h = crate::accounts::Harness::Claude;
    t.store.create(h, "b", "B", 10, crate::accounts::Kind::Signin, "x").unwrap();
    t.store.update(h, "b", |s| (s.signed_in, s.heard) = (true, Some(50))).unwrap();
    let program = t.program("claude", "#!/bin/sh\ncat > /dev/null\necho '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"structured_output\":{\"answer\":\"from b\"}}'\n");
    crate::accounts::router::TEST_ACCOUNTS.with(|x| *x.borrow_mut() = Some((t.store.clone(), program)));
    assert_eq!(models(), before, "the same models for every task");

    gates.lock().unwrap().extend(CODEX.iter().map(|(m, _)| (*m, Gate::AtLimit)));
    let engine = crate::policy::Engine::start(places.clone());
    let got = engine.call_prompt("route", "whom", "made up", &json!({"type": "string"}), docket_machine::WATCHER, &|_| Ok(())).unwrap();
    let d = &got.decision;
    assert_eq!((got.answer.clone(), d.decision.as_str(), d.chosen.as_deref()), (Some(json!("from b")), "fallback-model", Some(SONNET)), "{d:?}");
    let (last, refused) = d.attempts.split_last().unwrap();
    assert_eq!(last.account.as_deref(), Some("b"));
    assert!(!refused.is_empty() && refused.iter().all(|a| a.account.is_none() && a.failure.as_deref().is_some_and(|f| f.contains("usage_limit_reached"))), "{refused:?}");
    assert!(asked.lock().unwrap().iter().all(|q| q["model"] != SONNET), "Sonnet never went to the gateway");
    crate::accounts::router::TEST_ACCOUNTS.with(|x| *x.borrow_mut() = None);
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// The GPT models' numbers (the owner, N-265.p3 and p5): luna and gpt-6.1-terra small, sol, gpt-6.1-sol and
/// gpt-6.2-sol medium, astra large.
#[test]
fn the_gpt_models_are_luna_small_sol_medium_astra_large() {
    let c = Coord::new("gpt", None);
    let order = Order::read(&c.core.places(), &[]).unwrap();
    assert_eq!([order.tier.get("gpt-6-luna"), order.tier.get("gpt-6-sol"), order.tier.get("gpt-6-astra")], [Some(&1), Some(&2), Some(&3)]);
    // the newer two (N-265.p5): terra small, the newer sol medium, the older sol replaced
    assert_eq!([order.tier.get("gpt-6.1-terra"), order.tier.get("gpt-6.1-sol")], [Some(&1), Some(&2)]);
    // and gpt-6.2-sol medium, which replaced gpt-6.1-sol (paper-research N-1397.p20)
    assert_eq!(order.tier.get("gpt-6.2-sol"), Some(&2));
    assert!(!order.tier.contains_key("gpt-6-terra"), "{:?}", order.tier);
}

/// An agent made for a step runs at the effort the policy gives the step's call (`effort-for(CALL, LEVEL)`,
/// paper-research N-101 p673, p683; Tovrin's `+effort-for("mind", "medium").`): the model model-for chose, with
/// `:LEVEL` as roster takes it. A level the model does not take is refused, never dropped for the default.
#[test]
fn a_made_agent_runs_at_the_effort_the_policy_gives_its_call() {
    proxy();
    let c = Coord::new("effort", Some("+effort-for(\"mind\", \"medium\").\n"));
    let places = c.core.places();
    // the fact as Tovrin writes it loads (the test project's own kinds that say no model are refused, as ever)
    assert!(!c.core.tickets.errors.iter().any(|e| e.starts_with("the policy")), "{:#?}", c.core.tickets.errors);
    assert_eq!(models::effort_of(&places, "mind").unwrap().as_deref(), Some("medium"));
    assert_eq!(models::effort_of(&places, "design").unwrap(), None, "no fact for the call: no effort");
    // the system's own gives no task one since the watcher's `asks`, its one, went (proto-lead N-1479 p247)
    assert_eq!(models::effort_of(&places, "asks").unwrap(), None);
    assert_eq!(models::effort_of(&places, "intake").unwrap(), None);
    let d = models::decide(&places, "f1-1", "mind", Some("model.class == large")).unwrap();
    let m = d.chosen.clone().unwrap();
    assert_eq!(models::at_effort("mind", &m, Some("claude"), "medium", None).as_deref(), Ok("claude-opus-5-5:medium"));
    // a level Claude Code does not take
    let e = models::at_effort("mind", "claude-opus-5-5", Some("claude"), "huge", None).unwrap_err();
    assert_eq!(e, "the policy gives call mind effort huge, which claude-opus-5-5 does not take (low, medium, high, xhigh, max)");
    // a Codex model: the levels its catalog lists
    let levels = Some(Ok(vec!["low".to_string(), "medium".to_string(), "high".to_string()]));
    assert_eq!(models::at_effort("mind", "gpt-6-astra", Some("codex"), "high", levels.clone()).as_deref(), Ok("gpt-6-astra:high"));
    assert!(models::at_effort("mind", "gpt-6-astra", Some("codex"), "xhigh", levels).unwrap_err().contains("which gpt-6-astra does not take (low, medium, high)"));
    assert!(models::at_effort("mind", "gpt-6-astra", Some("codex"), "high", Some(Err("no catalog".into()))).unwrap_err().contains("the levels gpt-6-astra takes are not known: no catalog"));
    assert!(models::at_effort("mind", "x", None, "high", None).unwrap_err().contains("takes no effort"));
    crate::policy::facts::TEST_URL.with(|u| *u.borrow_mut() = None);
}

/// An instance with another number of values than its fact declares is refused at load, with its file, its line and
/// both counts (tovrin-writer N-101 p677: `+effort-for("mind").` loaded without a word, and would have left the
/// minder at the default effort).
#[test]
fn an_instance_with_the_wrong_number_of_values_is_refused_at_load() {
    let c = Coord::new("arity", Some("// the minder\n+effort-for(\"mind\").\n"));
    let e = c.core.tickets.errors.iter().find(|e| e.starts_with("the policy: ")).unwrap_or_else(|| panic!("{:#?}", c.core.tickets.errors));
    assert!(e.contains("policy/models.eflint:2") && e.contains("`effort-for` has 2 identifiers, the instance gives 1"), "{e}");
}

/// What the real proxy serves and what each kind of decision takes from it, read from the decisions' records (paper-
/// research N-101 p683). Ignored: it asks the proxy of the dev coord DOCKET_REAL_COORD with that coord's key.
#[test]
#[ignore]
fn what_the_real_proxy_serves_and_each_decision_takes() {
    let coord = std::env::var("DOCKET_REAL_COORD").expect("DOCKET_REAL_COORD: a dev coord with a .proxy-key");
    let places = crate::policy::Places { base: base(), coord: coord.clone(), ..Default::default() };
    let engine = crate::policy::Engine::start(places.clone());
    let served = engine.served().unwrap();
    println!("served: {}", served.iter().map(|(m, f)| format!("{m} ({f})")).collect::<Vec<_>>().join(", "));
    for (what, cond) in [("made, where model >= medium", "model >= medium"), ("made, where model.class == medium", "model.class == medium"), ("made, where model >= large", "model >= large"), ("made, where model.class == large", "model.class == large")] {
        let d = models::decide(&places, "f1-1", "x", Some(cond)).unwrap();
        println!("{what}: {:?}", d.chosen);
    }
    let call = |c: &str, task: &str| engine.decide("model-for", [("place", "call"), ("task", task), ("call", c), ("agent", "dev")].map(|(k, v)| (k.to_string(), v.to_string())).into()).unwrap();
    println!("an LLM call with no rule (clarify): {:?}", call("clarify", "clarify").chosen);
    println!("form-parts: {:?}", call("form-parts", "form").chosen);
    println!("form-head: {:?}", call("form-head", "form").chosen);
}

/// An agent made for a step that ran in a view when coordd went down, and still owes its step, is started again in
/// it; one made for a step that owes nothing is not (docket-core3, N-101: f17-2 held P-17.s3 after the restart with no
/// process behind it).
#[test]
fn a_step_agent_that_still_owes_its_step_is_started_again_after_a_restart() {
    proxy();
    let mut c = Coord::new("restart", None);
    let add = |n: &str, i: u32| format!("{{\"ev\":\"add\",\"id\":\"AGT-01{i}\",\"name\":\"{n}\",\"parent\":\"owner\",\"kind\":\"claude\",\"model\":\"claude-sonnet-5-5\",\"at\":\"y\"}}\n");
    let reg = c.dir.join("registry.jsonl");
    let was = std::fs::read_to_string(&reg).unwrap();
    std::fs::write(&reg, format!("{was}{}{}", add("f17-2", 1), add("f4-1", 2))).unwrap();
    for n in ["f17-2", "f4-1"] {
        std::fs::create_dir_all(c.dir.join(format!("agents/{n}"))).unwrap();
        std::fs::write(c.dir.join(format!("agents/{n}/layer")), "line@\n").unwrap();
    }
    c.core = Core::load(&c.dir.display().to_string(), &base());
    c.core.project = crate::project::Project::load(&c.dir.display().to_string()).unwrap();
    c.core.set_models(families());
    c.core.load_tickets();
    c.open("change", serde_json::json!({"words": "x", "who": "f17-2"}));
    assert!(!c.core.tickets.m.owes("f17-2").is_empty(), "{:?}", c.core.tickets.errors);
    std::fs::write(c.dir.join("running.json"), r#"["f17-2","f4-1"]"#).unwrap();
    c.core.hand_over_running(&[]);
    assert_eq!(c.core.resuming, vec!["f17-2".to_string()]);
    assert!(!c.dir.join("resume.json").exists(), "neither is handed to roster resume, outside its view");
}
