//! A page drawn from a template (`coord_types::template`; paper-research N-1446 p1785, read right p1789): a `[[view]]`
//! of kind `lens`, its ask asked of the project's coordd as the Map asks for its slice (`Request::Graph`, map/lanes.rs
//! `poll`), and the answer's nodes it names drawn as the rules' rows under their counts (window-rules §3, §4); a press
//! on a row opens its node on the Map.
//!
//! It asks when he opens the page, when its ask changes, and at "Read again", as the Map is asked again on train 68
//! (lanes.rs `poll`, its button in `line`): no timer of its own (p1789, 1). A template the check refuses draws its
//! faults in one plain line and asks nothing.

use crate::views::{fill, note, str_at};
use crate::{App, Proj, Sel};
use coord_types::template::{self, Draw, Group, Order, Template};
use coord_types::{GraphAnswer, Label};
use docket_machine::graph_slice::{Ask, SliceNode};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value;
use std::collections::HashMap;

/// The rows a group draws; the rest are counted under them.
const ROWS_MOST: usize = 100;

/// The template pages' asks and what came of them.
#[derive(Default)]
pub struct Templates {
    /// Each page's answer by project and view id: the template it was asked for, and the answer or why there is none.
    pub(crate) got: HashMap<(usize, String), (Template, Result<GraphAnswer, String>)>,
    /// The asks on their way, by project and view id, each with the template it asks for.
    pub(crate) asked: HashMap<(usize, String), Template>,
    /// The page shown at the last frame, by project and view id: a page opened again asks again.
    pub(crate) shown: Option<(usize, String)>,
    /// "Read again" pressed: the page shown asks again.
    pub(crate) again: bool,
    /// Moved by each answer, for the page's cache (views.rs `view`).
    pub(crate) rev: u64,
}

/// Whether project P's coordd gives the graph's slice (`GRAPH_PAGE_PROTOCOL`), as it gives the Map's.
fn served(p: &Proj) -> bool {
    p.down.is_none() && p.protocol >= coord_types::GRAPH_PAGE_PROTOCOL
}

/// The template page shown: the focused page, as no unfocused pane draws a view (panes.rs `area`). Its project, its
/// view id and its template, where its kind is `lens`, the check takes it and its project's coordd gives the graph.
fn shown(s: &App) -> Option<(usize, String, Template)> {
    let Sel::View(v) = &s.sel else { return None };
    let p = s.projects.get(s.cur).filter(|p| served(p))?;
    let id = if v.is_empty() { p.live.home() } else { v.clone() };
    let spec = p.live.view_specs().into_iter().find(|x| str_at(x, "id") == id)?;
    let t = (str_at(spec, "kind") == "lens").then(|| template::check(spec).ok()).flatten()?;
    Some((s.cur, id, t))
}

/// Each frame: the template page shown asks its ask of its project's coordd, one ask on its way for each, when he has
/// opened it, its ask changed since its answer, or he pressed "Read again"; the answer held stays drawn until the next
/// comes. Whether it asked.
pub fn poll(s: &mut App) -> bool {
    let now = shown(s);
    let key = now.as_ref().map(|(pi, id, _)| (*pi, id.clone()));
    let opened = key.is_some() && key != s.templates.shown;
    s.templates.shown = key;
    let Some((pi, id, t)) = now else { return false };
    let key = (pi, id.clone());
    let v = &s.templates;
    let held = v.got.get(&key).is_some_and(|(was, _)| was.asks_as(&t));
    if v.asked.contains_key(&key) || (held && !opened && !v.again) {
        return false;
    }
    s.templates.again = false;
    if !s.request_in(pi, coord_types::Request::Graph { ask: asked(&t, chrono::Utc::now().timestamp()) }, crate::Pending::Template { view: id }) {
        return false;
    }
    s.templates.asked.insert(key, t);
    true
}

/// T's ask at NOW (seconds): its `within` as the span's start, `from`, a record time.
fn asked(t: &Template, now: i64) -> Ask {
    let mut a = t.ask.clone();
    if let Some(at) = t.within.and_then(|w| chrono::DateTime::from_timestamp(now - w as i64, 0)) {
        a.from = Some(at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    }
    a
}

/// Project PI's answer to view VIEW's ask: held with the template it was asked for, or why there is none.
pub fn came(s: &mut App, pi: usize, view: String, ok: bool, out: String) {
    let key = (pi, view);
    let Some(t) = s.templates.asked.remove(&key) else { return };
    let got = if ok { serde_json::from_str::<GraphAnswer>(&out).map_err(|e| format!("unreadable answer: {e}")) } else { Err(out.trim().to_string()) };
    s.templates.got.insert(key, (t, got));
    s.templates.rev += 1;
}

/// View VIEW's ask of project PI's coordd, its pipe ended before the answer: asked again once it is back, while shown.
pub fn lost(v: &mut Templates, pi: usize, view: &str) {
    v.asked.remove(&(pi, view.to_string()));
}

/// Template view ID of project P, its table SPEC, as views.rs `page` draws it.
pub fn page(s: &App, p: &Proj, spec: &Value, id: &str, k: &Kit) -> El {
    let t = match template::check(spec) {
        Ok(t) => t,
        Err(f) => return note(&format!("Can't draw this template: {}", f.join("; ")), k),
    };
    if !served(p) {
        return note("Needs a newer engine.", k);
    }
    match s.templates.got.get(&(s.cur, id.to_string())) {
        None => note("Loading…", k),
        Some((_, Err(e))) => note(e, k),
        Some((_, Ok(a))) => drawn(&t, a, k),
    }
}

/// Answer A as template T draws it: each group's count and under it its rows (`draw = "rows"`), the newest or the
/// oldest first; what the limit left out, and "Read again".
fn drawn(t: &Template, a: &GraphAnswer, k: &Kit) -> El {
    let labels: HashMap<&str, &Label> = a.labels.iter().map(|l| (l.id.as_str(), l)).collect();
    let label = |n: &SliceNode| labels.get(n.node.id.as_str()).copied();
    let mut nodes: Vec<&SliceNode> = a.slice.nodes.iter().filter(|n| t.nodes.is_empty() || t.nodes.contains(&n.node.kind)).collect();
    // by time, a node with no time last either way
    nodes.sort_by(|x, y| match (&x.node.when, &y.node.when) {
        (Some(a), Some(b)) if t.order == Order::Newest => b.cmp(a),
        (Some(a), Some(b)) => a.cmp(b),
        (x, y) => y.is_some().cmp(&x.is_some()),
    });
    let mut groups: Vec<(String, Vec<&SliceNode>)> = Vec::new();
    for n in nodes {
        let g = group_of(t.group, n, label(n));
        match groups.iter_mut().find(|(w, _)| *w == g) {
            Some((_, ns)) => ns.push(n),
            None => groups.push((g, vec![n])),
        }
    }
    let mut c = col().key("template-rows").w_full().gap(2.0).px(24.0).pb(24.0);
    if groups.is_empty() {
        let none = match &t.ask.around {
            Some(at) if a.slice.unknown => format!("The graph holds no node {at}."),
            _ => "Nothing in the graph answers the page's ask.".to_string(),
        };
        c = c.child(text(none).font(k.ty.small).color(k.c.text_muted).wrap());
    }
    for (g, ns) in &groups {
        c = c.child(row().key(format!("template-group-{g}")).pt(10.0).pb(2.0).child(crate::outline::heading(g, ns.len(), false, k)));
        if t.draw == Draw::Rows {
            c = c.children(ns.iter().take(ROWS_MOST).map(|n| row_of(t, n, label(n), k)));
            if ns.len() > ROWS_MOST {
                c = c.child(text(format!("and {} more", ns.len() - ROWS_MOST)).font(k.ty.small).color(k.c.text_faint));
            }
        }
    }
    let left = (a.slice.left > 0).then(|| text(format!("{} more edges answered than its limit gives", a.slice.left)).font(k.ty.small).color(k.c.text_faint));
    let again = Button::new("Read again").variant(ButtonVariant::Ghost).help("As it stands now").on_press(|s: &mut App| s.templates.again = true);
    let head = row().w_full().px(24.0).pt(8.0).gap(8.0).center_y().child(rune::el().grow(1.0)).children(left).child(again.build(k));
    col().w_full().h_full().child(head).child(col().w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c))
}

/// What node N, its label L, is grouped under by G.
fn group_of(g: Group, n: &SliceNode, l: Option<&Label>) -> String {
    match g {
        Group::Kind => {
            let many = crate::map::lanes::kind_words(n.node.kind).1;
            many[..1].to_uppercase() + &many[1..]
        }
        Group::By => n.by.clone().unwrap_or_else(|| "No one".into()),
        Group::Ticket => l.and_then(|l| l.on.clone()).unwrap_or_else(|| "On no thread".into()),
        Group::Day => n.node.when.as_deref().and_then(|w| w.get(..10)).map_or_else(|| "No time".into(), String::from),
    }
}

/// Node N's row (window-rules §3): the mark where it went wrong, its words (T's `words` filled from its fields,
/// `template::fields`), its id quiet unless its words are its id, and its time at the right; a press opens it on the
/// Map.
fn row_of(t: &Template, n: &SliceNode, l: Option<&Label>, k: &Kit) -> El {
    let id = n.node.id.clone();
    let words = fill(&t.words, &template::fields(n, l)).unwrap_or_default();
    let failed = n.node.ok == Some(false);
    let kind = crate::map::lanes::kind_words(n.node.kind).0;
    let said = if failed { format!("{kind}, failed") } else { kind.to_string() };
    let time = n.node.when.as_deref().map(crate::when::at).unwrap_or_default();
    let r = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .py(2.0)
        .child(crate::outline::dot(false, failed, k))
        .child(text(words.clone()).key("words").font(k.ty.body).color(k.c.text_base).truncate().grow(1.0).shrink().w(0.0))
        .children((words != id).then(|| text(id.clone()).key("id").font(k.ty.small).color(k.c.text_faint)))
        .child(text(time).font(k.ty.small).color(k.c.text_muted));
    k.interactive(r).key(format!("template-row-{id}")).description(said).on_press(move |s: &mut App| crate::map::canvas::on_map(s, &id))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use docket_machine::graph::{Node, NodeKind, Source};
    use docket_machine::graph_slice::Slice;
    #[cfg(not(windows))]
    use rune::runtime::Runtime;
    use serde_json::json;

    /// The design's example (paper-research N-1446 p1785, 1), as the snapshot's project file holds it.
    pub(crate) fn made() -> Value {
        json!({"id": "edge6-made", "kind": "lens", "title": "What docket-edge6 made this week", "draw": "rows", "nodes": ["commit", "job", "build"],
            "group": "kind", "order": "newest", "words": "{label}", "within": "7d",
            "ask": {"around": "agent:docket-edge6", "hops": 2, "layers": ["work"], "limit": 2000}})
    }

    fn node(id: &str, kind: NodeKind, at: &str, what: &str, ok: Option<bool>, by: Option<&str>) -> SliceNode {
        SliceNode { node: Node { id: id.into(), kind, source: Source::Line(1), when: Some(format!("2026-11-22T{at}Z")), what: what.into(), ok, tokens: None }, by: by.map(String::from) }
    }

    fn label(id: &str, is: &str, words: &str, on: Option<&str>) -> Label {
        Label { id: id.into(), is: is.into(), label: words.into(), on: on.map(String::from) }
    }

    /// A made-up answer around docket-edge6: the agent, two commits, a job that failed and one that did not, a build,
    /// and a post whose label holds its first line, as coordd's labels give a post's.
    pub(crate) fn answer() -> GraphAnswer {
        let nodes = vec![
            node("agent:docket-edge6", NodeKind::Agent, "08:00:00", "", None, None),
            node("commit:28684eeb", NodeKind::Commit, "09:00:00", "a read row says its path", None, None),
            node("commit:3b2c4ac5", NodeKind::Commit, "10:00:00", "the read row, its last read's time", None, None),
            node("job:7-1", NodeKind::Job, "10:05:00", "test coordd: 251 passed, 1 failed", Some(false), None),
            node("job:7-2", NodeKind::Job, "10:20:00", "test coordd: 252 passed", Some(true), None),
            node("build:69", NodeKind::Build, "11:00:00", "", None, None),
            node("N-1446.p1789", NodeKind::Post, "11:30:00", "", None, Some("paper-research")),
        ];
        let labels = vec![
            label("agent:docket-edge6", "agent", "docket-edge6", None),
            label("commit:28684eeb", "commit", "a read row says its path", None),
            label("commit:3b2c4ac5", "commit", "the read row, its last read's time", None),
            label("N-1446.p1789", "post", "paper-research: made-up first words of the post", Some("0.3 prototype")),
        ];
        GraphAnswer { slice: Slice { nodes, ..Default::default() }, labels, ..Default::default() }
    }

    /// The made-up project research, its coordd at PROTOCOL, its project file's one view VIEW shown; its pipe a sink
    /// (map/deep.rs `piped`), named NAME.
    #[cfg(not(windows))]
    fn app(name: &str, view: Value, protocol: u32) -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child, std::path::PathBuf) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.protocol = protocol;
        p.live.project = json!({"project": {"name": "research"}, "view": [view.clone()]});
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::View(str_at(&view, "id"));
        let (child, out) = crate::map::deep::tests::piped(&mut s, 0, name);
        (held, s, child, out)
    }

    #[cfg(not(windows))]
    fn ask_poll(s: &mut App, _: f64, _: &mut rune::Tx) -> bool {
        poll(s)
    }

    #[cfg(not(windows))]
    fn window(s: App) -> Runtime<App> {
        let mut rt = Runtime::new(s, crate::view, (1400.0, 900.0), rune::Env::new(false, 1.0));
        rt.poll = Some(ask_poll);
        frames(&mut rt);
        rt
    }

    #[cfg(not(windows))]
    fn frames(rt: &mut Runtime<App>) {
        for _ in 0..4 {
            rt.invalidate();
            rt.frame(0.05);
        }
    }

    /// The template pages' asks on their way to project 0's coordd: each its id and its view.
    #[cfg(not(windows))]
    fn asks(rt: &Runtime<App>) -> Vec<(u64, String)> {
        let mut v: Vec<_> = rt.state.projects[0].pending.iter().filter_map(|(id, w)| if let crate::Pending::Template { view } = w { Some((*id, view.clone())) } else { None }).collect();
        v.sort();
        v
    }

    /// Coordd's answer A to the ask on its way, taken as the window takes a reply.
    #[cfg(not(windows))]
    fn answered(rt: &mut Runtime<App>, a: &GraphAnswer) {
        let (id, _) = asks(rt).pop().expect("an ask on its way");
        let out = serde_json::to_string(a).unwrap();
        rt.update(|s: &mut App, _| {
            s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None }));
        });
        frames(rt);
    }

    /// The page's groups and rows, top to bottom, by their keys.
    #[cfg(not(windows))]
    fn drawn(rt: &Runtime<App>) -> Vec<String> {
        let mut v: Vec<(f32, String)> = rt
            .elements()
            .into_iter()
            .filter(|e| !e.exiting)
            .filter_map(|e| {
                let k = e.key.as_deref()?;
                (k.starts_with("template-row-") || k.starts_with("template-group-")).then(|| (e.rect[1], k.to_string()))
            })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v.into_iter().map(|x| x.1).collect()
    }

    /// The first text drawn inside the element keyed KEY; with WORDS, the text keyed `words` there.
    #[cfg(not(windows))]
    fn text_of(rt: &Runtime<App>, key: &str, words: bool) -> Option<String> {
        let inside = format!("/{key}/");
        rt.elements().into_iter().find(|e| !e.exiting && e.path.contains(&inside) && e.text.is_some() && (!words || e.key.as_deref() == Some("words"))).and_then(|e| e.text)
    }

    /// Every text drawn, and every description.
    #[cfg(not(windows))]
    fn all_said(rt: &Runtime<App>) -> String {
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
        format!("{} {}", texts.join(" "), rt.accessibility_json())
    }

    #[cfg(not(windows))]
    fn said_of(v: &Value, key: &str) -> Option<String> {
        if v["key"].as_str() == Some(key) {
            return v["description"].as_str().map(String::from);
        }
        v["children"].as_array()?.iter().find_map(|c| said_of(c, key))
    }

    #[cfg(not(windows))]
    fn press_text(rt: &mut Runtime<App>, words: &str) {
        let e = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(words)).unwrap_or_else(|| panic!("{words} drawn"));
        press_at(rt, [e.rect[0] + e.rect[2] / 2.0, e.rect[1] + e.rect[3] / 2.0]);
    }

    #[cfg(not(windows))]
    fn press_at(rt: &mut Runtime<App>, at: [f32; 2]) {
        rt.pointer_move(at[0], at[1]);
        rt.frame(0.016);
        rt.pointer_down(at[0], at[1]);
        rt.pointer_up(at[0], at[1]);
        frames(rt);
    }

    /// The view's table in project 0's file changed by F, as a snapshot would bring it.
    #[cfg(not(windows))]
    fn view_changed(rt: &mut Runtime<App>, f: impl FnOnce(&mut Value)) {
        rt.update(|s: &mut App, _| {
            f(&mut s.projects[0].live.project["view"][0]);
            s.projects[0].live.views_rev += 1;
        });
        frames(rt);
    }

    /// p1789, 1: `within` is the span's start the page asks for, now less it.
    #[test]
    fn within_is_the_asks_start_now_less_it() {
        let t = template::check(&made()).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-22T22:30:00Z").unwrap().timestamp();
        assert_eq!(asked(&t, now), Ask { from: Some("2026-11-15T22:30:00Z".into()), ..t.ask.clone() });
        assert_eq!(asked(&Template { within: None, ..t.clone() }, now), t.ask, "no within: the ask as written");
    }

    /// p1785 1, p1789: the design's example drawn from a made-up answer. Its kinds' counts, the newest first, each node
    /// a row under its count with its label as its words and its id; the job that failed marked and said so; the agent
    /// and the post the ask reached left out, as its `nodes` say. Asked once, of the ask its text names.
    #[test]
    #[cfg(not(windows))]
    fn a_template_draws_its_kinds_counts_and_rows_from_the_answer() {
        let (_held, s, child, out) = app("t-rows", made(), coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        assert_eq!(asks(&rt).len(), 1, "asked on opening");
        answered(&mut rt, &answer());
        let rows = ["template-group-Builds", "template-row-build:69", "template-group-Jobs", "template-row-job:7-2", "template-row-job:7-1", "template-group-Commits", "template-row-commit:3b2c4ac5", "template-row-commit:28684eeb"];
        assert_eq!(drawn(&rt), rows);
        let heads: Vec<Option<String>> = ["Builds", "Jobs", "Commits"].iter().map(|g| text_of(&rt, &format!("template-group-{g}"), false)).collect();
        assert_eq!(heads, [Some("Builds · 1".to_string()), Some("Jobs · 2".into()), Some("Commits · 2".into())], "the rules' counts");
        assert_eq!(text_of(&rt, "template-row-commit:3b2c4ac5", true).as_deref(), Some("the read row, its last read's time"), "its label");
        assert_eq!(text_of(&rt, "template-row-job:7-1", true).as_deref(), Some("test coordd: 251 passed, 1 failed"), "no label: what it is");
        let ids = |row: &str| rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains(&format!("/{row}/")) && e.key.as_deref() == Some("id")).count();
        assert_eq!((ids("template-row-commit:3b2c4ac5"), ids("template-row-build:69")), (1, 0), "the id quiet beside a label, once where it is the words");
        let a11y = rt.accessibility_json();
        assert_eq!((said_of(&a11y, "template-row-job:7-1").as_deref(), said_of(&a11y, "template-row-job:7-2").as_deref()), (Some("job, failed"), Some("job")));
        assert!(asks(&rt).is_empty(), "nothing more asked");
        rt.state.projects[0].link = None;
        let sent: Vec<Value> = crate::map::deep::tests::sent(child, &out).into_iter().filter(|v| v.to_string().contains("agent:docket-edge6")).collect();
        assert_eq!(sent.len(), 1, "{sent:?}");
        let ask = sent[0].pointer("/req/ask").cloned().unwrap_or_default();
        assert_eq!((&ask["around"], &ask["hops"], &ask["layers"], &ask["limit"]), (&json!("agent:docket-edge6"), &json!(2), &json!(["work"]), &json!(2000)), "{}", sent[0]);
        let from = ask["from"].as_str().and_then(|f| chrono::DateTime::parse_from_rfc3339(f).ok()).map(|f| f.timestamp()).unwrap_or_default();
        assert!((chrono::Utc::now().timestamp() - 7 * 86_400 - from).abs() < 120, "from: now less 7 days, {}", sent[0]);
    }

    /// p1789, 2: a post's row says its writer, never its words; grouped by thread, under the thread's label.
    #[test]
    #[cfg(not(windows))]
    fn a_posts_row_says_its_writer_never_its_words() {
        let mut view = made();
        view["nodes"] = json!(["post"]);
        view["group"] = json!("ticket");
        let (_held, s, _child, _out) = app("t-post", view, coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        answered(&mut rt, &answer());
        assert_eq!(drawn(&rt), ["template-group-0.3 prototype", "template-row-N-1446.p1789"]);
        assert_eq!(text_of(&rt, "template-row-N-1446.p1789", true).as_deref(), Some("paper-research"));
        assert!(!all_said(&rt).contains("made-up first words"), "the post's words nowhere");
    }

    /// `draw = "count"`: the groups' counts alone.
    #[test]
    #[cfg(not(windows))]
    fn a_count_template_draws_its_counts_alone() {
        let mut view = made();
        view["draw"] = json!("count");
        let (_held, s, _child, _out) = app("t-count", view, coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        answered(&mut rt, &answer());
        assert_eq!(drawn(&rt), ["template-group-Builds", "template-group-Jobs", "template-group-Commits"]);
    }

    /// p1785 3, my red "the window asks before checking": a template the check refuses draws its faults in one plain
    /// line and asks nothing (`focus` too, p1818 1); so does a coordd that gives no graph.
    #[test]
    #[cfg(not(windows))]
    fn a_refused_template_draws_its_faults_and_asks_nothing() {
        for (name, change, says) in [
            ("t-hops", ("hops", json!(4)), "Can't draw this template: `ask.hops`: a whole number from 0 to 3 is wanted, not 4"),
            ("t-folder", ("folder", json!("docket-edge6")), "Can't draw this template: `ask.folder`"),
            ("t-focus", ("around", json!("focus")), "Can't draw this template: `ask.around`: `focus` comes with cards (T2); name a node"),
        ] {
            let mut view = made();
            view["ask"][change.0] = change.1;
            let (_held, s, _child, _out) = app(name, view, coord_types::GRAPH_PAGE_PROTOCOL);
            let rt = window(s);
            assert!(asks(&rt).is_empty(), "{name}: nothing asked");
            assert!(rt.elements().iter().any(|e| e.text.as_deref().is_some_and(|t| t.starts_with(says))), "{name}: {}", all_said(&rt));
        }
        let (_held, s, _child, _out) = app("t-old", made(), coord_types::GRAPH_PAGE_PROTOCOL - 1);
        let rt = window(s);
        assert!(asks(&rt).is_empty(), "an older coordd: nothing asked");
        assert!(all_said(&rt).contains("Needs a newer engine"));
    }

    /// p1789, 1: no timer of its own. Asked when he opens the page, when its ask changes and at "Read again", as the
    /// Map is; not again each frame, nor when only how it is drawn changes.
    #[test]
    #[cfg(not(windows))]
    fn a_template_asks_when_opened_when_its_ask_changes_and_at_read_again() {
        let (_held, s, _child, _out) = app("t-again", made(), coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        answered(&mut rt, &answer());
        frames(&mut rt);
        assert!(asks(&rt).is_empty(), "held: not asked each frame");
        view_changed(&mut rt, |v| v["words"] = json!("{kind}: {what}"));
        assert!(asks(&rt).is_empty(), "drawn otherwise, asked the same: not asked");
        assert_eq!(text_of(&rt, "template-row-job:7-1", true).as_deref(), Some("job: test coordd: 251 passed, 1 failed"), "drawn anew");
        rt.update(|s: &mut App, _| s.sel = Sel::Agents);
        frames(&mut rt);
        rt.update(|s: &mut App, _| s.sel = Sel::View("edge6-made".into()));
        frames(&mut rt);
        assert_eq!(asks(&rt).len(), 1, "opened again: asked");
        answered(&mut rt, &answer());
        press_text(&mut rt, "Read again");
        assert_eq!(asks(&rt).len(), 1, "Read again: asked");
        answered(&mut rt, &answer());
        view_changed(&mut rt, |v| v["ask"]["hops"] = json!(1));
        assert_eq!(asks(&rt).len(), 1, "its ask changed: asked");
    }

    /// A press on a row opens its node on the Map, the cloud's focus (map/cloud.rs `went_in`, p1818 2): kept at the
    /// cloud's next poll, which asks its slice once a layer is on beside his (`near_ask`); the same when the cloud
    /// drawn last was another project's.
    #[test]
    #[cfg(not(windows))]
    fn a_press_on_a_row_opens_its_node_on_the_map() {
        let (_held, s, _child, _out) = app("t-press", made(), coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        for other in [false, true] {
            if other {
                rt.update(|s: &mut App, _| {
                    s.sel = Sel::View("edge6-made".into());
                    s.cloud.of = 1;
                    s.projects[0].pending.retain(|_, w| !matches!(w, crate::Pending::Graph(_)));
                });
                frames(&mut rt);
            }
            answered(&mut rt, &answer());
            let (r, _) = rt.presented("template-row-commit:3b2c4ac5").expect("the commit's row");
            press_at(&mut rt, [r[0] + 40.0, r[1] + r[3] / 2.0]);
            assert!(crate::map::canvas::shown(&rt.state), "the Map: {:?}", rt.state.sel);
            assert_eq!(rt.state.cloud.focus.as_deref(), Some("commit:3b2c4ac5"), "its focus (another project's cloud before: {other})");
            rt.update(|s: &mut App, _| {
                s.cloud.layers = [docket_machine::graph_slice::Layer::Work].into_iter().collect();
                crate::map::cloud::poll(s);
            });
            assert_eq!(rt.state.cloud.focus.as_deref(), Some("commit:3b2c4ac5"), "kept at the poll (another project's cloud before: {other})");
            let asked: Vec<_> = rt.state.projects[0].pending.values().filter_map(|w| if let crate::Pending::Graph(a) = w { a.around.clone() } else { None }).collect();
            assert!(asked.iter().any(|a| a == "commit:3b2c4ac5"), "its slice asked (another project's cloud before: {other}): {asked:?}");
        }
    }

    /// p1885 (p1903): any focus is a card. The commit's row pressed, the cloud asks its slice in "Your messages" too,
    /// with work, its own layer, beside his (map/cloud.rs `near_ask`), and draws the commit as a card of its own, its
    /// subject its words, holding its job's card and its build's; in "Everything" the same, asked again with its layers
    /// (extends [`a_press_on_a_row_opens_its_node_on_the_map`]).
    #[test]
    #[cfg(not(windows))]
    fn a_commits_row_pressed_draws_its_card_holding_its_job_and_build() {
        use crate::map::cloud;
        let (_held, s, _child, _out) = app("t-focus-card", made(), coord_types::GRAPH_PAGE_PROTOCOL);
        let mut rt = window(s);
        answered(&mut rt, &answer());
        let (r, _) = rt.presented("template-row-commit:3b2c4ac5").expect("the commit's row");
        press_at(&mut rt, [r[0] + 40.0, r[1] + r[3] / 2.0]);
        for (style, on) in [("Your messages", &[][..]), ("Everything", cloud::PRESETS[2].1)] {
            rt.update(move |s: &mut App, _| {
                s.cloud.kept_read = Some(Err("a test's places are not kept".into()));
                s.cloud.layers = on.iter().copied().collect();
                cloud::poll(s);
            });
            // coordd's answers to the cloud's asks: his layer, and the commit's slice
            let asked: Vec<(u64, docket_machine::graph_slice::Ask)> = rt.state.projects[0].pending.iter().filter_map(|(id, w)| if let crate::Pending::Graph(a) = w { Some((*id, a.clone())) } else { None }).collect();
            assert!(asked.iter().any(|(_, a)| a.around.as_deref() == Some("commit:3b2c4ac5")), "{style}: its slice asked: {asked:?}");
            for (id, a) in asked {
                let out = serde_json::to_string(&if a.around.is_some() { cloud::tests::near_commit() } else { cloud::tests::base() }).unwrap();
                rt.update(move |s: &mut App, _| {
                    s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out, post: None }));
                });
            }
            rt.update(|s: &mut App, _| {
                cloud::poll(s);
            });
            frames(&mut rt);
            let g = rt.state.map.graph().expect("the cloud laid").clone();
            let c = g.find_in(g.project("project:research").expect("research's island"), "commit:3b2c4ac5").unwrap_or_else(|| panic!("{style}: the commit's card"));
            assert_eq!(g.node(c).label, "the read row, its last read's time", "{style}");
            let held: Vec<&str> = g.children(c).iter().map(|k| g.node(*k).label.as_str()).collect();
            assert_eq!(held, ["test coordd: 252 passed", "build:69"], "{style}");
        }
    }

    /// p1789, 3: a view of a kind this window does not draw, with no source to wait for, says it needs a newer window;
    /// one with a source still waits for its output.
    #[test]
    #[cfg(not(windows))]
    fn a_kind_this_window_does_not_draw_says_it_needs_a_newer_one() {
        for (name, view, says) in [
            ("t-newer", json!({"id": "pie", "kind": "pie"}), "Needs a newer window."),
            ("t-sourced", json!({"id": "pie", "kind": "pie", "source": "./pie"}), "waiting for the source's first output"),
        ] {
            let (_held, s, _child, _out) = app(name, view, coord_types::GRAPH_PAGE_PROTOCOL);
            let rt = window(s);
            assert!(all_said(&rt).contains(says), "{name}: {}", all_said(&rt));
        }
    }

    /// The template page's stills, from a made-up project and answer (never a coordd): the design's example, dark and
    /// light; its counts alone; a template the check refuses. `STILL_OUT=DIR yaaihh-tests templates::tests::stills
    /// --ignored --exact`.
    #[test]
    #[ignore]
    fn stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        // view_cost's test_app's folder, whose projects.toml the window reads at its start (paths.rs `folder_stills`)
        let own = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("projects.toml"), "# the template page's stills: no project, nothing asked of a coordd\n").unwrap();
        fn on(view: Value, dark: bool) -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
            p.protocol = coord_types::GRAPH_PAGE_PROTOCOL;
            p.down = None;
            p.live.project = json!({"project": {"name": "research"}, "view": [view.clone()]});
            s.projects = vec![p];
            (s.cur, s.dark) = (0, Some(dark));
            s.sel = Sel::View(str_at(&view, "id"));
            if let Ok(t) = template::check(&view) {
                s.templates.got.insert((0, str_at(&view, "id")), (t, Ok(answer())));
            }
            s
        }
        let shots: [(&str, fn() -> App); 4] = [
            ("template-rows-dark.png", || on(made(), true)),
            ("template-rows-light.png", || on(made(), false)),
            ("template-count.png", || {
                let mut v = made();
                v["draw"] = json!("count");
                on(v, true)
            }),
            ("template-refused.png", || {
                let mut v = made();
                v["ask"]["hops"] = json!(4);
                on(v, true)
            }),
        ];
        for (name, init) in shots {
            let args = vec!["--snap".to_string(), out.join(name).display().to_string(), "--size".into(), "1400x900".into(), "--at".into(), "1.0".into(), "--tree".into(), out.join(name.replace(".png", ".json")).display().to_string()];
            rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(args).try_run().unwrap();
        }
    }
}
