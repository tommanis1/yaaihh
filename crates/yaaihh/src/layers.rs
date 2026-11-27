//! Home's Layers now (his N-1104 p1069 and his "yes", p1071; paper-research N-1446 p2772): each layer open now, a
//! piece's working copy or a run's, by what it is and whose, what it lies on (read-only and shared), its own size, and
//! since when. A press opens what lies under it. coordd answers `Request::Layers` (layer.rs `Layer`, every layer open
//! or ended); a2h-speed's piece adds a run's layer as `kind: "run"`, what lies under a layer as `under`, each item with
//! `shared` (how many open layers lie on the same), and the layer's own size as `up_bytes` (p2815, the names p2904,
//! his yes p2927). Every field is read leniently as Devices reads its answer, so the section draws today's layers now
//! and the new fields once coordd gives them. Nothing in it asks anything of him, so it draws no accent.
//!
//! Who a layer is: a run's layer names its caller (`holder`), and its id is the job's, so the window's jobs give its
//! verb ("replay"); a piece's layer names its ticket, and the agent working on it is the one whose items hold it.
use crate::App;
use crate::devices::{items, lenient};
use rune::{El, col, row, text};
use rune_kit::Kit;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

/// How often Home asks while it shows: slower than Devices, as coordd measures each layer's own size when asked.
const EVERY: Duration = Duration::from_secs(10);

/// One layer of coordd's answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Layer {
    #[serde(default, deserialize_with = "lenient")]
    pub id: Option<String>,
    /// A piece's ticket; a run's, its caller's items (a list or one).
    #[serde(default, deserialize_with = "lenient")]
    pub ticket: Option<Value>,
    #[serde(default, deserialize_with = "lenient")]
    pub branch: Option<String>,
    /// `open` or `ended` (a piece's).
    #[serde(default, deserialize_with = "lenient")]
    pub state: Option<String>,
    /// A piece's holder process (a number); a run's caller (a name).
    #[serde(default, deserialize_with = "lenient")]
    pub holder: Option<Value>,
    /// `run` for a run's layer; none for a piece's.
    #[serde(default, deserialize_with = "lenient")]
    pub kind: Option<String>,
    /// The commit its base is.
    #[serde(default, deserialize_with = "lenient")]
    pub base: Option<String>,
    #[serde(default, deserialize_with = "items")]
    pub under: Vec<Under>,
    #[serde(default, deserialize_with = "lenient")]
    pub up_bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub ended: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub removed: Option<String>,
}

/// What lies under a layer (p2904): `record` (a record up to LINE), `binary` (PATH, MD5) or `tree` (COMMIT), its size,
/// and how many open layers lie on the same.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct Under {
    #[serde(default, deserialize_with = "lenient")]
    pub what: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub record: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub line: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub path: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub md5: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub commit: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub bytes: Option<u64>,
    #[serde(default, deserialize_with = "lenient")]
    pub shared: Option<u64>,
}

/// The section's state: coordd's last answer (or why there is none), the layers pressed open, when it was last asked.
#[derive(Debug, Default)]
pub struct Layers {
    pub shown: Option<Result<Vec<Layer>, String>>,
    pub open: BTreeSet<String>,
    pub asked: Option<Instant>,
}

/// While Home shows (devices.rs `poll`, which asks Home's other request): ask project PI's coordd every [`EVERY`].
pub fn ask(s: &mut App, pi: usize) {
    if s.layers.asked.is_some_and(|t| t.elapsed() < EVERY) || s.projects[pi].link.is_none() {
        return;
    }
    s.layers.asked = Some(Instant::now());
    s.request_in(pi, coord_types::Request::Layers, crate::Pending::Layers);
}

/// coordd's answer R: its layers, each read on its own (one that does not read is left out).
pub fn came(s: &mut App, r: Result<String, String>) {
    s.layers.shown = Some(match r.and_then(|out| serde_json::from_str::<Value>(&out).map_err(|e| format!("the server's answer: {e}"))) {
        Ok(Value::Array(v)) => Ok(v.into_iter().filter_map(|x| serde_json::from_value(x).ok()).collect()),
        Ok(_) => Err("unreadable layers".into()),
        Err(e) => Err(crate::accounts::plain(&e)),
    });
}

/// A press on layer ID: opens what lies under it, a second closes it.
pub fn press(st: &mut Layers, id: &str) {
    if !st.open.remove(id) {
        st.open.insert(id.to_string());
    }
}

/// The layers open now: a run's while coordd lists it, a piece's while its state is open; neither once ended or gone.
pub fn now(all: &[Layer]) -> Vec<&Layer> {
    all.iter().filter(|l| l.ended.is_none() && l.removed.is_none() && (l.kind.as_deref() == Some("run") || l.state.as_deref() == Some("open"))).collect()
}

/// The tickets a layer names, from a ticket or a list of them.
fn tickets(l: &Layer) -> Vec<String> {
    match &l.ticket {
        Some(Value::String(t)) if !t.is_empty() => vec![t.clone()],
        Some(Value::Array(v)) => v.iter().filter_map(|t| t.as_str()).filter(|t| !t.is_empty()).map(str::to_string).collect(),
        _ => Vec::new(),
    }
}

/// What layer L is and whose, from what the window holds of PROJECTS: a run's verb (its job's, by the layer's id) and
/// caller; a piece's branch (its last part) and the agent whose items hold its ticket.
pub fn who(l: &Layer, projects: &[crate::Proj]) -> (String, Option<String>) {
    let agents = || projects.iter().flat_map(|p| &p.live.agents);
    if l.kind.as_deref() == Some("run") {
        let job = l.id.as_deref().and_then(|id| agents().flat_map(|a| &a.jobs).find(|j| j.id == id));
        let verb = job.map(|j| j.verb.clone()).filter(|v| !v.is_empty()).unwrap_or_else(|| "run".into());
        let caller = match &l.holder {
            Some(Value::String(h)) if !h.is_empty() => Some(h.clone()),
            _ => None,
        };
        return (verb, caller);
    }
    let what = l.branch.as_deref().and_then(|b| b.rsplit('/').next()).filter(|b| !b.is_empty()).or(l.id.as_deref()).unwrap_or("a piece").to_string();
    let t = tickets(l);
    let agent = agents().find(|a| a.items.iter().any(|i| t.contains(i))).map(|a| a.name.clone());
    (what, agent.or_else(|| t.first().cloned()))
}

/// The short words of item U, as its row says it ("record to line 185,206", "tree of 00aba796 (30 MB, under 12
/// pieces)", a binary by its file's name).
fn short(u: &Under) -> String {
    match u.what.as_deref() {
        Some("record") => u.line.map_or_else(|| "the record".into(), |n| format!("record to line {}", crate::store::count(n))),
        Some("binary") => u.path.as_deref().and_then(|p| p.rsplit('/').next()).filter(|n| !n.is_empty()).unwrap_or("a binary").to_string(),
        Some("tree") => {
            let c: String = u.commit.as_deref().unwrap_or_default().chars().take(8).collect();
            let more: Vec<String> = [u.bytes.map(crate::files::size), u.shared.filter(|n| *n > 1).map(|n| format!("under {n} pieces"))].into_iter().flatten().collect();
            if more.is_empty() { format!("tree of {c}") } else { format!("tree of {c} ({})", more.join(", ")) }
        }
        Some(w) => w.to_string(),
        None => "something".into(),
    }
}

/// What layer L lies on, as its row says it: each item's short words; a layer that names none lies on its base's tree.
pub fn base_words(l: &Layer) -> String {
    if l.under.is_empty() {
        return l.base.as_deref().map(|b| format!("tree of {}", b.chars().take(8).collect::<String>())).unwrap_or_default();
    }
    l.under.iter().map(short).collect::<Vec<_>>().join(" · ")
}

/// Layer L's line under its name: what it lies on, its own size, since when.
pub fn support(l: &Layer) -> String {
    let parts: Vec<String> = [Some(base_words(l)).filter(|w| !w.is_empty()), l.up_bytes.map(|b| format!("its own {}", crate::files::size(b))), l.at.as_deref().map(|t| format!("since {}", crate::when::at(t)))]
        .into_iter()
        .flatten()
        .collect();
    parts.join(" · ")
}

/// What a press opens under layer L: each item under it whole (what, which, its size, shared with how many), then its
/// ticket and branch.
pub fn opened(l: &Layer) -> Vec<String> {
    let mut v: Vec<String> = l
        .under
        .iter()
        .map(|u| {
            let which = match u.what.as_deref() {
                Some("record") => [u.record.as_ref().map(|r| format!("{r}'s record")), u.line.map(|n| format!("to line {}", crate::store::count(n)))].into_iter().flatten().collect::<Vec<_>>().join(" "),
                Some("binary") => [u.path.clone(), u.md5.as_ref().map(|m| format!("md5 {}", m.chars().take(8).collect::<String>()))].into_iter().flatten().collect::<Vec<_>>().join(" · "),
                Some("tree") => format!("the tree of {}", u.commit.as_deref().unwrap_or_default().chars().take(8).collect::<String>()),
                _ => short(u),
            };
            let shared = u.shared.filter(|n| *n > 1).map(|n| format!("{} open layers lie on it", n));
            [Some(which).filter(|w| !w.is_empty()), u.bytes.map(crate::files::size), shared].into_iter().flatten().collect::<Vec<_>>().join(" · ")
        })
        .collect();
    let t = tickets(l);
    let of: Vec<String> = [Some(t.join(", ")).filter(|t| !t.is_empty()), l.branch.as_ref().map(|b| format!("branch {b}"))].into_iter().flatten().collect();
    if !of.is_empty() {
        v.push(of.join(" · "));
    }
    v
}

/// The Layers now section for Home, under HEADING's look; none until coordd answered.
pub fn section(s: &App, k: &Kit, heading: &dyn Fn(String) -> El) -> Option<El> {
    let c = col().key("home-layers").w_full().gap(2.0);
    let all = match s.layers.shown.as_ref()? {
        Err(why) => return Some(c.child(heading("Layers now".into())).child(text(why.clone()).font(k.ty.body).color(k.c.text_muted).truncate())),
        Ok(all) => all,
    };
    let open = now(all);
    let mut c = c.child(heading(crate::refs::count("Layers now", open.len())));
    if open.is_empty() {
        return Some(c.child(text("None open".to_string()).font(k.ty.body).color(k.c.text_muted)));
    }
    for (i, l) in open.into_iter().enumerate() {
        let id = l.id.clone().unwrap_or_else(|| i.to_string());
        c = c.child(layer_row(l, &id, who(l, &s.projects), s.layers.open.contains(&id), k));
    }
    Some(c)
}

/// Layer L's row: what it is with whose quiet beside it, its line under it; a press opens what lies under it.
fn layer_row(l: &Layer, id: &str, (what, whose): (String, Option<String>), open: bool, k: &Kit) -> El {
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).truncate().shrink();
    let mut label = row().gap(6.0).center_y().shrink().min_w(0.0).child(text(what).font(k.ty.body).color(k.c.text_base).truncate());
    if let Some(w) = whose {
        label = label.child(quiet(w));
    }
    let left = col().grow(1.0).shrink().w(0.0).gap(1.0).child(label).child(text(support(l)).font(k.ty.small).color(k.c.text_muted).truncate());
    let head = row().w_full().center_y().py(4.0).child(left).key(format!("layer-{id}"));
    let key = id.to_string();
    let c = col().w_full().key(format!("layer-row-{id}")).child(k.interactive(head).on_press(move |s: &mut App| press(&mut s.layers, &key)));
    if !open {
        return c;
    }
    c.children(opened(l).into_iter().map(|t| text(t).font(k.ty.small).color(k.c.text_muted).wrap().pl(12.0).py(1.0)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    /// coordd's answer as layer.rs writes a piece's layer, with a2h-speed's run layer and the new fields (p2815, p2904),
    /// made up: a replay's run on research's record and a test binary, accounts-lead's piece on a tree twelve lie on,
    /// a piece that ended, and one whose `under` is not a list.
    pub(crate) fn answer() -> String {
        json!([
            {"id": "j7", "kind": "run", "holder": "docket-core", "ticket": ["N-1446"], "base": "c0ffee11", "up": "/made-up/up", "at": "2026-11-24T16:51:00Z",
             "up_bytes": 222_298_112u64,
             "under": [{"what": "record", "record": "research", "line": 185_206, "bytes": 1_395_864_371u64, "shared": 2},
                       {"what": "binary", "path": "target/debug/deps/coordd-f235052c", "md5": "05431864aa", "bytes": 98_566_144u64}]},
            {"id": "holders-1", "ticket": "N-1590", "repo": "/made-up/repo", "base": "00aba796beef", "branch": "accounts-lead/holders", "state": "open",
             "holder": 4242, "up": "/made-up/up2", "outputs": [], "at": "2026-11-24T15:00:00Z", "up_bytes": 1_468_006u64,
             "under": [{"what": "tree", "commit": "00aba796beef", "bytes": 31_457_280u64, "shared": 12}]},
            {"id": "old-1", "ticket": "N-12", "base": "1234abcd", "branch": "x/old", "state": "ended", "up": "/u", "outputs": [], "at": "2026-11-20T10:00:00Z", "ended": "2026-11-20T11:00:00Z"},
            {"id": "today-1", "ticket": "N-77", "base": "feedbeef99", "branch": "edge/today", "state": "open", "holder": 1, "up": "/u", "outputs": [], "at": "2026-11-24T17:00:00Z", "under": "odd"}
        ])
        .to_string()
    }

    /// The layers open now, each with its words: a run by its job's verb and caller on what it lies on with its own
    /// size; a piece by its branch and the agent holding its ticket, on a tree with how many lie on it; a piece of
    /// today's coordd (no `under`) on its base's tree; the ended one left out.
    #[test]
    fn the_layers_open_now_say_what_they_lie_on_and_whose_they_are() {
        let (_held, mut s) = crate::home::tests::made();
        s.projects[0].live.agents[0].items = vec!["N-1590".into()];
        let holder = s.projects[0].live.agents[0].name.clone();
        s.projects[0].live.agents[1].jobs = vec![serde_json::from_value(json!({"id": "j7", "verb": "replay", "since": "2026-11-24T16:51:00Z", "step": "running", "step_since": "2026-11-24T16:51:05Z"})).unwrap()];
        came(&mut s, Ok(answer()));
        let all = s.layers.shown.clone().unwrap().unwrap();
        let open = now(&all);
        assert_eq!(open.iter().map(|l| l.id.clone().unwrap()).collect::<Vec<_>>(), ["j7", "holders-1", "today-1"]);
        assert_eq!(who(open[0], &s.projects), ("replay".to_string(), Some("docket-core".to_string())));
        assert_eq!(who(open[1], &s.projects), ("holders".to_string(), Some(holder)));
        assert_eq!(who(open[2], &s.projects), ("today".to_string(), Some("N-77".to_string())), "no agent holds it: its ticket");
        assert_eq!(base_words(open[0]), "record to line 185,206 · coordd-f235052c");
        assert!(support(open[0]).starts_with("record to line 185,206 · coordd-f235052c · its own 212.0 MB · since "), "{}", support(open[0]));
        assert_eq!(base_words(open[1]), "tree of 00aba796 (30.0 MB, under 12 pieces)");
        assert_eq!(base_words(open[2]), "tree of feedbeef", "today's coordd names no under: its base");
        assert_eq!(opened(open[0]), ["research's record to line 185,206 · 1.3 GB · 2 open layers lie on it", "target/debug/deps/coordd-f235052c · md5 05431864 · 94.0 MB", "N-1446"]);
        // a run whose job the window does not hold is a run; an answer that is not a list says so
        assert_eq!(who(&Layer { kind: Some("run".into()), ..Layer::default() }, &s.projects).0, "run");
        came(&mut s, Ok(r#"{"rows": []}"#.into()));
        assert!(s.layers.shown.as_ref().unwrap().is_err());
    }
}
