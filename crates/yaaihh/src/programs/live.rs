//! What the engine says now, for the drawing (N-1988): what the record performed, refused and still waits on, kept as
//! each line comes; an instance written as the engine reads it, to ask `explain` of it; and an explanation (coordd-v2's
//! why.rs JSON, N-1921) as the lines it is drawn in.

use serde_json::Value as Json;
use std::collections::{BTreeMap, BTreeSet};

/// The act each harness moment is written as (std.hooks `serve`): its second field names the moment.
pub const REACH: &str = "std.hooks.Reach";
/// How many refusals are kept: the latest.
const REFUSALS: usize = 20;
/// How deep an explanation is drawn.
const DEPTH: usize = 12;

/// What the record's lines say of the drawing's things, each line taken once and in any order (desk.rs `take`).
#[derive(Default)]
pub struct Tally {
    /// Each Act and Event performed, by full name: how many times, and its latest line.
    pub performed: BTreeMap<String, (usize, u64)>,
    /// Each harness moment reached, by name: how many times, and its latest line.
    pub reached: BTreeMap<String, (usize, u64)>,
    /// Each act refused, by full name: how many times.
    pub refused: BTreeMap<String, usize>,
    /// The latest refusals, by line.
    pub refusals: BTreeMap<u64, Json>,
    /// The effect lines no answer has come to, by line: their Effect and run.
    open: BTreeMap<u64, (String, String)>,
    /// The lines answered before their effect line came.
    early: BTreeSet<u64>,
    cancelled: BTreeSet<String>,
}

/// A value's words: a text as it is, an instance of a type identified by a text (std.hooks' `Moment`) as that text.
fn word(v: &Json) -> Option<&str> {
    v.as_str().or_else(|| v.get("args")?.get(0)?.as_str())
}

fn bump(m: &mut BTreeMap<String, (usize, u64)>, k: &str, at: u64) {
    let e = m.entry(k.to_string()).or_insert((0, at));
    *e = (e.0 + 1, e.1.max(at));
}

impl Tally {
    /// Line LINE, at place AT.
    pub fn take(&mut self, at: u64, line: &Json) {
        if let Some(p) = line.get("performed") {
            let Some(act) = p["act"]["inst"].as_str() else { return };
            bump(&mut self.performed, act, at);
            if let Some(m) = (act == REACH).then(|| word(&p["act"]["args"][1])).flatten() {
                bump(&mut self.reached, m, at);
            }
        } else if let Some(r) = line.get("refused") {
            if let Some(act) = r["act"]["inst"].as_str() {
                *self.refused.entry(act.to_string()).or_default() += 1;
            }
            self.refusals.insert(at, r.clone());
            while self.refusals.len() > REFUSALS {
                self.refusals.pop_first();
            }
        } else if let Some(e) = line.get("effect") {
            let (Some(op), Some(run)) = (e["op"].as_str(), e["run"].as_str()) else { return };
            if !self.early.remove(&at) && !self.cancelled.contains(run) {
                self.open.insert(at, (op.to_string(), run.to_string()));
            }
        } else if let Some(n) = line.get("answered").and_then(|a| a["line"].as_u64()) {
            if self.open.remove(&n).is_none() {
                self.early.insert(n);
            }
        } else if let Some(run) = line.get("cancelled").and_then(|c| c["run"].as_str()) {
            self.open.retain(|_, (_, r)| r != run);
            self.cancelled.insert(run.to_string());
        }
    }

    /// How many steps wait on an answer to Effect OP.
    pub fn waiting(&self, op: &str) -> usize {
        self.open.values().filter(|(o, _)| o == op).count()
    }
}

/// A value as a Docket text writes it: none for one with no written form, or a text a hole would be read in.
fn literal(v: &Json, name: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    Some(match v {
        Json::String(t) if !t.contains("{{") => format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\t', "\\t")),
        Json::Number(n) if n.is_i64() => n.to_string(),
        Json::Bool(b) => b.to_string(),
        Json::Object(o) if o.contains_key("inst") => written(v, name)?,
        _ => return None,
    })
}

/// Instance I (`{"inst": NAME, "args": [...]}`, as `holding` gives each) as the engine reads it, NAME given its name in
/// the module queries are read in (`name`).
pub fn written(i: &Json, name: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let ty = name(i.get("inst")?.as_str()?)?;
    let args = i.get("args")?.as_array()?.iter().map(|a| literal(a, name)).collect::<Option<Vec<_>>>()?;
    Some(format!("{ty}({})", args.join(", ")))
}

/// Instance I in words: its type's own name and its values.
pub fn words(i: &Json) -> String {
    let ty = i["inst"].as_str().map_or("?", super::read::short);
    let args: Vec<String> = i["args"].as_array().into_iter().flatten().map(|a| if a.get("inst").is_some() { words(a) } else { a.to_string() }).collect();
    format!("{ty}({})", args.join(", "))
}

/// One line of an explanation: how deep it stands, whether what it says holds (none: not a truth), its words, the
/// place in the texts it is written (`module:line:col`).
#[derive(Clone, Debug, PartialEq)]
pub struct Said {
    pub depth: usize,
    pub holds: Option<bool>,
    pub words: String,
    pub at: Option<String>,
}

/// Explanation E as its lines, depth first: an `explain` answer, or the `why` of a refused act (a `refused` line's
/// `policy`).
pub fn said(e: &Json) -> Vec<Said> {
    let mut out = Vec::new();
    if e.get("failing").is_some() {
        refused(e, 0, &mut out);
    } else {
        explain(e, 0, &mut out);
    }
    out
}

fn line(out: &mut Vec<Said>, depth: usize, holds: Option<bool>, words: String, at: Option<&Json>) {
    out.push(Said { depth, holds, words, at: at.and_then(Json::as_str).map(str::to_string) });
}

/// Bindings `{x: v}` in words: `x = v`, by name.
fn bound(b: &Json) -> String {
    let o = b.as_object().map(|o| o.iter().map(|(k, v)| format!("{k} = {}", if v.get("inst").is_some() { words(v) } else { v.to_string() })).collect::<Vec<_>>()).unwrap_or_default();
    if o.is_empty() { String::new() } else { format!("  [{}]", o.join(", ")) }
}

fn done(d: &Json) -> String {
    match (d["step"].as_u64(), d["act"].as_str()) {
        (Some(0), _) | (None, _) => "before any step this engine saw".into(),
        (Some(n), Some(a)) => format!("at step {n}, by {a}"),
        (Some(n), None) => format!("at step {n}"),
    }
}

fn deeper(out: &mut Vec<Said>, depth: usize) -> bool {
    if depth > DEPTH {
        line(out, depth, None, "… deeper, not drawn".into(), None);
        return true;
    }
    false
}

fn explain(e: &Json, d: usize, out: &mut Vec<Said>) {
    if deeper(out, d) {
        return;
    }
    let inst = e["inst"].as_str().unwrap_or("?");
    let holds = e["holds"].as_bool();
    let verb = match (holds, e.get("on_path").is_some()) {
        (_, true) => "is explained above, on this path",
        (Some(true), _) => "holds",
        (Some(false), _) => "does not hold",
        (None, _) => "is unknown: the well-founded reading leaves it so",
    };
    line(out, d, holds, format!("{inst} {verb}"), None);
    if let Some(by) = e.get("by") {
        by_rule(by, d + 1, out);
    }
    if let Some(x) = e.get("ended") {
        line(out, d + 1, Some(false), format!("ended {}", done(x)), None);
    }
    if let Some(o) = e.get("overridden") {
        line(out, d + 1, None, "it would hold, by:".into(), None);
        by_rule(o, d + 2, out);
    }
    if let Some(rs) = e.get("rules").and_then(Json::as_array) {
        if rs.is_empty() && e.get("ended").is_none() {
            line(out, d + 1, Some(false), "no rule gives it, and no step stated it".into(), None);
        }
        for r in rs {
            rule(r, d + 1, out);
        }
    }
    for l in e.get("cycle").and_then(Json::as_array).into_iter().flatten() {
        let not = if l["not"].as_bool() == Some(true) { " through Not" } else { "" };
        line(out, d + 1, None, format!("{} reads itself{not}: {}", l["inst"].as_str().unwrap_or("?"), l["rule"].as_str().unwrap_or("")), l.get("at"));
    }
}

fn by_rule(h: &Json, d: usize, out: &mut Vec<Said>) {
    if let Some(x) = h.get("stated") {
        line(out, d, Some(true), format!("stated {}", done(x)), None);
    } else if let Some(at) = h.get("postulated") {
        line(out, d, Some(true), "stated by its module".into(), Some(at));
    } else if let Some(r) = h.get("rule") {
        rule(r, d, out);
    } else if let Some(u) = h.get("unproved").and_then(Json::as_str) {
        line(out, d, None, u.to_string(), None);
    }
}

fn rule(r: &Json, d: usize, out: &mut Vec<Said>) {
    if deeper(out, d) {
        return;
    }
    line(out, d, None, format!("by {}{}", r["rule"].as_str().unwrap_or("its rule"), bound(&r["bindings"])), r.get("at"));
    for p in r["premises"].as_array().into_iter().flatten() {
        premise(p, d + 1, out);
    }
    if r["cut"].as_bool() == Some(true) {
        line(out, d + 1, None, "the explanation's budget ran out before every premise was asked".into(), None);
    }
}

fn premise(p: &Json, d: usize, out: &mut Vec<Said>) {
    if deeper(out, d) {
        return;
    }
    line(out, d, p["holds"].as_bool(), format!("{}{}", p["text"].as_str().unwrap_or("?"), bound(&p["bindings"])), p.get("at"));
    for q in p.get("parts").and_then(Json::as_array).into_iter().flatten() {
        premise(q, d + 1, out);
    }
    for f in p.get("facts").and_then(Json::as_array).into_iter().flatten() {
        explain(f, d + 1, out);
    }
    if p["cut"].as_bool() == Some(true) {
        line(out, d + 1, None, "the explanation's budget ran out here".into(), None);
    }
}

fn refused(w: &Json, d: usize, out: &mut Vec<Said>) {
    if deeper(out, d) {
        return;
    }
    let act = w["act"].as_str().unwrap_or("?");
    let (failing, syncs) = (w["failing"].as_array().cloned().unwrap_or_default(), w["syncs"].as_array().cloned().unwrap_or_default());
    let again = if w["again"].as_bool() == Some(true) { ", as explained above" } else { "" };
    if failing.is_empty() && syncs.is_empty() {
        line(out, d, Some(true), format!("{act} is enabled{again}"), None);
        return;
    }
    line(out, d, Some(false), format!("{act} is not enabled{again}"), None);
    for p in &failing {
        premise(p, d + 1, out);
    }
    for s in &syncs {
        refused(s, d + 1, out);
    }
    if w["cut"].as_bool() == Some(true) {
        line(out, d + 1, None, "the explanation's budget ran out here".into(), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn act(ty: &str, args: Json) -> Json {
        json!({"performed": {"act": {"inst": ty, "args": args}, "party": "w1", "run": null}})
    }

    /// Each line once, in any order: what came, what was refused, and what waits until its answer or its run's end.
    #[test]
    fn the_tally_counts_each_line_once_in_any_order() {
        let lines = [
            (3, act(REACH, json!(["w1", "turn_begins"]))),
            (1, act(REACH, json!(["w1", {"inst": "std.hooks.Moment", "args": ["turn_begins"]}]))),
            (2, act("std.comms.Spoke", json!(["w1", "n1"]))),
            (5, json!({"answered": {"line": 4, "value": "x"}})),
            (4, json!({"effect": {"op": "std.comms.TextOf", "args": ["n1"], "run": "r1", "head": "", "key": ""}})),
            (6, json!({"effect": {"op": "std.comms.TextOf", "args": ["n2"], "run": "r2", "head": "", "key": ""}})),
            (7, json!({"effect": {"op": "std.comms.TextOf", "args": ["n3"], "run": "r3", "head": "", "key": ""}})),
            (8, json!({"cancelled": {"run": "r3", "by": "o1"}})),
            (9, json!({"refused": {"act": {"inst": "std.comms.Hear", "args": ["w2", "n1"]}, "party": "w2", "run": null, "why": "source"}})),
        ];
        let mut t = Tally::default();
        for (at, l) in &lines {
            t.take(*at, l);
        }
        assert_eq!(t.reached.get("turn_begins"), Some(&(2, 3)));
        assert_eq!(t.performed.get(REACH), Some(&(2, 3)));
        assert_eq!((t.performed.get("std.comms.Spoke"), t.refused.get("std.comms.Hear")), (Some(&(1, 2)), Some(&1)));
        assert_eq!(t.waiting("std.comms.TextOf"), 1, "n1 was answered (its answer came first), n3's run cancelled");
        for at in 100..130 {
            t.take(at, &json!({"refused": {"act": {"inst": "x.A", "args": []}, "party": null, "run": null, "why": "source"}}));
        }
        assert_eq!((t.refusals.len(), t.refusals.keys().next()), (REFUSALS, Some(&110)), "only the latest are kept");
    }

    /// An instance is written in the query module's names, its texts escaped; one with a hole's marks is not written.
    #[test]
    fn an_instance_is_written_as_the_engine_reads_it() {
        let name = |q: &str| q.strip_prefix("std.attention.").map(|n| format!("A.{n}"));
        let i = json!({"inst": "std.attention.Neglected", "args": ["w\"1", 3, true]});
        assert_eq!(written(&i, &name).as_deref(), Some("A.Neglected(\"w\\\"1\", 3, true)"));
        assert_eq!(written(&json!({"inst": "std.attention.Neglected", "args": ["{{ x }}"]}), &name), None);
        assert_eq!(written(&json!({"inst": "other.Fact", "args": []}), &name), None, "a module the query module does not import");
        assert_eq!(words(&i), "Neglected(\"w\\\"1\", 3, true)");
    }

    /// An explanation reads as its tree: what holds by which rule, each premise and the facts under it, each with its
    /// place; a refused act's failing condition the same.
    #[test]
    fn an_explanation_reads_as_its_tree() {
        let e = json!({"inst": "a.May(\"w1\")", "holds": true, "by": {"rule": {"at": "a:4:3", "rule": "a.Member(p)", "bindings": {"p": "w1"}, "premises": [
            {"at": "a:4:14", "text": "a.Member(p)", "bindings": {"p": "w1"}, "holds": true, "facts": [{"inst": "a.Member(\"w1\")", "holds": true, "by": {"stated": {"step": 2, "act": "a.Join(\"w1\")"}}}]}
        ]}}});
        let lines: Vec<(usize, Option<bool>, String, Option<String>)> = said(&e).into_iter().map(|s| (s.depth, s.holds, s.words, s.at)).collect();
        assert_eq!(lines, [
            (0, Some(true), "a.May(\"w1\") holds".to_string(), None),
            (1, None, "by a.Member(p)  [p = \"w1\"]".into(), Some("a:4:3".into())),
            (2, Some(true), "a.Member(p)  [p = \"w1\"]".into(), Some("a:4:14".into())),
            (3, Some(true), "a.Member(\"w1\") holds".into(), None),
            (4, Some(true), "stated at step 2, by a.Join(\"w1\")".into(), None),
        ]);
        let w = json!({"act": "a.Borrow(\"w2\")", "failing": [{"at": "a:9:5", "text": "a.Member(by)", "bindings": {}, "holds": false}], "syncs": []});
        let r: Vec<String> = said(&w).into_iter().map(|s| s.words).collect();
        assert_eq!(r, ["a.Borrow(\"w2\") is not enabled", "a.Member(by)"]);
        let unknown = said(&json!({"inst": "a.P(1)", "holds": null, "cycle": [{"inst": "a.P(1)", "at": "a:2:1", "rule": "Not(a.P(x))", "not": true}]}));
        assert_eq!((unknown[0].holds, unknown[1].at.as_deref()), (None, Some("a:2:1")));
    }
}
