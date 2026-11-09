//! The window's state, kept across a restart or reopen: the split and its panes, the sidebar, what each
//! pane shows (the project and page, the agent, Chat or Terminal), the zoom, every text typed and not sent (the composers, the
//! answers, the comments waiting and the one being written) and the boxes open (replies, the new-project dialog).
//! Mail sent and not delivered yet is coordd's and comes back from it.
//!
//! One file, `ui.json` beside the config (`ui-dev.json` for `--dev`), written whole to a temporary file and renamed
//! over it, half a second after the last change; read once at start. A missing file, or one that does not parse,
//! means the defaults; projects and agents that are gone are left out.

use crate::{App, Sel, ask};
use rune::TextEdit;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// How long the state stays unchanged before it is written.
const SETTLE: Duration = Duration::from_millis(500);

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct Saved {
    /// The focused project, by id.
    project: Option<String>,
    sel: Option<Sel>,
    /// The split's other panes: (project id, page).
    panes: Vec<(String, Sel)>,
    focus: usize,
    sidebar_width: Option<f32>,
    sidebar_open: Option<bool>,
    /// The whole window's zoom (zoom.rs).
    zoom: Option<f32>,
    /// (project id, agent, text).
    composers: Vec<(String, String, String)>,
    /// (project id, mail id, typed, comments): what a queued mail was made of, for Edit.
    drafts: Vec<(String, String, String, Vec<ask::Comment>)>,
    /// Comments waiting, per target (an agent or a question).
    comments: Vec<(String, Vec<ask::Comment>)>,
    /// Answers being written, per question.
    answers: Vec<(String, String)>,
    picked: Option<ask::Picked>,
    /// The note being written for `picked`.
    writing: Option<String>,
    replying: Vec<String>,
    show_all: bool,
    /// Agents shown as their terminal: (project id, agent), on their page and as tiles.
    term_pages: Vec<(String, String)>,
    term_tiles: Vec<(String, String)>,
    /// The runs and tool calls opened in the chats.
    expanded: Vec<String>,
    agents_tab: usize,
    open_mail: Option<String>,
    new_project: Option<NewProject>,
}

#[derive(Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
struct NewProject {
    name: String,
    dir: String,
    file: String,
    boxed: bool,
}

/// What was last written, and since when the state differs from it.
#[derive(Default)]
pub struct Keeper {
    /// The hash of the state last written (or read): each poll hashes the state's JSON as it is written out, without
    /// building the text, and writes only when the hash moved.
    last: u64,
    since: Option<Instant>,
}

fn path() -> std::path::PathBuf {
    let name = if crate::DEV.load(std::sync::atomic::Ordering::Relaxed) { "ui-dev.json" } else { "ui.json" };
    std::path::Path::new(&crate::projects::config_path()).with_file_name(name)
}

fn edit(t: &str) -> TextEdit {
    TextEdit::new(t.to_string())
}

fn take(s: &App) -> Saved {
    let id = |i: usize| s.projects.get(i).map(|p| p.id.clone());
    let pairs = |set: &std::collections::HashSet<(usize, String)>| {
        let mut v: Vec<(String, String)> = set.iter().filter_map(|(i, a)| Some((id(*i)?, a.clone()))).collect();
        v.sort();
        v
    };
    let mut composers: Vec<_> = s.composers.iter().filter(|(_, t)| !t.text.is_empty()).filter_map(|((i, a), t)| Some((id(*i)?, a.clone(), t.text.clone()))).collect();
    composers.sort();
    let mut drafts: Vec<_> = s.drafts.iter().filter_map(|((i, m), (t, c))| Some((id(*i)?, m.clone(), t.clone(), c.clone()))).collect();
    drafts.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let mut comments: Vec<_> = s.notes.comments.iter().filter(|(_, v)| !v.is_empty()).map(|(t, v)| (t.clone(), v.clone())).collect();
    comments.sort_by(|a, b| a.0.cmp(&b.0));
    let mut answers: Vec<_> = s.notes.answers.iter().filter(|(_, t)| !t.text.is_empty()).map(|(q, t)| (q.clone(), t.text.clone())).collect();
    answers.sort();
    let mut replying: Vec<String> = s.notes.replying.iter().cloned().collect();
    replying.sort();
    let mut expanded: Vec<String> = s.expanded.iter().cloned().collect();
    expanded.sort();
    let np = &s.new_project;
    Saved {
        project: id(s.cur),
        sel: Some(s.sel.clone()),
        panes: s.panes.iter().filter_map(|(i, sel)| Some((id(*i)?, sel.clone()))).collect(),
        focus: s.focus,
        sidebar_width: Some(s.sidebar.width()),
        sidebar_open: Some(s.sidebar.is_open()),
        zoom: Some(s.zoom.level()),
        composers,
        drafts,
        comments,
        answers,
        picked: s.notes.picked.clone(),
        writing: s.notes.writing.as_ref().map(|w| w.text.clone()),
        replying,
        show_all: s.notes.show_all,
        term_pages: pairs(&s.term.pages),
        term_tiles: pairs(&s.term.tiles),
        expanded,
        agents_tab: s.agents_tab,
        open_mail: s.open_mail.clone(),
        new_project: np.open.then(|| NewProject { name: np.name.text.clone(), dir: np.dir.text.clone(), file: np.file.text.clone(), boxed: np.boxed }),
    }
}

/// The state saved last, put back into S (just made, its projects not connected yet).
pub fn restore(s: &mut App) {
    let Some(v) = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) else { return };
    let ids: Vec<String> = s.projects.iter().map(|p| p.id.clone()).collect();
    let ix = |id: &str| ids.iter().position(|p| p == id);
    // an agent page read once the project connects (its snapshot reads every chat it holds)
    let mut chats: Vec<(usize, String)> = Vec::new();
    if let Some(i) = v.project.as_deref().and_then(ix) {
        s.cur = i;
        // a page of an extension this yaaihh lacks is not restored
        if let Some(sel) = v.sel.filter(|sel| !matches!(sel, Sel::Ext(e, _) if crate::ext::get(e).is_none())) {
            if let Sel::Agent(a) = &sel {
                chats.push((i, a.clone()));
            }
            s.sel = sel;
        }
    }
    s.panes = v.panes.into_iter().filter_map(|(p, sel)| Some((ix(&p)?, sel))).collect();
    for (i, sel) in &s.panes {
        if let Sel::Agent(a) = sel {
            chats.push((*i, a.clone()));
        }
    }
    s.focus = if s.panes.is_empty() { 0 } else { v.focus.min(s.panes.len() - 1) };
    if let Some(w) = v.sidebar_width {
        s.sidebar.set_width(w);
    }
    if let Some(o) = v.sidebar_open {
        s.sidebar.set_open(o);
    }
    if let Some(z) = v.zoom {
        s.zoom.restore(z);
    }
    for (i, a) in chats {
        s.projects[i].convs.entry(a).or_default();
    }
    for (p, a, t) in v.composers {
        if let Some(i) = ix(&p) {
            s.composers.insert((i, a), edit(&t));
        }
    }
    for (p, m, t, c) in v.drafts {
        if let Some(i) = ix(&p) {
            s.drafts.insert((i, m), (t, c));
        }
    }
    s.notes.comments = v.comments.into_iter().collect();
    s.notes.answers = v.answers.into_iter().map(|(q, t)| (q, edit(&t))).collect();
    s.notes.picked = v.picked;
    s.notes.writing = v.writing.filter(|_| s.notes.picked.is_some()).map(|t| edit(&t));
    s.notes.replying = v.replying.into_iter().collect();
    s.notes.show_all = v.show_all;
    s.term.pages = v.term_pages.into_iter().filter_map(|(p, a)| Some((ix(&p)?, a))).collect();
    s.term.tiles = v.term_tiles.into_iter().filter_map(|(p, a)| Some((ix(&p)?, a))).collect();
    s.expanded = v.expanded.into_iter().collect();
    s.agents_tab = v.agents_tab;
    s.open_mail = v.open_mail;
    if let Some(np) = v.new_project {
        s.new_project.open = true;
        (s.new_project.name, s.new_project.dir, s.new_project.file) = (edit(&np.name), edit(&np.dir), edit(&np.file));
        s.new_project.boxed = np.boxed;
    }
    s.keeper.last = hash(&take(s));
}

fn hash(saved: &Saved) -> u64 {
    let mut v = crate::version::Version::default();
    let _ = serde_json::to_writer(&mut v, saved);
    v.finish()
}

/// Each poll: a state that changed and then stayed so for [`SETTLE`] is written; until then the window wakes for it.
pub fn keep(s: &mut App, tx: &mut rune::Tx) {
    if rune::is_headless() {
        return;
    }
    let saved = take(s);
    let now = hash(&saved);
    if now == s.keeper.last {
        s.keeper.since = None;
        return;
    }
    let since = *s.keeper.since.get_or_insert_with(Instant::now);
    let waited = since.elapsed();
    if waited < SETTLE {
        tx.wake_after(SETTLE - waited);
        return;
    }
    let p = path();
    let tmp = p.with_extension("json.tmp");
    let text = serde_json::to_string(&saved).unwrap_or_default();
    if std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &p)).is_ok() {
        s.keeper.last = now;
        s.keeper.since = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::ProjectConfig;

    fn app(dir: &std::path::Path) -> App {
        unsafe { std::env::set_var("YAAIHH_CONFIG", dir.join("projects.toml")) };
        let mut s = crate::init();
        s.projects = ["a", "b"].iter().map(|id| crate::Proj::new(&ProjectConfig { name: id.to_string(), id: id.to_string(), boxed: true }, Vec::new())).collect();
        s
    }

    #[test]
    fn the_window_comes_back_as_it_was_left_and_a_bad_file_means_defaults() {
        let dir = std::env::temp_dir().join(format!("yaaihh-state-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = app(&dir);
        s.cur = 1;
        s.sel = Sel::Agent("LAZY".into());
        s.panes = vec![(0, Sel::Questions)];
        s.composers.insert((1, "LAZY".into()), edit("half a thought"));
        s.notes.answers.insert("q3".into(), edit("yes, but"));
        s.notes.replying.insert("m7".into());
        s.term.pages.insert((0, "orch".into()));
        let saved = serde_json::to_string(&take(&s)).unwrap();
        std::fs::write(path(), &saved).unwrap();

        let mut back = app(&dir);
        // the projects come back in another order: they are found by id
        back.projects.reverse();
        restore(&mut back);
        assert_eq!((back.cur, &back.sel), (0, &Sel::Agent("LAZY".into())));
        assert_eq!(back.panes, vec![(1, Sel::Questions)]);
        assert_eq!(back.composers.get(&(0, "LAZY".into())).map(|t| t.text.as_str()), Some("half a thought"));
        assert_eq!(back.notes.answers.get("q3").map(|t| t.text.as_str()), Some("yes, but"));
        assert!(back.notes.replying.contains("m7") && back.term.pages.contains(&(1, "orch".into())));
        assert!(back.projects[0].convs.contains_key("LAZY"), "the agent shown is read once its project connects");

        std::fs::write(path(), "{ not json").unwrap();
        let mut fresh = app(&dir);
        restore(&mut fresh);
        assert_eq!((fresh.cur, &fresh.sel, fresh.panes.len()), (0, &Sel::View(String::new()), 0));
        std::fs::remove_dir_all(&dir).ok();
    }
}
