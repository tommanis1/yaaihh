//! Part 4's library text (paper-research N-578 p3, N-730 E3; one-interpreter.md §3, §6):
//! `agent-base/policy/eflint/yaaihh.eflint`, in part 3's eFLINT text in place of the dialect's teams.eflint and
//! slow.eflint, checked against eflint-rs directly:
//!
//! - it loads in part 3's text of agent-base's policy (`interp::load`), in place of teams.eflint and slow.eflint;
//! - its tiers, classes and superseded models are teams.eflint's (`replaced` there), each instance asked;
//! - each act it shares with the dialect (`start-subagent`, `ask-team`, `approve-team`, `add-agent`, `set-slow`,
//!   `lift-slow`) is enabled exactly where norms.rs enables the dialect's, in the same situations, stated as coordd
//!   states them (`Core::norm_facts`);
//! - its effects: slow mode set and lifted, the lead derived from the role, a member refused a team, a project's
//!   `Extend` letting the lead set slow mode.

use crate::policy::{norms, Places};
use eflint::replay::{Answer, Event, Replay, Value};

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

fn library() -> String {
    let path = format!("{}/policy/eflint/yaaihh.eflint", base());
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// agent-base's policy as part 3 loads it (the dialect's files written out, teams.eflint and slow.eflint left out,
/// then the files in eFLINT whole, this library among them), with MORE after it.
fn loaded(more: &str) -> Replay {
    let spec = crate::policy::interp::load(&Places { base: base(), ..Default::default() }).unwrap();
    Replay::from_text(&format!("{}{more}", spec.text)).unwrap()
}

fn s(x: &str) -> Value {
    Value::Str(x.to_string())
}

fn ok(a: Answer) -> Answer {
    assert!(a.ok && a.errors.is_empty(), "{:?}", a.errors);
    a
}

/// A situation: a team and its models, as coordd states them to the norms (`Core::norm_facts`).
#[derive(Clone)]
struct Team {
    /// name, role, parent
    agents: Vec<(&'static str, &'static str, Option<&'static str>)>,
    /// agent, model
    runs: Vec<(&'static str, &'static str)>,
    /// agent, granted, used
    grants: Vec<(&'static str, i64, i64)>,
    /// added to a team
    members: Vec<&'static str>,
}

const MODELS: [(&str, &str); 6] = [("claude-haiku-5-5", "claude"), ("claude-sonnet-5-5", "claude"), ("claude-opus-5-5", "claude"), ("gpt-6-luna", "codex"), ("gpt-6-astra", "codex"), ("kimi-k2", "kimi")];

impl Team {
    /// Every agent above A, up its parents (coordd's walk for `below`).
    fn above(&self, a: &str) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut at = a;
        while let Some(q) = self.agents.iter().find(|(n, _, _)| *n == at).and_then(|(_, _, p)| *p) {
            if out.contains(&q) {
                break;
            }
            out.push(q);
            at = q;
        }
        out
    }

    /// The facts, as `Core::norm_facts` makes them: name and identifiers as text.
    fn facts(&self) -> Vec<(String, Vec<String>)> {
        let mut f: Vec<(String, Vec<String>)> = Vec::new();
        let mut p = |n: &str, v: &[&str]| f.push((n.to_string(), v.iter().map(|x| x.to_string()).collect()));
        for (a, r, _) in &self.agents {
            p("agent", &[a]);
            p("role", &[a, r]);
        }
        for (m, fam) in MODELS {
            p("model", &[m]);
            p("family", &[m, fam]);
        }
        for (a, m) in &self.runs {
            p("runs", &[a, m]);
        }
        for (a, _, parent) in &self.agents {
            if let Some(q) = parent {
                p("parent", &[a, q]);
            }
            for up in self.above(a) {
                p("below", &[a, up]);
            }
        }
        for (a, g, u) in &self.grants {
            p("granted", &[a, &g.to_string()]);
            p("used", &[a, &u.to_string()]);
        }
        for a in &self.members {
            p("member", &[a]);
        }
        f
    }

    /// The library loaded after the prelude, with the same facts stated (a number as a number).
    fn replay(&self) -> Replay {
        let mut r = loaded("");
        for (name, ids) in self.facts() {
            let objects = ids.iter().map(|i| i.parse::<i64>().map_or_else(|_| s(i), Value::Int)).collect();
            ok(r.apply(&Event::Fact { name, objects, holds: Some(true) }));
        }
        r
    }

    /// The same facts stated on a copy of BASE (the text loaded once), each performed without reading back every
    /// act enabled (`Replay::perform`), as a situation needs only its facts. Its acts are then asked by `perform` too:
    /// `apply` on this state reads acts `perform` left underived.
    fn on(&self, base: &Replay) -> Replay {
        let mut r = Replay { cfg: base.cfg.clone() };
        for (name, ids) in self.facts() {
            let objects = ids.iter().map(|i| i.parse::<i64>().map_or_else(|_| s(i), Value::Int)).collect();
            ok(r.perform(&Event::Fact { name, objects, holds: Some(true) }));
        }
        r
    }
}

/// Whether the library's ACT with ACTOR and OBJECTS is enabled in SITUATION (its facts stated): performed on a copy
/// of it, executed and not reported as disabled.
fn library_enables(situation: &Replay, act: &str, actor: &str, objects: Vec<Value>) -> bool {
    let a = ok(Replay { cfg: situation.cfg.clone() }.perform(&Event::Act { name: act.into(), actor: s(actor), objects }));
    assert!(!a.executed.is_empty(), "{act} {actor}: nothing executed");
    a.disabled.is_empty()
}

fn teams() -> Vec<Team> {
    let agents = vec![("owner", "owner", None), ("orch", "lead", None), ("dev", "member", Some("orch")), ("w1", "member", Some("dev")), ("w2", "member", Some("w1")), ("cons", "consultant", None)];
    let runs = vec![("orch", "claude-opus-5-5"), ("dev", "claude-sonnet-5-5"), ("w1", "claude-haiku-5-5"), ("w2", "gpt-6-astra"), ("cons", "kimi-k2")];
    let plain = Team { agents: agents.clone(), runs: runs.clone(), grants: Vec::new(), members: Vec::new() };
    let room = Team { grants: vec![("dev", 2, 1)], members: vec!["w1"], ..plain.clone() };
    let full = Team { grants: vec![("dev", 2, 2), ("w1", 1, 0)], members: vec!["w1", "w2"], ..plain.clone() };
    let zero = Team { grants: vec![("dev", 0, 0), ("cons", 3, 2)], ..plain.clone() };
    vec![plain, room, full, zero]
}

#[test]
fn the_library_loads_and_states_the_system_policys_tiers_classes_and_superseded_models() {
    let mut r = loaded("");
    let scopes = Places { base: base(), ..Default::default() }.sources().unwrap();
    let mut asked = std::collections::BTreeMap::<String, usize>::new();
    for d in scopes.iter().flat_map(|(_, ds)| ds) {
        let crate::policy::syntax::DeclKind::Instance { fact, args } = &d.kind else { continue };
        if !["tier", "class", "replaced"].contains(&fact.as_str()) {
            continue;
        }
        let arg = |e: &crate::policy::syntax::Expr| match e {
            crate::policy::syntax::Expr::Str(x) => format!("{x:?}"),
            crate::policy::syntax::Expr::Int(n) => n.to_string(),
            other => panic!("{other:?}"),
        };
        // the dialect's `replaced` is the library's `superseded`
        let named = if fact == "replaced" { "superseded" } else { fact.as_str() };
        let text = match (fact.as_str(), &args[..]) {
            ("class", [n, k]) => format!("class({}, {})", arg(n), arg(k)),
            (_, [m, rest @ ..]) => format!("{named}({})", std::iter::once(format!("model({})", arg(m))).chain(rest.iter().map(arg)).collect::<Vec<_>>().join(", ")),
            _ => panic!("{fact} {args:?}"),
        };
        let a = ok(r.apply(&Event::Phrase(format!("?{text}."))));
        assert_eq!(a.queries, vec![true], "the library lacks {text}");
        *asked.entry(named.to_string()).or_default() += 1;
    }
    // and no more than the system policy's: as many instances of each
    let lib = library();
    for (fact, n) in &asked {
        assert_eq!(lib.lines().filter(|l| l.starts_with(&format!("+{fact}("))).count(), *n, "{fact}");
    }
    assert_eq!(asked.len(), 3, "{asked:?}");
    // the files it takes the place of, on its first line, for part 3's load
    assert_eq!(lib.lines().next(), Some("// replaces: teams.eflint slow.eflint"));
}

/// Each act the library and the dialect share, for every agent of each situation (and every model, for
/// start-subagent, every agent as approve-team's recipient), enabled in the library exactly where norms.rs enables it.
#[test]
fn the_librarys_acts_answer_as_the_dialects_do_in_the_same_situations() {
    let n = norms::load(&Places { base: base(), ..Default::default() }).unwrap();
    let mut compared = 0;
    let mut differ = Vec::new();
    let t0 = std::time::Instant::now();
    let spec = crate::policy::interp::load(&Places { base: base(), ..Default::default() }).unwrap();
    let text = Replay::from_text(&spec.text).unwrap();
    let mut times = vec![format!("the text loaded: {:?}", t0.elapsed())];
    // the same policy without the library (the dialect's teams.eflint and slow.eflint written out in its place), for
    // what part 3 answers in prod before part 4
    let without = std::env::temp_dir().join(format!("library-without-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&without);
    std::fs::create_dir_all(without.join("policy/eflint")).unwrap();
    for e in std::fs::read_dir(format!("{}/policy", base())).unwrap().flatten() {
        if e.path().is_file() {
            std::fs::copy(e.path(), without.join("policy").join(e.file_name())).unwrap();
        }
    }
    for e in std::fs::read_dir(format!("{}/policy/eflint", base())).unwrap().flatten() {
        if e.file_name() != "yaaihh.eflint" {
            std::fs::copy(e.path(), without.join("policy/eflint").join(e.file_name())).unwrap();
        }
    }
    let dialect_only = crate::policy::interp::load(&Places { base: without.display().to_string(), ..Default::default() }).unwrap();
    for (k, team) in teams().iter().enumerate() {
        let t0 = std::time::Instant::now();
        let situation = team.on(&text);
        let stated = t0.elapsed();
        // what part 3 answers in prod for one asker in this situation (`Spec::answers`: the text, the facts, every act)
        let t0 = std::time::Instant::now();
        let rows = spec.answers(&team.facts(), Some("orch")).unwrap().len();
        let with = t0.elapsed();
        let t0 = std::time::Instant::now();
        let mut facts = team.facts();
        facts.push(("scope".into(), vec!["project".into()]));
        let rows_before = dialect_only.answers(&facts, Some("orch")).unwrap().len();
        times.push(format!("situation {k}: facts stated {stated:?}; part 3's answers for one asker {with:?} ({rows} rows), without the library {:?} ({rows_before} rows)", t0.elapsed()));
        let checks_at = (std::time::Instant::now(), compared);
        let mut facts: Vec<_> = team.facts();
        facts.push(("scope".into(), vec!["project".into()]));
        let facts: Vec<_> = facts.into_iter().filter(|(f, _)| n.facts.contains_key(f)).collect();
        let shown = n.solve(&facts).unwrap();
        let dialect = |act: &str, args: &[&str]| n.verdict(&shown, act, args).unwrap().unwrap_or_else(|| panic!("no act {act}")).enabled;
        let names: Vec<&str> = team.agents.iter().map(|(a, _, _)| *a).collect();
        let mut check = |what: String, d: bool, l: bool| {
            compared += 1;
            if d != l {
                differ.push(format!("situation {k}: {what}: dialect {d}, library {l}"));
            }
        };
        for a in &names {
            check(format!("ask-team {a}"), dialect("ask-team", &[a]), library_enables(&situation, "ask-team", a, vec![]));
            check(format!("add-agent {a}"), dialect("add-agent", &[a]), library_enables(&situation, "add-agent", a, vec![]));
            check(format!("set-slow {a}"), dialect("set-slow", &[a, "project"]), library_enables(&situation, "set-slow", a, vec![s("project")]));
            check(format!("lift-slow {a}"), dialect("lift-slow", &[a, "project"]), library_enables(&situation, "lift-slow", a, vec![s("project")]));
            for b in &names {
                check(format!("approve-team {a} {b}"), dialect("approve-team", &[a, b]), library_enables(&situation, "approve-team", a, vec![s(b)]));
            }
            for (m, _) in MODELS {
                check(format!("start-subagent {a} {m}"), dialect("start-subagent", &[a, m]), library_enables(&situation, "start-subagent", a, vec![s(m)]));
            }
        }
        let n = compared - checks_at.1;
        times.push(format!("situation {k}: {n} acts checked, {:?} each", checks_at.0.elapsed() / n.max(1) as u32));
    }
    let _ = std::fs::remove_dir_all(&without);
    eprintln!("{}", times.join("\n"));
    assert!(differ.is_empty(), "{} of {compared} differ:\n{}", differ.len(), differ.join("\n"));
    assert_eq!(compared, 4 * 6 * (4 + 6 + 6));
}

fn holds(r: &mut Replay, phrase: &str) -> bool {
    ok(r.apply(&Event::Phrase(format!("?{phrase}.")))).queries == vec![true]
}

/// What the library's acts do: slow mode set and lifted by the owner, not by a lead until a project says so; the lead
/// derived from the role; a member refused a team, another agent not.
#[test]
fn the_librarys_effects() {
    let team = &teams()[1];
    let act = |r: &mut Replay, name: &str, actor: &str, objects: Vec<Value>| ok(r.apply(&Event::Act { name: name.into(), actor: s(actor), objects }));
    let mut r = team.replay();
    assert!(!act(&mut r, "ask-team", "w1", vec![]).disabled.is_empty(), "a member asks no team");
    assert!(act(&mut r, "ask-team", "dev", vec![]).disabled.is_empty());
    assert!(holds(&mut r, r#"lead(agent("orch"))"#) && !holds(&mut r, r#"lead(agent("dev"))"#));
    assert!(!act(&mut r, "set-slow", "orch", vec![s("project")]).disabled.is_empty(), "a lead may not, until a project says so");
    let mut r = team.replay();
    assert!(act(&mut r, "set-slow", "owner", vec![s("project")]).disabled.is_empty());
    assert!(holds(&mut r, r#"slowed(slow-scope("project"))"#));
    assert!(act(&mut r, "lift-slow", "owner", vec![s("project")]).disabled.is_empty());
    assert!(!holds(&mut r, r#"slowed(slow-scope("project"))"#));
    // a project's clause: the lead may set slow mode
    let mut p = loaded("\nExtend Act set-slow\n  Holds when lead(agent).\n");
    for (n, role) in [("orch", "lead"), ("owner", "owner")] {
        ok(p.apply(&Event::Fact { name: "agent".into(), objects: vec![s(n)], holds: Some(true) }));
        ok(p.apply(&Event::Fact { name: "role".into(), objects: vec![s(n), s(role)], holds: Some(true) }));
    }
    assert!(act(&mut p, "set-slow", "orch", vec![s("project")]).disabled.is_empty(), "a project's Extend lets the lead");
}
