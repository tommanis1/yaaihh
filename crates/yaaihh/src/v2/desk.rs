//! The threads of a coordd-v2 record, as the window's thread pages draw them: read from the record's lines alone.

// Which Acts, Events and Effect make a thread is the package's to say, in its threads file (map.rs).

use super::map::{Map, Rule};
use coord_types::TicketView;
use serde_json::{json, Value as Json};
use std::collections::BTreeMap;

/// A refusal the record wrote: the act, the party that performed it, and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Refusal {
    pub act: String,
    pub party: String,
    pub why: String,
}

/// The name the window's threads give the seated party, as every thread page reads him.
pub const HIM: &str = "owner";

/// A line of the record that asks something and waits for its answer: its place, its Effect and arguments, its run.
#[derive(Clone, Debug, PartialEq)]
pub struct Waiting {
    pub line: u64,
    pub op: String,
    pub args: Vec<Json>,
    pub run: String,
}

/// The record's lines as the window has them, by place.
#[derive(Default)]
pub struct Desk {
    lines: BTreeMap<u64, Json>,
    /// Bumped by each new line, so a page keyed on it reads again.
    pub rev: u64,
    /// What the lines say of the harness's programs, each line counted once (programs/live.rs).
    pub tally: crate::programs::live::Tally,
}

/// A value of the record as a word: a text as it is, anything else as its JSON.
fn word(v: &Json) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_string)
}

impl Desk {
    /// Line `line` at place `at`; false when it was there already.
    pub fn take(&mut self, at: u64, line: Json) -> bool {
        if self.lines.contains_key(&at) {
            return false;
        }
        self.tally.take(at, &line);
        self.lines.insert(at, line);
        self.rev += 1;
        true
    }

    /// How many lines are held.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// The parties the record says performed an act, in the order each first did.
    pub fn parties(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in self.lines.values().filter_map(|l| l.get("performed")?.get("party")?.as_str()) {
            if !out.iter().any(|o| o == p) {
                out.push(p.to_string());
            }
        }
        out
    }

    /// The hash of the text the engine runs, from its latest `loaded` line.
    pub fn loaded_line(&self) -> Option<&str> {
        self.lines.values().rev().find_map(|l| l.get("loaded")?.as_str())
    }

    /// The refusal in the record's latest line performing or refusing one of ACTS, if that line is a refusal.
    pub fn refused(&self, acts: &[&str]) -> Option<Refusal> {
        let of = |l: &Json| ["performed", "refused"].iter().any(|k| l.get(k).and_then(|x| x["act"]["inst"].as_str()).is_some_and(|i| acts.contains(&i)));
        let r = self.lines.values().rev().find(|l| of(l))?.get("refused")?;
        let why = match &r["why"] {
            Json::Object(o) => o.iter().map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or_default())).collect::<Vec<_>>().join(", "),
            w => w.as_str().unwrap_or_default().to_string(),
        };
        Some(Refusal { act: r["act"]["inst"].as_str()?.to_string(), party: r["party"].as_str().unwrap_or("no party").to_string(), why })
    }

    /// The party seated as the owner, once a line says so.
    pub fn owner(&self, m: &Map) -> Option<String> {
        self.lines.values().find_map(|l| arg(l, &m.owner, "who"))
    }

    /// The lines asking one of `ops` that wait still: no answer came, and the run that asked was not cancelled.
    pub fn waiting(&self, ops: &[&str]) -> Vec<Waiting> {
        let answered: std::collections::BTreeSet<u64> = self.lines.values().filter_map(|l| l.get("answered")?.get("line")?.as_u64()).collect();
        let cancelled: std::collections::BTreeSet<&str> = self.lines.values().filter_map(|l| l.get("cancelled")?.get("run")?.as_str()).collect();
        let asks = self.lines.iter().filter_map(|(at, l)| {
            let e = l.get("effect")?;
            let op = e.get("op")?.as_str().filter(|op| ops.contains(op))?;
            let args = e.get("args")?.as_array()?.clone();
            Some(Waiting { line: *at, op: op.to_string(), args, run: e.get("run")?.as_str()?.to_string() })
        });
        asks.filter(|w| !answered.contains(&w.line) && !cancelled.contains(w.run.as_str())).collect()
    }

    /// Each passage's words: the answer to the line of the words' Effect that asked for it.
    fn texts(&self, m: &Map) -> BTreeMap<String, String> {
        let asked: BTreeMap<u64, String> = self
            .lines
            .iter()
            .filter_map(|(at, l)| {
                let e = l.get("effect")?;
                let id = e.get("args")?.get(m.words.at("passage")?)?;
                (e.get("op")?.as_str()? == m.words.name).then(|| (*at, word(id)))
            })
            .collect();
        let answered = |l: &Json| {
            let a = l.get("answered")?;
            Some((asked.get(&a.get("line")?.as_u64()?)?.clone(), word(a.get("value")?)))
        };
        self.lines.values().filter_map(answered).collect()
    }

    /// The threads, numbered in the order they were opened, as the window's thread pages read them; a passage's time
    /// only where the package gives one.
    pub fn tickets(&self, m: &Map) -> BTreeMap<u64, TicketView> {
        let (texts, owner) = (self.texts(m), self.owner(m).unwrap_or_default());
        let who = |p: String| if p == owner { HIM.to_string() } else { p };
        // who each passage was said to, and which were said to him, wherever their lines stand
        let said_to: BTreeMap<String, String> = self.lines.values().filter_map(|l| { let r = first(&m.said_to, l)?; Some((arg(l, r, "passage")?, arg(l, r, "to")?)) }).collect();
        let to_owner: Vec<String> = self.lines.values().filter_map(|l| arg(l, first(&m.told, l)?, "passage")).collect();
        let mut used: std::collections::BTreeSet<u64> = std::collections::BTreeSet::new();
        let written: BTreeMap<String, i64> =
            self.lines.values().filter_map(|l| { let r = first(&m.written, l)?; Some((arg(l, r, "passage")?, args(l)?.get(r.at("at")?)?.as_i64()?)) }).collect();
        let mut threads: Vec<(String, Json)> = Vec::new();
        // each passage's place on its thread, and to whom it was handed
        let mut placed: BTreeMap<String, (usize, usize)> = BTreeMap::new();
        let mut handed: Vec<(String, String)> = Vec::new();
        for l in self.lines.values() {
            let a = |r: &Rule, role: &str| arg(l, r, role).unwrap_or_default();
            let at = |threads: &[(String, Json)], name: &str| threads.iter().position(|(t, _)| t == name);
            // one line may be several things at once (an imported passage said to him is a post too)
            let time = |r: &Rule| r.at("at").and_then(|at| args(l)?.get(at)?.as_i64()).map_or(String::new(), stamp);
            if let Some(r) = first(&m.handed, l) {
                handed.push((a(r, "passage"), a(r, "to")));
            }
            if let Some(r) = first(&m.open, l) {
                // its own id where the package gives one (an imported thread keeps its number), else one past the highest
                let given = r.at("id").map(|_| a(r, "id")).filter(|id| !id.is_empty());
                let wanted = given.as_deref().and_then(|id| id.rsplit('-').next()?.parse::<u64>().ok()).filter(|n| !used.contains(n));
                let n = wanted.unwrap_or_else(|| used.last().map_or(1, |n| n + 1));
                used.insert(n);
                let id = given.filter(|_| wanted.is_some()).unwrap_or_else(|| format!("N-{n}"));
                let kind = r.at("kind").map(|_| a(r, "kind")).filter(|k| !k.is_empty()).unwrap_or_else(|| "thread".into());
                // `ctx.thread`: the engine's name for it, which his words are put on (seat.rs)
                threads.push((a(r, "thread"), json!({"id": id, "n": n, "kind": kind, "opener": who(a(r, "by")), "ctx": {"thread": a(r, "thread")},
                    "form": {"to": ""}, "title": a(r, "title"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 0, "spent": 0,
                    "opened_at": time(r), "given": [], "posts": []})));
            }
            if let Some(r) = first(&m.end, l)
                && let Some(i) = at(&threads, &a(r, "thread"))
            {
                threads[i].1["status"] = json!({"is": "done", "result": {"head": a(r, "how")}});
                if r.at("at").is_some() {
                    threads[i].1["closed_at"] = json!(time(r));
                }
            }
            if let Some(r) = first(&m.post, l) {
                let (node, by) = (a(r, "passage"), r.at("by").map_or(HIM.to_string(), |_| who(a(r, "by"))));
                let Some(i) = at(&threads, &a(r, "thread")) else { continue };
                let secs = r.at("at").and_then(|at| args(l)?.get(at)?.as_i64()).or_else(|| written.get(&node).copied());
                let when = secs.map_or(String::new(), stamp);
                let t = &mut threads[i].1;
                let to = said_to.get(&node).cloned();
                if let (Some(to), true) = (&to, t["form"]["to"] == "") {
                    t["form"]["to"] = json!(to);
                }
                if t["opened_at"] == "" {
                    t["opened_at"] = json!(when);
                }
                let posts = t["posts"].as_array_mut().expect("made with posts");
                placed.insert(node.clone(), (i, posts.len() + 1));
                let mut p = json!({"n": posts.len() + 1, "at": when, "by": by, "text": texts.get(&node).cloned().unwrap_or_default(), "names": to.into_iter().collect::<Vec<_>>()});
                if to_owner.contains(&node) {
                    p["told"] = json!([HIM]);
                }
                posts.push(p);
            }
        }
        // a handing is a `given` row of the thread, carrying his posts up to that one
        for (node, to) in handed {
            let Some(&(i, n)) = placed.get(&node) else { continue };
            let at = threads[i].1["posts"][n - 1]["at"].clone();
            threads[i].1["given"].as_array_mut().expect("made with given").push(json!({"to": to, "delivery": "", "upto": n, "at": at}));
        }
        threads.into_iter().filter_map(|(_, t)| serde_json::from_value::<TicketView>(t).ok()).map(|t| (t.n, t)).collect()
    }
}

/// Seconds since 1970 as the thread pages' time.
fn stamp(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map_or(String::new(), |t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// The first of `rules` that `line` performed.
fn first<'a>(rules: &'a [Rule], line: &Json) -> Option<&'a Rule> {
    rules.iter().find(|r| is(line, r))
}

/// The arguments of a `performed` line.
fn args(line: &Json) -> Option<&Vec<Json>> {
    line.get("performed")?.get("act")?.get("args")?.as_array()
}

/// Whether `line` performed rule `r`'s declaration.
fn is(line: &Json, r: &Rule) -> bool {
    line.get("performed").and_then(|p| p.get("act")?.get("inst")?.as_str()) == Some(r.name.as_str())
}

/// The argument of role `role` of rule `r`, where `line` performed it.
fn arg(line: &Json, r: &Rule, role: &str) -> Option<String> {
    is(line, r).then(|| args(line)?.get(r.at(role)?).map(word)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(ty: &str, args: Json) -> Json {
        json!({"performed": {"act": {"inst": ty, "args": args}, "party": null, "run": null}})
    }

    /// An act's refusal is said while it is the latest line of that act, and not once the act is performed after it.
    #[test]
    fn a_refusal_is_said_till_the_act_is_performed_again() {
        let refused = |party: &str, act: &str| json!({"refused": {"act": {"inst": act, "args": [party]}, "party": party, "run": "r1", "why": {"policy": "not the owner"}}});
        let mut d = Desk::default();
        d.take(0, act("a.Set", json!(["o1"])));
        d.take(1, refused("w1", "a.Set"));
        d.take(2, refused("w1", "a.Other"));
        let said = Refusal { act: "a.Set".into(), party: "w1".into(), why: "policy: not the owner".into() };
        assert_eq!(d.refused(&["a.Set"]), Some(said), "the latest line of a.Set is its refusal");
        d.take(3, act("a.Set", json!(["o1"])));
        assert_eq!(d.refused(&["a.Set"]), None, "performed since");
    }

    /// A question waits until its answer's line comes, or the run that asked is cancelled; only the Effects named.
    #[test]
    fn a_question_waits_till_it_is_answered_or_its_run_is_cancelled() {
        let ask = |op: &str, run: &str| json!({"effect": {"op": op, "args": ["w1", "a note"], "run": run, "head": "", "key": ""}});
        let lines = [ask("p.Ask", "r1"), ask("p.Ask", "r2"), ask("p.Other", "r3"), ask("p.Ask", "r4"), json!({"answered": {"line": 0, "value": "x"}}), json!({"cancelled": {"run": "r4", "by": "r9"}})];
        let mut d = Desk::default();
        for (at, l) in lines.into_iter().enumerate() {
            d.take(at as u64, l);
        }
        assert_eq!(d.waiting(&["p.Ask"]), [Waiting { line: 1, op: "p.Ask".into(), args: vec![json!("w1"), json!("a note")], run: "r2".into() }]);
    }

    /// A made-up package's threads file: its own names, his posts with their time in an argument.
    const MAP: &str = "owner = { name = \"p.Seat\", who = 0 }\nwords = { name = \"p.Words\", passage = 0 }\nopen = [{ name = \"p.Open\", by = 0, thread = 1, title = 2 }, { name = \"p.Was\", thread = 0, by = 1, title = 2, at = 3, id = 4, kind = 5 }]\n\
        said_to = { name = \"p.Says\", passage = 0, to = 1 }\ntold = [{ name = \"p.Told\", passage = 1 }, { name = \"p.Him\", passage = 0 }]\nwritten = { name = \"p.When\", passage = 0, at = 1 }\nhanded = { name = \"p.Handed\", passage = 0, to = 1 }\n\
        [[post]]\nname = \"p.His\"\npassage = 0\nthread = 1\nat = 2\n[[post]]\nname = \"p.Put\"\nby = 0\npassage = 1\nthread = 2\n[[post]]\nname = \"p.Him\"\npassage = 0\nby = 1\nthread = 2\n";

    /// A thread, his words on it and an agent's reply, read from the record's lines by the package's threads file.
    #[test]
    fn a_thread_his_words_and_a_reply_are_read_from_the_record_by_the_threads_file() {
        let m: Map = toml::from_str(MAP).unwrap();
        let words = |n: &str| json!({"effect": {"op": "p.Words", "args": [n], "run": "r", "head": "", "key": ""}});
        let lines = [
            json!({"loaded": "ab"}),
            act("p.Seat", json!(["o1"])),
            act("p.Open", json!(["o1", "t1", "the outline"])),
            act("p.Says", json!(["h1", "w1"])),
            words("h1"),
            json!({"answered": {"line": 4, "value": "keep it small"}}),
            act("p.His", json!(["h1", "t1", 1_795_000_000])),
            words("n1"),
            json!({"answered": {"line": 7, "value": "two levels at most"}}),
            act("p.Handed", json!(["h1", "w1"])),
            act("p.When", json!(["n1", 1_795_000_060])),
            act("p.Put", json!(["w1", "n1", "t1"])),
            act("p.Told", json!(["w1", "n1"])),
            act("p.Put", json!(["w1", "n9", "t9"])),
            act("p.Was", json!(["t2", "w2", "an old one", 1_700_000_000, "N-1446", "item"])),
            act("p.Open", json!(["o1", "t3", "a new one after"])),
            act("p.Him", json!(["n5", "w2", "t2"])),
        ];
        let mut d = Desk::default();
        for (at, l) in lines.into_iter().enumerate() {
            assert!(d.take(at as u64, l));
        }
        assert!(!d.take(0, json!({})), "a line already held is not taken twice");
        let ts = d.tickets(&m);
        assert_eq!(ts.len(), 3, "a post on a thread nobody opened goes nowhere");
        assert_eq!(ts[&1447].title, "a new one after", "a new thread after an import follows the highest number, not a gap");
        let old = &ts[&1446];
        assert_eq!((old.id.as_str(), old.kind.as_str()), ("N-1446", "item"), "an imported thread keeps its id and kind");
        assert_eq!((old.title.as_str(), old.opened_at.to_string(), old.posts[0].by.as_str(), old.posts[0].told.clone()),
            ("an old one", "2023-11-14T22:13:20Z".to_string(), "w2", vec!["owner".to_string()]), "a list's second rule, and one line both said to him and a post");
        let t = &ts[&1];
        assert_eq!((t.id.as_str(), t.title.as_str(), t.opener.as_deref(), &t.ctx["thread"]), ("N-1", "the outline", Some("owner"), &json!("t1")));
        let posts: Vec<(String, Json, Vec<String>, String)> = t.posts.iter().map(|p| (p.by.clone(), p.text.value(), p.names.clone(), p.at.clone())).collect();
        assert_eq!(posts, [
            ("owner".into(), json!("keep it small"), vec!["w1".into()], "2026-11-18T11:06:40Z".into()),
            ("w1".into(), json!("two levels at most"), vec![], "2026-11-18T11:07:40Z".into()),
        ], "a time only where the package's acts give one");
        assert_eq!(t.posts[1].told, ["owner"]);
        assert_eq!((t.given.len(), t.given[0].to.as_str(), t.given[0].hands(1), t.given[0].hands(2)), (1, "w1", true, false), "his post handed to w1");
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "v2".into(), id: "v2".into(), boxed: false }, Vec::new());
        p.live.tickets = ts.clone();
        let w = crate::whereabouts::of(&p, t, &t.posts[0]).expect("his post");
        assert_eq!(w.entries[0].is, crate::whereabouts::Is::Answered { post: 2 }, "the reply follows the handing: not waiting");
        let untimed: Map = toml::from_str(&MAP.replace("p.When", "p.Never")).unwrap();
        assert_eq!(d.tickets(&untimed)[&1].posts[1].at, "", "no time where no act gives one");
        let other: Map = toml::from_str(&MAP.replace("p.Open", "p.Begin")).unwrap();
        assert_eq!(d.tickets(&other).len(), 1, "the threads are what the file names, not what the window knows");
    }
}
