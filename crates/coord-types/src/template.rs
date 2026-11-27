//! A page template (paper-research N-1446 p1779, item 6; the design p1785, read right p1789): the text a lens over the
//! one graph is drawn from, "a saved query with a renderer" (the owner, N-1104 p605), which each team writes for its
//! own structures (p607). It is a `[[view]]` of project.toml with `kind = "lens"`. Its `[view.ask]` is
//! `graph_slice::Ask` word for word, so there is no second query language; its other keys say which of the answer's
//! nodes it draws and how, with the window's own elements (window-rules §3, §4): rows under their counts, or the
//! counts alone.
//!
//! The vocabulary is closed, as the look's is (look.rs): a key the check does not know is refused, so a template
//! written for a newer window says so instead of being half drawn. One check, [`check`], with no IO: coordd's
//! text-change runs it before a template is written, and the window runs it on the project file its snapshot carries.
//!
//! **What a template never reaches** (p1789, 2: the stand-in that draws a change's still runs with his rights, and the
//! agent that changed it reads the still): no file, link or command (no `source` or `older`); no agent's folder or
//! session (`folder`, `session`: the file view's own, on his press), nor its system prompt (`prompt`, `version`: its
//! page's own); a walk of at most [`HOPS_MOST`] hops and [`LIMIT_MOST`] edges; and words filled only from [`FIELDS`]
//! ([`fields`]), never a post's or a block's text, which only his press opens (`Request::Raw`). So a post's label is
//! its writer alone.
//!
//! ```toml
//! [[view]]
//! id = "edge6-made"
//! kind = "lens"
//! title = "What docket-edge6 made this week"
//! draw = "rows"                       # rows | count
//! nodes = ["commit", "job", "build"]  # the answer's nodes it draws, by kind (graph.rs NodeKind); left out: every kind
//! group = "kind"                      # kind | by | ticket | day
//! order = "newest"                    # newest | oldest
//! words = "{label}"                   # a row's words: {id} {kind} {label} {ticket} {by} {when} {what} {ok}
//! within = "7d"                       # the ask's `from` is now less this when the page asks: m, h or d, 1m to 90d
//!
//! [view.ask]                          # graph_slice::Ask, word for word
//! around = "agent:docket-edge6"       # a node's id; left out, the whole graph, newest first
//! hops = 2                            # 0 to 3
//! layers = ["work"]                   # his, agents, deliveries, work, calls, files; left out: all
//! limit = 2000                        # 1 to 6000; left out, 2000
//! ```

use docket_machine::graph::NodeKind;
use docket_machine::graph_slice::{Ask, Layer, SliceNode};
use serde_json::{Value, json};

/// A template as checked: every part there, the built-in value where the text left it out.
#[derive(Clone, Debug, PartialEq)]
pub struct Template {
    pub draw: Draw,
    /// The kinds of node it draws; empty: every kind.
    pub nodes: Vec<NodeKind>,
    pub group: Group,
    pub order: Order,
    /// A row's words, its `{field}`s of [`FIELDS`] filled from the node.
    pub words: String,
    /// In seconds: the ask's `from` is the time the page asks less this.
    pub within: Option<u64>,
    pub ask: Ask,
}

impl Default for Template {
    fn default() -> Self {
        Template { draw: Draw::Rows, nodes: Vec::new(), group: Group::Kind, order: Order::Newest, words: "{label}".into(), within: None, ask: Ask::default() }
    }
}

impl Template {
    /// Whether it asks what OTHER asks: the same ask and span. Its other keys change how it is drawn, not what it asks.
    pub fn asks_as(&self, other: &Template) -> bool {
        self.ask == other.ask && self.within == other.within
    }
}

/// How the nodes are drawn: a row each under its group's count, or the counts alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Draw {
    Rows,
    Count,
}

/// What the rows are grouped by: the node's kind, its writer or agent (`by`), its thread, its day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Kind,
    By,
    Ticket,
    Day,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Newest,
    Oldest,
}

/// The fields a row's words take, each from the node or its label ([`fields`]).
pub const FIELDS: &[&str] = &["id", "kind", "label", "ticket", "by", "when", "what", "ok"];

/// The most hops a template's walk goes out.
pub const HOPS_MOST: u32 = 3;

/// The most edges it asks for: the cloud's own most (`NEAR_LIMIT`, map/cloud.rs).
pub const LIMIT_MOST: usize = 6000;

/// The longest span `within` names, in seconds: 90 days.
pub const WITHIN_MOST: u64 = 90 * 86_400;

/// The keys of a template, in the order the check names them.
const KEYS: &str = "id, kind, title, draw, nodes, group, order, words, within, ask";

/// Check the template V (its `[[view]]` table as JSON, or as TOML read into JSON): the template with the built-in
/// values where V left a key out, or every fault found, each naming its key.
pub fn check(v: &Value) -> Result<Template, Vec<String>> {
    let mut t = Template::default();
    let mut faults = Vec::new();
    let Some(top) = v.as_object() else {
        return Err(vec!["the template is not a table".into()]);
    };
    for (k, x) in top {
        match k.as_str() {
            "id" | "title" => {
                line(k, x, &mut faults);
            }
            "kind" if x.as_str() == Some("lens") => {}
            "kind" => faults.push(format!("`kind`: a template's kind is \"lens\", not {x}")),
            "draw" => t.draw = word("draw", x, &[("rows", Draw::Rows), ("count", Draw::Count)], &mut faults).unwrap_or(t.draw),
            "nodes" => t.nodes = names("nodes", x, "graph.rs NodeKind: post, ticket, agent, commit, job, build, …", &mut faults),
            "group" => t.group = word("group", x, &[("kind", Group::Kind), ("by", Group::By), ("ticket", Group::Ticket), ("day", Group::Day)], &mut faults).unwrap_or(t.group),
            "order" => t.order = word("order", x, &[("newest", Order::Newest), ("oldest", Order::Oldest)], &mut faults).unwrap_or(t.order),
            "words" => t.words = words(x, &mut faults).unwrap_or(t.words),
            "within" => t.within = within(x, &mut faults),
            "ask" => ask(x, &mut t.ask, &mut faults),
            "source" | "older" => faults.push(format!("`{k}`: a template runs nothing; a view with a command is not a template")),
            _ => faults.push(format!("`{k}`: no such key of a template (known: {KEYS})")),
        }
    }
    if t.within.is_some() && t.ask.from.is_some() {
        faults.push("`within` and `ask.from`: both say where the span starts; give one".into());
    }
    if faults.is_empty() { Ok(t) } else { Err(faults) }
}

/// The fields of node N, its label L as coordd gave it, that a row's words take ([`FIELDS`]), each a string, empty
/// where it has none; a node with no label is labelled by what its source says it is (`what`: a job's verb and
/// totals), else by its id. A post gives its writer alone: its label is "BY: its first line" (coordd's label.rs), and
/// a post's words come to the window only on his press (p1789, 2).
pub fn fields(n: &SliceNode, l: Option<&crate::Label>) -> Value {
    let node = &n.node;
    let kind = serde_json::to_value(node.kind).ok().and_then(|k| k.as_str().map(String::from)).unwrap_or_default();
    let post = node.kind == NodeKind::Post;
    let label = match (post, l.map(|l| l.label.as_str()).filter(|w| !w.is_empty())) {
        (true, _) => n.by.clone().unwrap_or_else(|| node.id.clone()),
        (false, Some(w)) => w.to_string(),
        (false, None) if !node.what.is_empty() => node.what.clone(),
        (false, None) => node.id.clone(),
    };
    let ok = match node.ok {
        Some(true) => "ok",
        Some(false) => "failed",
        None => "",
    };
    json!({
        "id": node.id,
        "kind": kind,
        "label": label,
        "ticket": l.and_then(|l| l.on.clone()).unwrap_or_default(),
        "by": n.by.clone().unwrap_or_default(),
        "when": node.when.clone().unwrap_or_default(),
        "what": if post { String::new() } else { node.what.clone() },
        "ok": ok,
    })
}

/// The longest a template's line of text may be (its title, its words), in characters: the look's own most.
const LINE_MOST: usize = crate::look::WORDS_MOST;

/// X as one line of plain text, at most [`LINE_MOST`] characters.
fn line(key: &str, x: &Value, faults: &mut Vec<String>) -> Option<String> {
    let Some(s) = x.as_str() else {
        faults.push(format!("`{key}`: a line of text is wanted, not {x}"));
        return None;
    };
    if s.chars().any(char::is_control) || s.chars().count() > LINE_MOST {
        faults.push(format!("`{key}`: one line of at most {LINE_MOST} characters is wanted"));
        return None;
    }
    Some(s.to_string())
}

/// X as one of WORDS' words, its value.
fn word<T: Copy>(key: &str, x: &Value, words: &[(&str, T)], faults: &mut Vec<String>) -> Option<T> {
    let found = x.as_str().and_then(|w| words.iter().find(|(n, _)| *n == w)).map(|(_, v)| *v);
    if found.is_none() {
        let known: Vec<&str> = words.iter().map(|(n, _)| *n).collect();
        faults.push(format!("`{key}`: {x} is not one of {}", known.join(", ")));
    }
    found
}

/// X as a list of names a serde enum E takes (snake_case), none of them its `other` (a kind this build does not know).
fn named<E: serde::de::DeserializeOwned + PartialEq>(key: &str, x: &Value, other: E, known: &str, faults: &mut Vec<String>) -> Vec<E> {
    let Some(a) = x.as_array() else {
        faults.push(format!("`{key}`: a list is wanted, not {x}"));
        return Vec::new();
    };
    let mut out = Vec::new();
    for n in a {
        match serde_json::from_value::<E>(n.clone()).ok().filter(|e| *e != other) {
            Some(e) => out.push(e),
            None => faults.push(format!("`{key}`: {n} is not a name it knows ({known})")),
        }
    }
    out
}

fn names(key: &str, x: &Value, known: &str, faults: &mut Vec<String>) -> Vec<NodeKind> {
    named(key, x, NodeKind::Other, known, faults)
}

/// The words of a row: one line whose every `{field}` is one of [`FIELDS`].
fn words(x: &Value, faults: &mut Vec<String>) -> Option<String> {
    let w = line("words", x, faults)?;
    let mut rest = w.as_str();
    let mut bad = Vec::new();
    while let Some(i) = rest.find(['{', '}']) {
        let after = &rest[i + 1..];
        match (rest.as_bytes()[i], after.find('}')) {
            (b'{', Some(j)) if FIELDS.contains(&&after[..j]) => rest = &after[j + 1..],
            (b'{', Some(j)) => {
                bad.push(format!("{{{}}}", &after[..j]));
                rest = &after[j + 1..];
            }
            _ => {
                faults.push("`words`: a `{` with no `}` after it, or a `}` with no `{`".into());
                return None;
            }
        }
    }
    if !bad.is_empty() {
        faults.push(format!("`words`: {} is not a field (known: {})", bad.join(", "), FIELDS.join(", ")));
        return None;
    }
    if w.trim().is_empty() {
        faults.push("`words`: a row has words".into());
        return None;
    }
    Some(w)
}

/// `within`: a number and its unit, m, h or d, from a minute to [`WITHIN_MOST`]; in seconds.
fn within(x: &Value, faults: &mut Vec<String>) -> Option<u64> {
    let secs = x.as_str().and_then(|s| {
        let unit = match s.chars().last()? {
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            _ => return None,
        };
        let n = s[..s.len() - 1].parse::<u64>().ok()?;
        n.checked_mul(unit).filter(|v| (60..=WITHIN_MOST).contains(v))
    });
    if secs.is_none() {
        faults.push(format!("`within`: {x} is not a span from 1m to 90d (a number, then m, h or d: \"7d\", \"36h\")"));
    }
    secs
}

/// `[view.ask]`: `graph_slice::Ask`'s own fields, but the folder's and what is worked on now, within bounds. `around`
/// names a node: "focus", the node the page is opened at, comes with cards (T2), and is refused until then (p1818, 1).
fn ask(v: &Value, a: &mut Ask, faults: &mut Vec<String>) {
    let Some(t) = v.as_object() else {
        faults.push(format!("`ask`: a table is wanted, not {v}"));
        return;
    };
    for (k, x) in t {
        let key = format!("ask.{k}");
        let number = |most: u64, least: u64, faults: &mut Vec<String>| match x.as_u64().filter(|n| (least..=most).contains(n)) {
            Some(n) => Some(n),
            None => {
                faults.push(format!("`{key}`: a whole number from {least} to {most} is wanted, not {x}"));
                None
            }
        };
        match k.as_str() {
            "layers" => a.layers = named(&key, x, Layer::Other, "his, agents, deliveries, work, calls, files", faults).into_iter().collect(),
            "around" => match x.as_str().map(str::trim) {
                Some(f) if f.eq_ignore_ascii_case("focus") => faults.push(format!("`{key}`: `focus` comes with cards (T2); name a node")),
                _ => a.around = line(&key, x, faults).filter(|s| !s.trim().is_empty()),
            },
            "hops" => a.hops = number(HOPS_MOST as u64, 0, faults).unwrap_or(0) as u32,
            "limit" => a.limit = number(LIMIT_MOST as u64, 1, faults).unwrap_or(0) as usize,
            "who" | "items" => {
                let list: Vec<String> = match x.as_array() {
                    Some(l) => l.iter().filter_map(|n| line(&key, n, faults)).collect(),
                    None => {
                        faults.push(format!("`{key}`: a list of names is wanted, not {x}"));
                        Vec::new()
                    }
                };
                if k == "who" { a.who = list } else { a.items = list }
            }
            "from" | "to" => {
                let at = x.as_str().filter(|s| s.len() <= 40 && s.as_bytes().get(..10).is_some_and(|d| d.iter().enumerate().all(|(i, b)| if i == 4 || i == 7 { *b == b'-' } else { b.is_ascii_digit() })));
                match at {
                    Some(s) if k == "from" => a.from = Some(s.to_string()),
                    Some(s) => a.to = Some(s.to_string()),
                    None => faults.push(format!("`{key}`: a record time is wanted (2026-11-22T10:00:00Z), not {x}")),
                }
            }
            "folder" | "session" => faults.push(format!("`{key}`: an agent's folder and its sessions are the file view's own, opened on his press; a template asks the graph's slice")),
            "prompt" | "version" => faults.push(format!("`{key}`: an agent's system prompt and its versions are its page's own, opened on his press; a template asks the graph's slice")),
            "now" => faults.push(format!("`{key}`: nothing in a template draws what is worked on now")),
            _ => faults.push(format!("`{key}`: no such field of the graph's ask (known: layers, around, hops, who, items, from, to, limit)")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use docket_machine::graph::{Node, Source};

    /// The design's example (p1785, 1), as project.toml's view reads into JSON.
    fn made() -> Value {
        json!({"id": "edge6-made", "kind": "lens", "title": "What docket-edge6 made this week", "draw": "rows", "nodes": ["commit", "job", "build"],
            "group": "kind", "order": "newest", "words": "{label}", "within": "7d",
            "ask": {"around": "agent:docket-edge6", "hops": 2, "layers": ["work"], "limit": 2000}})
    }

    fn with(key: &str, x: Value) -> Value {
        let mut v = made();
        match key.strip_prefix("ask.") {
            Some(k) => v["ask"][k] = x,
            None => v[key] = x,
        }
        v
    }

    fn faults(v: Value) -> String {
        check(&v).unwrap_err().join("; ")
    }

    #[test]
    fn the_designs_example_reads_into_the_graphs_own_ask() {
        let t = check(&made()).unwrap();
        let ask = Ask { layers: [Layer::Work].into_iter().collect(), around: Some("agent:docket-edge6".into()), hops: 2, limit: 2000, ..Default::default() };
        assert_eq!(t, Template { draw: Draw::Rows, nodes: vec![NodeKind::Commit, NodeKind::Job, NodeKind::Build], group: Group::Kind, order: Order::Newest, words: "{label}".into(), within: Some(7 * 86_400), ask });
        assert_eq!(check(&json!({"id": "all", "kind": "lens"})), Ok(Template::default()), "every key but id and kind may be left out");
        assert_eq!(check(&with("draw", json!("count"))).map(|t| t.draw), Ok(Draw::Count));
        assert_eq!(check(&with("within", json!("36h"))).map(|t| t.within), Ok(Some(36 * 3600)));
        assert_eq!(check(&with("words", json!("{what} · {when}, {ok}"))).map(|t| t.words), Ok("{what} · {when}, {ok}".into()));
    }

    /// p1789, 2: what a template never reaches is refused, each fault naming its key.
    #[test]
    fn a_folder_a_session_and_more_than_three_hops_are_refused() {
        for (key, x, says) in [
            ("ask.folder", json!("docket-edge6"), "`ask.folder`: an agent's folder and its sessions are the file view's own"),
            ("ask.session", json!("session:5c6f1a2b"), "`ask.session`: an agent's folder"),
            ("ask.prompt", json!("docket-edge6"), "`ask.prompt`: an agent's system prompt and its versions are its page's own"),
            ("ask.version", json!("text:0123456789ab"), "`ask.version`: an agent's system prompt"),
            ("ask.hops", json!(4), "`ask.hops`: a whole number from 0 to 3 is wanted, not 4"),
            ("ask.limit", json!(6001), "`ask.limit`: a whole number from 1 to 6000"),
            ("ask.limit", json!(0), "`ask.limit`: a whole number from 1 to 6000"),
            ("ask.now", json!(true), "`ask.now`: nothing in a template draws"),
            ("ask.depth", json!(1), "`ask.depth`: no such field of the graph's ask"),
            ("ask.layers", json!(["work", "secrets"]), "`ask.layers`: \"secrets\" is not a name it knows"),
            ("ask.from", json!("last week"), "`ask.from`: a record time is wanted"),
            ("source", json!("./made.sh"), "`source`: a template runs nothing"),
            ("older", json!("./o"), "`older`: a template runs nothing"),
            ("colour", json!("#ff0000"), "`colour`: no such key of a template"),
            ("kind", json!("board"), "`kind`: a template's kind is \"lens\""),
            ("nodes", json!(["commit", "secret"]), "`nodes`: \"secret\" is not a name it knows"),
            ("draw", json!("cards"), "`draw`: \"cards\" is not one of rows, count"),
            ("group", json!("hour"), "`group`: \"hour\" is not one of kind, by, ticket, day"),
            ("within", json!("91d"), "`within`: \"91d\" is not a span from 1m to 90d"),
            ("within", json!("7w"), "`within`: \"7w\" is not a span"),
            ("words", json!("{text}"), "`words`: {text} is not a field"),
            ("words", json!("{label"), "`words`: a `{` with no `}`"),
            ("words", json!("one\ntwo"), "`words`: one line of at most 200 characters"),
            ("title", json!("x".repeat(201)), "`title`: one line of at most 200 characters"),
        ] {
            let f = faults(with(key, x.clone()));
            assert!(f.contains(says), "{key} = {x}: {f}");
        }
        assert!(faults(with("ask.from", json!("2026-11-20T00:00:00Z"))).contains("`within` and `ask.from`"), "one start of the span");
        assert_eq!(check(&with("ask.hops", json!(3))).map(|t| t.ask.hops), Ok(3), "3 hops is the most, and taken");
    }

    /// p1818, 1: every word of the closed list does what it says; "focus", which T1 does not fill, is refused, and a
    /// node's own name with it in is taken.
    #[test]
    fn around_focus_is_refused_until_cards_fill_it() {
        for x in ["focus", " Focus "] {
            assert_eq!(check(&with("ask.around", json!(x))), Err(vec!["`ask.around`: `focus` comes with cards (T2); name a node".to_string()]), "{x:?}");
        }
        assert_eq!(check(&with("ask.around", json!("agent:focus"))).map(|t| t.ask.around), Ok(Some("agent:focus".into())), "a node's name");
    }

    /// p1789, 2: a row's words are the node's fields and its label, never a post's words.
    #[test]
    fn a_posts_fields_are_its_writer_never_its_words() {
        let node = |id: &str, kind: NodeKind, what: &str, ok: Option<bool>, by: Option<&str>| SliceNode {
            node: Node { id: id.into(), kind, source: Source::Line(7), when: Some("2026-11-22T10:00:00Z".into()), what: what.into(), ok, tokens: None },
            by: by.map(String::from),
        };
        let label = |id: &str, l: &str, on: Option<&str>| crate::Label { id: id.into(), is: "post".into(), label: l.into(), on: on.map(String::from) };
        let post = node("N-1104.p815", NodeKind::Post, "", None, Some("owner"));
        let f = fields(&post, Some(&label("N-1104.p815", "owner: made-up first words of his", Some("his chat"))));
        assert_eq!(f, json!({"id": "N-1104.p815", "kind": "post", "label": "owner", "ticket": "his chat", "by": "owner", "when": "2026-11-22T10:00:00Z", "what": "", "ok": ""}));
        assert!(!f.to_string().contains("made-up first words"), "{f}");
        let job = node("job:7-1", NodeKind::Job, "test coordd: 252 passed, 1 failed", Some(false), None);
        assert_eq!(fields(&job, None)["label"], "test coordd: 252 passed, 1 failed", "no label: what it is");
        assert_eq!(fields(&node("build:69", NodeKind::Build, "", None, None), None)["label"], "build:69", "nor that: its id");
        assert_eq!((fields(&job, None)["what"].as_str(), fields(&job, None)["ok"].as_str()), (Some("test coordd: 252 passed, 1 failed"), Some("failed")));
        let commit = node("commit:3b2c4ac5", NodeKind::Commit, "the read row", Some(true), None);
        assert_eq!(fields(&commit, Some(&label("commit:3b2c4ac5", "the read row's subject", None)))["label"], "the read row's subject");
    }
}
