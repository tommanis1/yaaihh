//! Which declarations of a package make the window's threads: its threads file, checked against the loaded text.

use docket_v2::check::names::Sym;
use docket_v2::check::Loaded;
use docket_v2::syntax::tree::{Fields, Norm};
use serde::Deserialize;
use std::collections::BTreeMap;

/// One declaration by its qualified name, and the place of each argument the window reads, by its role.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Rule {
    pub name: String,
    #[serde(flatten)]
    pub places: BTreeMap<String, usize>,
}

impl Rule {
    /// The place of role `role`, which the check has made sure of where it is required.
    pub fn at(&self, role: &str) -> Option<usize> {
        self.places.get(role).copied()
    }
}

/// The threads file: the owner's seat, the Effect giving a passage's words, a thread's opening and end, who a passage
/// is said to, a passage said to the owner, and each way a passage is put on a thread.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Map {
    pub owner: Rule,
    pub words: Rule,
    #[serde(deserialize_with = "many")]
    pub open: Vec<Rule>,
    #[serde(default, deserialize_with = "many")]
    pub end: Vec<Rule>,
    #[serde(default, deserialize_with = "many")]
    pub said_to: Vec<Rule>,
    #[serde(default, deserialize_with = "many")]
    pub told: Vec<Rule>,
    /// Acts or Events giving a passage's time in seconds, as the import's and a live post's both may.
    #[serde(default, deserialize_with = "many")]
    pub written: Vec<Rule>,
    /// Acts or Events saying a passage of his was handed to a party: his post's line then follows it to the reply.
    #[serde(default, deserialize_with = "many")]
    pub handed: Vec<Rule>,
    #[serde(default, deserialize_with = "many")]
    pub post: Vec<Rule>,
}

/// One rule written as a table, or several as a list of tables.
fn many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Rule>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        One(Rule),
        Many(Vec<Rule>),
    }
    Ok(match Written::deserialize(d)? {
        Written::One(r) => vec![r],
        Written::Many(rs) => rs,
    })
}

/// The roles each rule must place, then those it may.
fn roles(kind: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match kind {
        "owner" => (&["who"], &[]),
        "words" => (&["passage"], &[]),
        "open" => (&["by", "thread", "title"], &["at", "id", "kind"]),
        "end" => (&["thread", "how"], &["at"]),
        "said_to" => (&["passage", "to"], &[]),
        "told" => (&["passage"], &[]),
        "written" => (&["passage", "at"], &[]),
        "handed" => (&["passage", "to"], &[]),
        _ => (&["passage", "thread"], &["by", "at"]),
    }
}

impl Map {
    /// The file's text read, then checked against `loaded`: each fault said, by rule and name.
    pub fn read(text: &str, loaded: &Loaded) -> Result<Map, Vec<String>> {
        let m: Map = toml::from_str(text).map_err(|e| vec![e.to_string()])?;
        let mut faults = Vec::new();
        let lists = [("open", &m.open), ("end", &m.end), ("said_to", &m.said_to), ("told", &m.told), ("written", &m.written), ("handed", &m.handed), ("post", &m.post)];
        let rules = [("owner", &m.owner), ("words", &m.words)].into_iter().chain(lists.into_iter().flat_map(|(k, rs)| rs.iter().map(move |r| (k, r))));
        for (kind, r) in rules {
            faults.extend(check(kind, r, loaded));
        }
        if m.open.is_empty() {
            faults.push("open: no declaration opens a thread".into());
        }
        if faults.is_empty() { Ok(m) } else { Err(faults) }
    }
}

/// What is wrong with rule `r` of kind `kind` in `loaded`: a name not declared or of the wrong kind, a role missing or
/// unknown, a place past the declaration's arguments.
fn check(kind: &str, r: &Rule, loaded: &Loaded) -> Vec<String> {
    let want = if kind == "words" { "an Effect" } else { "an Act or Event" };
    let n = match loaded.syms.get(&r.name) {
        Some(Sym::Effect { fields, .. }) if kind == "words" => fields.len(),
        Some(Sym::Norm { kind: Norm::Act | Norm::Event, fields, .. }) if kind != "words" => match fields {
            Fields::Named(f) => f.len(),
            Fields::By(_) => 1,
        },
        _ => return vec![format!("{kind}: the loaded text declares no {} as {want}", r.name)],
    };
    let (must, may) = roles(kind);
    let mut out: Vec<String> = must.iter().filter(|role| !r.places.contains_key(**role)).map(|role| format!("{kind} {}: no place for `{role}`", r.name)).collect();
    for (role, at) in &r.places {
        if !must.contains(&role.as_str()) && !may.contains(&role.as_str()) {
            out.push(format!("{kind} {}: `{role}` is no role here", r.name));
        } else if *at >= n {
            out.push(format!("{kind} {}: `{role}` at {at}, and it has {n} arguments", r.name));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded() -> Loaded {
        let text = "module t\nEffect Words(id: String) -> String\nAct Open(by: String, t: String, title: String)\nEvent Put(n: String, t: String)\nAct Seat(who: String)";
        docket_v2::check::load(vec![docket_v2::syntax::read(text).unwrap()]).unwrap()
    }

    const GOOD: &str = "owner = { name = \"t.Seat\", who = 0 }\nwords = { name = \"t.Words\", passage = 0 }\nopen = { name = \"t.Open\", by = 0, thread = 1, title = 2 }\n[[post]]\nname = \"t.Put\"\npassage = 0\nthread = 1\n";

    #[test]
    fn a_threads_file_is_checked_against_the_loaded_text() {
        let m = Map::read(GOOD, &loaded()).unwrap();
        assert_eq!((m.post[0].at("thread"), m.post[0].at("by")), (Some(1), None));
        let bad = GOOD.replace("t.Put", "t.Gone").replace("title = 2", "title = 3").replace("t.Words", "t.Open");
        let faults = Map::read(&bad, &loaded()).unwrap_err();
        assert_eq!(faults, [
            "words: the loaded text declares no t.Open as an Effect",
            "open t.Open: `title` at 3, and it has 3 arguments",
            "post: the loaded text declares no t.Gone as an Act or Event",
        ]);
        let faults = Map::read(&GOOD.replace("thread = 1\n", "thread = 1\nwhen = 0\n"), &loaded()).unwrap_err();
        assert_eq!(faults, ["post t.Put: `when` is no role here"]);
    }
}
