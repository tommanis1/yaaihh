//! Home and Overview's machine state for the v2 project, which has no coordd of v1 to ask: each page's question put to
//! the machine's reader (`v2::machine`), and each new answer handed to the page's own `came`.

use crate::App;
use serde_json::{Value, json};
use std::collections::HashMap;

/// What an answer is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Ask {
    Store,
    Page(String),
    Machines,
    Repos(String),
}

/// The questions the pages want answered, and the answer each was last handed.
#[derive(Default)]
pub struct Reader {
    wanted: Vec<(Ask, Value)>,
    handed: HashMap<String, Value>,
}

/// Whether project PI is the v2 project and the projects file names its reader.
pub fn reads(s: &App, pi: usize) -> bool {
    s.ext.try_get::<crate::v2::V2>().is_some_and(|v| v.at == Some(pi) && v.config.as_ref().is_some_and(|c| !c.reader.is_empty()))
}

/// The repositories the v2 project names (`[v2] repos`), as the reader's `repos` takes them.
fn repos(s: &App) -> Vec<(String, String)> {
    let c = s.ext.try_get::<crate::v2::V2>().and_then(|v| v.config.as_ref());
    c.map(|c| c.repos.iter().map(|(n, f)| (n.clone(), f.clone())).collect()).unwrap_or_default()
}

/// ASK wanted from now on, FROM a store page's first entry; a new page of the store replaces the last; asked again.
pub fn ask(s: &mut App, ask: Ask, from: u64) {
    let q = match &ask {
        Ask::Store => json!({"store": {"row": null, "from": 0}}),
        Ask::Page(row) => json!({"store": {"row": row, "from": from}}),
        Ask::Machines => json!({"machines": {}}),
        Ask::Repos(_) => json!({"repos": repos(s)}),
    };
    let w = &mut s.machine.wanted;
    w.retain(|(a, _)| !(*a == ask || matches!((a, &ask), (Ask::Page(_), Ask::Page(_)))));
    w.push((ask, q));
    crate::v2::machine_again(s);
}

/// The answers new since the last frame, each to its page, the reader's `{"error"}` as a refusal; true when one came.
pub fn came(s: &mut App) -> bool {
    let fresh: Vec<(Ask, Value)> = s.machine.wanted.iter().filter_map(|(a, q)| Some((a.clone(), crate::v2::machine(s, q)?))).filter(|(a, v)| s.machine.handed.get(&key(a)) != Some(v)).collect();
    for (ask, v) in &fresh {
        s.machine.handed.insert(key(ask), v.clone());
        let r = match v.get("error").and_then(Value::as_str) {
            Some(e) => Err(e.to_string()),
            None => Ok(v.to_string()),
        };
        match ask.clone() {
            Ask::Store => crate::store::came(s, r),
            Ask::Page(row) => crate::store::paged(s, &row, r),
            Ask::Machines => crate::devices::came(s, r),
            Ask::Repos(id) => crate::home::repo_state_came(s, id, r),
        }
    }
    !fresh.is_empty()
}

fn key(a: &Ask) -> String {
    format!("{a:?}")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// On the v2 project each page's ask goes to the reader with the repositories its file names; each answer reaches
    /// the page as coordd's would, the reader's error as a refusal, and an answer already handed is not handed again.
    #[test]
    fn each_ask_goes_to_the_reader_and_its_answer_to_its_page() {
        let (_held, mut s) = crate::home::tests::made();
        let script = r#"while IFS= read -r q; do case "$q" in *o1-paper*) echo '[{"name":"o1-paper","path":"/srv/o1","branch":"main","dirty":0}]';; *machines*) echo '{"error":"no devices file"}';; *) echo '{"rows":[]}';; esac; done"#;
        let repos = [("o1-paper".to_string(), "/srv/o1".to_string())].into();
        crate::v2::with_reader(&mut s, 0, vec!["sh".into(), "-c".into(), script.into()], repos);
        assert!(reads(&s, 0) && !reads(&s, 1));
        for a in [Ask::Repos("walk".into()), Ask::Store, Ask::Machines] {
            ask(&mut s, a, 0);
        }
        let t = std::time::Instant::now();
        let mut handed = 0;
        while handed < 3 {
            crate::v2::drain_reader(&s);
            if came(&mut s) {
                handed = s.machine.handed.len();
            }
            assert!(t.elapsed() < std::time::Duration::from_secs(5), "only {handed} came");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(s.repo_state.shown["walk"][0].path.as_deref(), Some("/srv/o1"));
        assert!(s.store.shown.as_ref().is_some_and(|r| r.is_ok()), "the store's rows");
        assert!(matches!(&s.devices.shown, crate::devices::Shown::None(w) if w.contains("no devices file")), "the reader's error is said");
        assert!(!came(&mut s), "nothing new: nothing handed again");
    }
}
