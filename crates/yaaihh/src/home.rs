//! A project's Overview, every project's first tab: its repositories with what git says of each now and their
//! actions (the code view, Files, a PDF), its agents, its newest threads, where its space goes, and the way to its settings.
//! When the project file has a home view of its own, that view is the project's home and this page is [`ID`].

use crate::{App, Proj, Sel};
use coord_types::{AgentView, Request, TicketView};
use rune::{El, ScrollSpec, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextInput};
use serde_json::Value;
use std::sync::mpsc::Receiver;

/// How many of the newest open topics Home lists; All threads opens the rest.
const TOPICS: usize = 8;

/// The marks of what a press does: goes to a page, unfolds in place.
pub const GO: &str = "›";
pub const FOLD: [&str; 2] = ["▸", "▾"];

/// How often the repositories' state is asked while Home shows.
const REPO_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// Home's view id in a project whose file has a home view of its own, where `View("")` is that view: an id no view
/// of a project file takes.
pub const ID: &str = "~home";

/// Whether P's home is this page: its file has no home view (live.rs `home`: its `home`, else its first view).
pub fn here(p: &Proj) -> bool {
    p.live.home().is_empty()
}

/// Home's page in P, its tab: the project's home when it is P's home, else [`ID`].
pub fn sel(p: &Proj) -> Sel {
    Sel::View(if here(p) { String::new() } else { ID.to_string() })
}

/// What repository NAME of P is for, in the file's words (`[project] about = { NAME = "…" }`; yaaihh/fresh N-1729, the
/// owner's yes on yaaihh's N-1): a key coordd's project file reading leaves to the window, as `repos` stays plain
/// folders for the engine. None when the file says nothing of it.
fn about(p: &Proj, name: &str) -> Option<String> {
    p.live.project.pointer("/project/about").and_then(|m| m.get(name)).and_then(Value::as_str).map(str::trim).filter(|x| !x.is_empty()).map(str::to_string)
}

/// Where the owner's New agent starts an agent now, by coordd's one rule (`places.start`, core.rs `start_dir`); None
/// from a coordd before it.
fn start(p: &Proj) -> Option<&str> {
    p.live.places.as_ref()?.start.as_deref().filter(|x| !x.is_empty())
}

/// P's repositories as its file names them: each of `repos` by name (as coordd takes them, by name) with its folder as
/// the file writes it, else its `repo` alone; and whether New agent starts in it. The file's pick (project.rs `repo`:
/// `repo` names one of `repos`, else the first by name) is the folder coordd sends as `places.repo`; agents start in it
/// when `places.start` is that folder, or, from a coordd that sends no `start`, by the file's rule alone.
pub fn repos(p: &Proj) -> Vec<(String, String, bool)> {
    let f = &p.live.project;
    let repo = f.pointer("/project/repo").and_then(Value::as_str).map(str::trim);
    let mut named: Vec<(String, String)> = f.pointer("/project/repos").and_then(Value::as_object).map(|m| m.iter().filter_map(|(n, v)| Some((n.clone(), v.as_str()?.to_string()))).collect()).unwrap_or_default();
    named.sort();
    let resolved = p.live.places.as_ref().and_then(|x| x.repo.as_deref());
    let there = match start(p) {
        Some(st) => resolved.is_some_and(|r| r.trim_end_matches('/') == st.trim_end_matches('/')),
        None => true,
    };
    if named.is_empty() {
        return repo.map(|r| vec![(r.rsplit('/').find(|x| !x.is_empty()).unwrap_or(r).to_string(), r.to_string(), there)]).unwrap_or_default();
    }
    let pick = match repo {
        Some(r) if named.iter().any(|(n, _)| n == r) => r.to_string(),
        _ => named[0].0.clone(),
    };
    named.into_iter().map(|(n, path)| (n.clone(), path, there && n == pick)).collect()
}

/// Agent NAME's home in project P, as coordd keeps it (`agents/NAME/home` in its coord's folder).
pub fn home_of(p: &Proj, name: &str) -> Option<String> {
    p.live.places.as_ref().map(|x| x.coord.trim_end_matches('/')).filter(|c| !c.is_empty()).map(|c| format!("{c}/agents/{name}/home"))
}

/// P's repositories by name with their folders: coordd's resolved ones, else the file's as written (an older coordd).
pub fn resolved_repos(p: &Proj) -> Vec<(String, String)> {
    match p.live.places.as_ref().map(|x| &x.repos).filter(|r| !r.is_empty()) {
        Some(r) => r.clone(),
        None => repos(p).into_iter().map(|(n, f, _)| (n, f)).collect(),
    }
}

/// Where New agent starts P's agents, PATH on the coordds' machine, said with no folder's path in it (docket-edge7's
/// walk N-1446 p3050, 4: "Home names a Linux folder: New agents work in ~/agent-coord/docket-try/walk"; paper-research
/// p3051): in P's own coord folder (`~/agent-coord/ID`, new-project; files.rs `roots`) it is the project's own folder;
/// elsewhere, the folder by its name, as a repository's row names it.
fn place_word(p: &Proj, path: &str) -> String {
    let path = path.trim_end_matches('/');
    let coord = p.live.places.as_ref().map(|x| x.coord.trim_end_matches('/')).filter(|c| !c.is_empty());
    if coord.is_some_and(|c| path == c || path.strip_prefix(c).is_some_and(|r| r.starts_with('/'))) {
        return "the project's own folder".into();
    }
    format!("the folder {}", path.rsplit('/').next().filter(|x| !x.is_empty()).unwrap_or(path))
}

/// P's open topics, the newest first (outline.rs `pieces`: every open top thread).
fn topics(p: &Proj) -> Vec<&TicketView> {
    let mut v: Vec<&TicketView> = crate::outline::pieces(p, None).into_iter().map(|(g, _)| g).collect();
    v.sort_by(|a, b| b.opened_at.cmp(&a.opened_at).then(b.n.cmp(&a.n)));
    v
}

/// Home's page for P, project PI: repository and agent cards on top, Threads beside Usage, Storage and Settings below.
pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let pi = s.place_of(p);
    let heading = |t: String| text(t).font(k.ty.small_medium).color(k.c.text_muted);
    let quiet = |t: String| text(t).font(k.ty.small).color(k.c.text_faint).truncate().shrink();
    let name = |t: String| text(t).font(k.ty.body).color(k.c.text_base).truncate().shrink();
    let line = |key: String, left: El, right: El| row().key(key).w_full().gap(12.0).center_y().py(4.0).child(col().grow(1.0).shrink().min_w(0.0).child(left)).child(right);
    // a line of its own that is no row to press, as large as the rows
    let said = |t: String| text(t).font(k.ty.body).color(k.c.text_muted).truncate().shrink();
    // a heading's count, none when its line under it says there is none
    let titled = |word: &str, n: usize| if n == 0 { word.to_string() } else { crate::refs::count(word, n) };
    let head = |word: String, action: Option<El>| row().w_full().center_y().child(col().grow(1.0).child(heading(word))).children(action);
    let cards = crate::home_cards::grid;

    // its repositories as cards, Add a repository beside the heading for a coordd that takes it
    let states = s.repo_state.shown.get(&p.id);
    let rs = shown_repos(p, states.map_or(&[], Vec::as_slice));
    let add = (p.protocol >= coord_types::ADD_REPO_PROTOCOL).then(|| Button::new("Add a repository…").variant(ButtonVariant::Ghost).on_press(add_repo_open).build(k).key("home-add-repo"));
    let mut repos_el = col().key("home-repos").w_full().gap(8.0).child(head(titled("Repositories", rs.len()), add));
    if rs.is_empty() {
        repos_el = repos_el.child(line("home-no-repo".into(), said("No repository yet".into()), quiet(String::new())));
    }
    let repo_cards = rs.iter().map(|(n, folder, marked)| crate::home_cards::repo(s, pi, n, folder, *marked, about(p, n), states.and_then(|v| v.iter().find(|x| x.name == *n)), k)).collect();
    if !rs.is_empty() {
        repos_el = repos_el.child(cards("home-repos-grid", repo_cards));
    }
    // New agent starts in none of them: where it does
    let own = p.live.places.as_ref().is_some_and(|x| x.own_homes);
    if let Some(w) = start(p).filter(|_| !rs.iter().any(|r| r.2)).map(|st| place_word(p, st)).or_else(|| own.then(|| "their own homes".to_string())) {
        repos_el = repos_el.child(line("home-start".into(), said(format!("New agents work in {w}")), quiet(String::new())));
    }

    // its agents as cards: those that run or wait, the stopped ones as a count under them
    let (live, stopped): (Vec<&AgentView>, Vec<&AgentView>) = p.live.agent_tree().into_iter().map(|(_, a)| a).partition(|a| a.state != "stopped");
    let new_agent = Button::new("New agent").variant(ButtonVariant::Ghost).on_press(crate::projects::new_agent_open).build(k).key("home-new-agent");
    let mut agents_el = col().key("home-agents").w_full().gap(8.0).child(head(titled("Agents", live.len()), Some(new_agent)));
    if live.is_empty() {
        let none = if stopped.is_empty() { "No agent yet: New agent starts one" } else { "No running agents" };
        agents_el = agents_el.child(line("home-no-agent".into(), said(none.into()), quiet(String::new())));
    }
    if !live.is_empty() {
        agents_el = agents_el.child(cards("home-agents-grid", live.iter().map(|a| crate::home_cards::agent(s, pi, a, k)).collect()));
    }
    if !stopped.is_empty() {
        let words = if live.is_empty() { format!("{} stopped", stopped.len()) } else { format!("and {} stopped", stopped.len()) };
        agents_el = agents_el.child(quiet(words));
    }

    // its threads, the newest open topics, as a list
    let all = topics(p);
    let all_threads = Button::new("All threads").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.sel = Sel::Tickets).build(k).key("home-all-threads");
    let mut threads_el = col().key("home-threads").w_full().gap(2.0).child(head(titled("Threads", all.len()), Some(all_threads)));
    if all.is_empty() {
        threads_el = threads_el.child(line("home-no-thread".into(), said("No thread yet".into()), quiet(String::new())));
    }
    for v in all.iter().take(TOPICS) {
        let (n, id) = (v.n, v.id.clone());
        threads_el = threads_el.child(k.interactive(line(format!("home-thread-{}", v.id), name(crate::outline::label(p, v)), quiet(v.id.clone()))).on_press(move |s: &mut App| crate::find::open_at(s, n, &id, None)));
    }

    // what the project's agents have used, stopped ones too
    let usage: Vec<coord_types::ModelUsage> = p.live.agents.iter().flat_map(|a| a.usage.iter().cloned()).collect();
    let usage_el = (!usage.is_empty()).then(|| col().key("home-usage").w_full().gap(2.0).child(heading("Usage".into())).child(row().py(4.0).child(crate::spend::spend(k, &crate::agents::spend_of(s, &usage)))));
    // where this project's space goes; the whole store and the machines are the window's Home
    let store_el = crate::store::section(s, p, k, &heading, false);
    let settings_el = col().key("home-settings").w_full().gap(2.0).child(heading("Settings".into())).child(crate::global::link(k, "home-settings-open", "This project's settings", |s: &mut App| crate::policy::open(s, crate::policy::Scope::Project)));
    let layers_el = crate::layers::section(s, k, &heading);

    // below the cards: Threads on the left, the rest stacked on the right; one column when the window is narrow
    let panel = |e: El| crate::home_cards::panel(k, e);
    let side = col().key("home-side").grow(1.0).min_w(300.0).gap(16.0).children(usage_el.map(panel)).children(store_el.map(panel)).child(panel(settings_el)).children(layers_el.map(panel));
    let lower = row().key("home-lower").w_full().flex_wrap().gap(16.0).items_start().child(col().grow(1.5).min_w(380.0).child(panel(threads_el))).child(side);
    let body = col().w_full().max_w(1080.0).gap(28.0).px(24.0).py(20.0).child(repos_el).child(agents_el).child(lower);
    col().key("home").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(body)
}

/// The repositories Overview lists: P's file's, else those the reader of STATES says (a v2 project has no file).
fn shown_repos(p: &Proj, states: &[RepoState]) -> Vec<(String, String, bool)> {
    let rs = repos(p);
    if !rs.is_empty() {
        return rs;
    }
    states.iter().map(|x| (x.name.clone(), x.path.clone().unwrap_or_default(), false)).collect()
}

/// What git says of one repository now (coordd's `Request::RepoState`), each field it did not give left out.
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq)]
pub struct RepoState {
    pub name: String,
    pub path: Option<String>,
    /// Why the reader would not look in it (a place kept out); none for the rest.
    pub refused: Option<String>,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub subject: Option<String>,
    pub dirty: Option<u64>,
    pub ahead: Option<u64>,
    pub behind: Option<u64>,
    pub pdf: Option<String>,
    /// When its last commit was made (RFC 3339).
    pub at: Option<String>,
}

/// The repositories' state by project id, and when each was last asked.
#[derive(Default)]
pub struct RepoStates {
    pub shown: std::collections::HashMap<String, Vec<RepoState>>,
    pub asked: std::collections::HashMap<String, std::time::Instant>,
}

/// While Home shows: project PI's repositories asked every [`REPO_EVERY`].
pub fn ask_repo_state(s: &mut App, pi: usize) {
    let Some(p) = s.projects.get(pi) else { return };
    let reader = crate::machine::reads(s, pi);
    if (!reader && (p.protocol < coord_types::HOME_PROTOCOL || p.link.is_none())) || s.repo_state.asked.get(&p.id).is_some_and(|t| t.elapsed() < REPO_EVERY) {
        return;
    }
    let id = p.id.clone();
    s.repo_state.asked.insert(id.clone(), std::time::Instant::now());
    if reader {
        crate::machine::ask(s, crate::machine::Ask::Repos(id), 0);
    } else {
        s.request_in(pi, Request::RepoState, crate::Pending::RepoState { project: id });
    }
}

/// coordd's answer R for project ID; a refusal keeps what was shown.
pub fn repo_state_came(s: &mut App, id: String, r: Result<String, String>) {
    if let Some(v) = r.ok().and_then(|t| serde_json::from_str::<Vec<RepoState>>(&t).ok()) {
        s.repo_state.shown.insert(id, v);
    }
}

/// Repository NAME at FOLDER shown in Files: the code view, its repository in the Explorer, where project PI's coordd
/// serves it; else the Files page with its folder open.
pub fn show_files(s: &mut App, pi: usize, name: &str, folder: &str) {
    if crate::code::served(s, pi) {
        crate::code::open(s, pi, coord_types::codeview::CodeTarget::Repo { path: name.to_string() });
    } else {
        crate::files::open_folder(s, name, folder);
    }
}

/// Repository NAME's Files button, as the Repos drawer has it: the code view where coordd serves it, else Files.
pub fn files_button(pi: usize, name: &str, folder: &str, k: &Kit) -> El {
    let (n, f) = (name.to_string(), folder.to_string());
    Button::new("Files").variant(ButtonVariant::Ghost).help(format!("{name} in Files")).on_press(move |s: &mut App| show_files(s, pi, &n, &f)).build(k)
}

/// The Add a repository dialog (p2452): FOLDER on coordd's machine, `~/` or whole; NAME, its last part unless he
/// names it; asked of coordd as `Request::AddRepo`, whose checks decide and whose refusal the dialog says in its words.
#[derive(Default)]
pub struct AddRepo {
    pub open: bool,
    pub project: usize,
    pub folder: TextEdit,
    pub name: TextEdit,
    pub running: Option<Receiver<Result<String, String>>>,
    pub error: Option<String>,
}

/// The name a repository at FOLDER takes when he names none: its last part.
fn last_part(folder: &str) -> String {
    folder.trim().trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string()
}

/// The request the dialog A holds for project P, or why it is not one yet: a folder from `~/` or whole, a plain name,
/// not a repository of P already. What coordd checks beyond it (a git repository's top, a place never served) comes
/// back as its refusal.
fn add_repo_request(a: &AddRepo, p: &Proj) -> Result<Request, String> {
    let folder = a.folder.text.trim();
    if !(folder.starts_with("~/") || folder.starts_with('/')) {
        return Err("A full path, or from ~/".into());
    }
    let name = match a.name.text.trim() {
        "" => last_part(folder),
        n => n.to_string(),
    };
    if !crate::projects::agent_name_ok(&name) {
        return Err("Its name: letters, digits, - and _ only.".into());
    }
    if repos(p).iter().any(|(n, _, _)| *n == name) {
        return Err(format!("{name} is a repository of {} already.", p.name));
    }
    Ok(Request::AddRepo { name, path: folder.to_string() })
}

fn add_repo_open(s: &mut App) {
    if s.add_repo.running.is_none() {
        s.add_repo = AddRepo { open: true, project: s.cur, ..Default::default() };
    }
    s.add_repo.open = true;
}

/// Ask coordd for the repository the dialog holds.
fn add_repo_now(s: &mut App) {
    let a = &s.add_repo;
    if a.running.is_some() {
        return;
    }
    let Some(p) = s.projects.get_mut(a.project) else { return };
    let asked = add_repo_request(a, p).and_then(|r| p.ask_roster(&r).ok_or_else(|| format!("{} is not reached now.", p.name)));
    match asked {
        Ok(rx) => (s.add_repo.running, s.add_repo.error) = (Some(rx), None),
        Err(e) => s.add_repo.error = Some(e),
    }
}

/// The dialog's ask, once coordd answered: closed (the snapshot coordd sends brings the repository onto Home), or its
/// refusal said in the dialog; true when it answered.
pub fn add_repo_ended(s: &mut App) -> bool {
    let a = &mut s.add_repo;
    let Some(r) = a.running.as_ref().and_then(|rx| rx.try_recv().ok()) else { return false };
    a.running = None;
    match r {
        Ok(_) => a.open = false,
        Err(e) => a.error = Some(e),
    }
    true
}

/// The Add a repository dialog, into LAYER while it is open.
pub fn add_repo_dialog(s: &App, k: &Kit, layer: &mut Vec<El>) {
    let a = &s.add_repo;
    if !a.open {
        return;
    }
    let running = a.running.is_some();
    let folder = TextInput::new(&a.folder)
        .placeholder(if cfg!(windows) { "~/code/my-repo (on the WSL machine)" } else { "~/code/my-repo" })
        .label("Folder")
        .width(None)
        .disabled(running)
        .on_edit(|s: &mut App| &mut s.add_repo.folder)
        .on_submit(|s: &mut App, _| add_repo_now(s))
        .build(k);
    let last = last_part(&a.folder.text);
    let name = TextInput::new(&a.name)
        .placeholder(if last.is_empty() { "its folder's name" } else { last.as_str() })
        .label("Name")
        .width(None)
        .disabled(running)
        .on_edit(|s: &mut App| &mut s.add_repo.name)
        .on_submit(|s: &mut App, _| add_repo_now(s))
        .build(k);
    let note = match (&a.error, running) {
        (_, true) => ("Adding it…".to_string(), k.c.text_muted),
        (Some(e), _) => (e.clone(), k.c.danger.fg),
        (None, _) => ("A git repository's top folder".to_string(), k.c.text_faint),
    };
    let go = Button::new(if running { "Adding…" } else { "Add" })
        .variant(ButtonVariant::Contrast)
        .disabled(running || a.folder.text.trim().is_empty())
        .on_press(add_repo_now)
        .build(k)
        .key("add-repo-go");
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).disabled(running).on_press(|s: &mut App| s.add_repo.open = false).build(k);
    let body = col()
        .w_full()
        .px(16.0)
        .pb(16.0)
        .gap(12.0)
        .child(folder)
        .child(name)
        .child(text(note.0).font(k.ty.small).color(note.1).wrap().key("add-repo-note"))
        .child(row().w_full().justify_end().gap(8.0).child(cancel).child(go));
    Dialog::new("add-repo", "Add a repository")
        .open(true)
        .fit()
        .width(420.0)
        .child(body)
        .on_close(|s: &mut App| {
            if s.add_repo.running.is_none() {
                s.add_repo.open = false;
            }
        })
        .build(k, layer);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rune::Runtime;
    use serde_json::json;

    fn view(v: Value) -> TicketView {
        let mut base = json!({"kind": "thread", "ctx": {}, "form": {}, "title": "", "status": {"is": "resting"}, "children": [], "program": "", "bound": 5, "spent": 0, "given": [], "posts": []});
        for (k, x) in v.as_object().unwrap() {
            base[k] = x.clone();
        }
        serde_json::from_value(base).unwrap()
    }

    fn agent(i: u32, name: &str, state: &str) -> AgentView {
        serde_json::from_value(json!({"id": format!("AGT-{i}"), "name": name, "parent": "owner", "role": "", "kind": "claude", "model": "claude-opus-5-5", "state": state,
            "since": "2026-11-24T08:00:00Z", "created": "2026-11-24T08:00:00Z", "last_active": "2026-11-24T08:00:00Z", "last_tool": null, "tools": [], "items": [], "transcript": null}))
        .unwrap()
    }

    pub(crate) fn settle(rt: &mut Runtime<App>) {
        rt.invalidate();
        for _ in 0..3 {
            rt.frame(0.016);
        }
    }

    pub(crate) fn keyed(rt: &mut Runtime<App>, key: &str) {
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn keys(rt: &Runtime<App>, pre: &str) -> Vec<String> {
        let mut v: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.key).filter(|k| k.starts_with(pre)).collect();
        v.dedup();
        v
    }

    pub(crate) fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
    }

    /// A project as New project makes it: no project file, two agents running and one stopped, two threads.
    pub(crate) fn made() -> (std::sync::MutexGuard<'static, ()>, App) {
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "walk".into(), id: "walk".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::PROTOCOL;
        for v in [
            view(json!({"id": "N-1", "n": 1, "opener": "owner", "form": {"to": "try-1"}, "title": "First words", "opened_at": "2026-11-24T08:01:00Z"})),
            view(json!({"id": "N-2", "n": 2, "opener": "owner", "form": {"to": "try-1"}, "title": "The lexer", "opened_at": "2026-11-24T08:05:00Z"})),
        ] {
            p.live.apply(coord_types::Event { seq: v.n, body: coord_types::Body::Ticket(v) });
        }
        p.live.agents = vec![agent(1, "try-1", "idle"), agent(2, "try-2", "working"), agent(3, "old", "stopped")];
        s.projects = vec![p];
        s.cur = 0;
        (held, s)
    }

    #[test]
    fn a_project_made_without_a_file_opens_on_home_with_no_repository_its_agents_and_its_threads() {
        let (_held, mut s) = made();
        assert!(here(&s.projects[0]), "no file, no home view: Home is its home");
        assert_eq!(crate::panes::project_tabs(&s.projects[0]).first().map(|(l, t)| (l.as_str(), t.clone())), Some(("Overview", Sel::View(String::new()))), "Overview is the first tab");
        // as New project leaves it: the project just made is shown, on its Home
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Ok(crate::projects::ProjectConfig { name: "walk-b".into(), id: "walk-b".into(), boxed: false })).unwrap();
        s.new_project.running = Some(rx);
        s.sel = Sel::Agents;
        assert!(s.new_project_done());
        assert_eq!((s.cur, &s.sel), (1, &Sel::View(String::new())), "the new project opens on Home");
        s.projects.truncate(1);
        s.cur = 0;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "No repository yet"), "{t:?}");
        assert!(keys(&rt, "home-repos-grid").is_empty(), "no empty grid under it");
        assert!(t.iter().any(|x| x == "Repositories") && !t.iter().any(|x| x.starts_with("Repositories ·")), "none said once, not counted too: {t:?}");
        assert!(t.iter().any(|x| x == "Agents · 2") && t.iter().any(|x| x == "and 1 stopped"), "{t:?}");
        // each agent's model by his name for it, as New agent lists it (paper-research N-1446 p3207, 3), not its id
        assert!(t.iter().filter(|x| *x == "Opus 5.5").count() == 2 && !t.iter().any(|x| x == "claude-opus-5-5"), "the plain model name: {t:?}");
        assert_eq!(keys(&rt, "home-thread-"), ["home-thread-N-2", "home-thread-N-1"], "the newest first");
        assert!(keys(&rt, "home-new-agent").len() == 1 && keys(&rt, "home-all-threads").len() == 1);
        assert!(!t.iter().any(|x| x.starts_with("the project has no view")), "{t:?}");
        assert!(keys(&rt, "home-start").is_empty(), "a coordd that sends no start: no line");
        // coordd's one rule (places.start): a project made in New project with no folder starts each agent in its home
        rt.update(|s, _| s.projects[0].live.places = Some(coord_types::Places { coord: "/home/u/agent-coord/walk".into(), own_homes: true, ..Default::default() }));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "No repository yet") && t.iter().any(|x| x == "New agents work in their own homes"), "{t:?}");

        // the running agents as cards, the stopped ones a count
        let grid = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("home-agents-grid")).unwrap().rect;
        let cards: Vec<[f32; 4]> = rt.elements().into_iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("home-agent-"))).map(|e| e.rect).collect();
        assert!(cards.len() == 2 && cards.iter().all(|r| r[1] >= grid[1] && r[2] < 300.0) && cards[0][1] == cards[1][1], "two small cards side by side: {cards:?}");
        assert!(!t.iter().any(|x| x == "working" || x == "idle"), "the state is the dot's colour, not words too: {t:?}");
        // a press on an agent's card opens its page; on a thread, the thread
        keyed(&mut rt, "home-agent-try-2");
        assert_eq!(rt.state.sel, Sel::Agent("try-2".into()));
        rt.update(|s, _| s.sel = Sel::View(String::new()));
        settle(&mut rt);
        keyed(&mut rt, "home-thread-N-1");
        assert_eq!((rt.state.sel.clone(), rt.state.tickets.shown), (Sel::Tickets, Some(1)));
    }

    /// docket-edge7's walk of a new project on a real engine (N-1446 p3050, 4; paper-research p3051): Home said "New
    /// agents work in ~/agent-coord/docket-try/walk", a Linux folder. It names none: where agents work in words, a
    /// repository by its name.
    #[test]
    fn a_new_projects_home_names_no_folder() {
        let (_held, mut s) = made();
        s.projects[0].live.places = Some(coord_types::Places { coord: "/home/u/agent-coord/walk".into(), start: Some("/home/u/agent-coord/walk/work".into()), ..Default::default() });
        s.projects[0].live.project = json!({"project": {"repos": {"tovrin": "/home/u/src/tovrin"}, "repo": "elsewhere"}});
        s.sel = Sel::View(String::new());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "New agents work in the project's own folder"), "{t:?}");
        assert!(keys(&rt, "home-repo-") == ["home-repo-tovrin"] && t.iter().any(|x| x == "tovrin"), "the repository by its name: {t:?}");
        assert!(!t.iter().any(|x| x.contains("/home/u") || x.contains("~/") || x.contains("agent-coord")), "no folder on Home: {t:?}");
    }

    /// p3050, 7 (paper-research p3051): the left bar said "Threads 0" with the project's thread open in it, counting
    /// only what waits on him. It counts the open threads, as the Threads page's All active does.
    #[test]
    fn the_left_bars_threads_counts_the_open_threads() {
        let (_held, mut s) = made();
        s.sel = Sel::View(String::new());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(crate::waits::on_owner(&rt.state.projects[0]).is_empty(), "nothing waits on him");
        let row = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("tickets")).expect("the left bar's Threads").rect;
        let inside = |r: [f32; 4]| r[0] >= row[0] && r[1] >= row[1] && r[0] + r[2] <= row[0] + row[2] + 0.5 && r[1] + r[3] <= row[1] + row[3] + 0.5;
        let said: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && inside(e.rect)).filter_map(|e| e.text).collect();
        assert!(said.iter().any(|x| x == "Threads") && said.iter().any(|x| x == "2") && !said.iter().any(|x| x == "0"), "Threads 2, its two open threads: {said:?}");
    }

    /// An agent's page: one row of its stream, context ring, drawers and ⋯; the rest is ⋯'s menu, one column, each pick
    /// shutting it.
    #[test]
    fn an_agents_page_opens_with_with_you_the_rest_behind_its_dots() {
        let (_held, mut s) = made();
        s.sel = Sel::Agent("try-1".into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let rest = ["With agents", "Raw log", "System prompt", "Initial prompt", "Home", "Session"];
        // the pane's own words, right of the sidebar (whose Home row is the window's)
        let drawn = |rt: &Runtime<App>| -> Vec<(String, f32, f32)> {
            let els = rt.elements();
            let edge = els.iter().find(|e| e.key.as_deref() == Some("global-home")).map_or(0.0, |e| e.rect[0] + e.rect[2]);
            let mut v: Vec<(String, f32, f32)> = els.into_iter().filter(|e| !e.exiting && e.rect[0] >= edge).filter_map(|e| Some((e.text?, e.rect[0], e.rect[1]))).filter(|x| rest.contains(&x.0.as_str())).collect();
            v.dedup_by(|a, b| a.0 == b.0);
            v
        };
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "With you") && t.iter().any(|x| x == "⋯"), "{t:?}");
        assert!(["Repos", "Inspect", "Terminal"].iter().all(|d| t.iter().any(|x| x == d)), "its drawers' toggles: {t:?}");
        assert!(drawn(&rt).is_empty(), "the rest behind ⋯: {:?}", drawn(&rt));
        keyed(&mut rt, "agent-more-try-1");
        let open = drawn(&rt);
        assert_eq!(open.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), rest, "⋯ opens its menu");
        assert!(open.windows(2).all(|w| w[0].1 == w[1].1 && w[0].2 < w[1].2), "one column, not a row: {open:?}");
        keyed(&mut rt, "agent-more-try-1");
        assert!(drawn(&rt).is_empty(), "a second press shuts it: {:?}", drawn(&rt));
        // a pick: the other stream shown, the menu shut
        keyed(&mut rt, "agent-more-try-1");
        let at = drawn(&rt).into_iter().find(|x| x.0 == "With agents").unwrap();
        rt.pointer_move(at.1 + 4.0, at.2 + 4.0);
        rt.frame(0.016);
        rt.pointer_down(at.1 + 4.0, at.2 + 4.0);
        rt.frame(0.016);
        rt.pointer_up(at.1 + 4.0, at.2 + 4.0);
        settle(&mut rt);
        assert_eq!(rt.state.streams.which.get(&(0, "try-1".to_string())), Some(&crate::streams::Which::Agents));
        assert!(rt.state.streams.more.is_empty() && texts(&rt).iter().any(|x| x == "With agents"), "the stream it shows, the menu shut");
    }

    #[test]
    fn home_lists_the_files_repositories_by_name_and_marks_where_agents_start() {
        let (_held, mut s) = made();
        s.projects[0].live.project = json!({"project": {"name": "walk", "repos": {"tovrin": "../tovrin", "p4": "../p4"}, "repo": "tovrin"}});
        let p = &s.projects[0];
        assert_eq!(repos(p), [("p4".to_string(), "../p4".to_string(), false), ("tovrin".to_string(), "../tovrin".to_string(), true)]);
        s.projects[0].live.project = json!({"project": {"repos": {"tovrin": "../tovrin", "p4": "../p4"}}});
        assert_eq!(repos(&s.projects[0]).iter().filter(|r| r.2).map(|r| r.0.as_str()).collect::<Vec<_>>(), ["p4"], "no repo: the first by name, as coordd takes it");
        s.projects[0].live.project = json!({"project": {"repo": "~/work/walk"}});
        assert_eq!(repos(&s.projects[0]), [("walk".to_string(), "~/work/walk".to_string(), true)]);
        // from coordd's start: marked when New agent starts in the file's pick (places.repo), else none
        s.projects[0].live.project = json!({"project": {"repos": {"tovrin": "../tovrin", "p4": "../p4"}, "repo": "tovrin"}});
        s.projects[0].live.places = Some(coord_types::Places { coord: "/home/u/agent-coord/walk".into(), repo: Some("/home/u/tovrin".into()), start: Some("/home/u/tovrin/".into()), ..Default::default() });
        assert_eq!(repos(&s.projects[0]).iter().filter(|r| r.2).map(|r| r.0.as_str()).collect::<Vec<_>>(), ["tovrin"], "start is the pick");
        s.projects[0].live.places.as_mut().unwrap().start = Some("/home/u/agent-coord/walk".into());
        assert!(repos(&s.projects[0]).iter().all(|r| !r.2), "start is none of them: none marked");
        s.projects[0].live.places = None;
        // a file with a home view keeps it as its home; Overview is still its first tab, by its own id
        s.projects[0].live.project = json!({"project": {"home": "home"}, "view": [{"id": "home", "kind": "overview", "items": "", "runner": ""}]});
        assert!(!here(&s.projects[0]));
        let tabs: Vec<(String, Sel)> = crate::panes::project_tabs(&s.projects[0]).into_iter().take(2).collect();
        assert_eq!(tabs, [("Overview".to_string(), Sel::View(ID.into())), ("home".to_string(), Sel::View("home".into()))]);
        s.projects[0].live.project = json!({"project": {"repos": {"tovrin": "../tovrin"}}});
        s.sel = Sel::View(String::new());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert_eq!(keys(&rt, "home-repo-"), ["home-repo-tovrin"]);
        assert_eq!(keys(&rt, "home-start-dot-"), ["home-start-dot-tovrin"], "the dot where agents start");
        rt.update(|s, _| s.projects[0].live.places = Some(coord_types::Places { coord: "/c".into(), repo: Some("/srv/tovrin".into()), start: Some("/srv/tovrin".into()), ..Default::default() }));
        settle(&mut rt);
        assert!(keys(&rt, "home-start-dot-") == ["home-start-dot-tovrin"] && keys(&rt, "home-start").iter().all(|k| k.starts_with("home-start-dot-")));
        // New agent starts elsewhere (coordd's rule over the file's): Home says where, the folder by its name (p3050, 4)
        rt.update(|s, _| s.projects[0].live.places.as_mut().unwrap().start = Some("/srv/other".into()));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(keys(&rt, "home-start-dot-").is_empty() && t.iter().any(|x| x == "New agents work in the folder other"), "{t:?}");
        // a file with a home view: the Project entry opens it, and Home's tab draws Home
        rt.update(|s, _| {
            s.projects[0].live.project = json!({"project": {"home": "home", "repos": {"tovrin": "../tovrin"}}, "view": [{"id": "home", "kind": "overview", "items": "", "runner": ""}]});
            (s.sel, s.project_tab) = (Sel::Agents, None);
        });
        settle(&mut rt);
        keyed(&mut rt, "project-pages");
        assert_eq!(rt.state.sel, Sel::View("home".into()), "the entry opens the file's home");
        assert!(keys(&rt, "home-repo-").is_empty());
        rt.update(|s, _| s.sel = Sel::View(ID.into()));
        settle(&mut rt);
        assert_eq!(keys(&rt, "home-repo-"), ["home-repo-tovrin"], "Home's tab");
    }

    /// Each repository is a card: its name, its branch and how many files changed, nothing more; no path, no commit,
    /// no prose; a press on the card opens it, never folds it.
    #[test]
    fn a_repository_is_a_card_its_name_and_a_quiet_branch_and_a_press_opens_it() {
        let (_held, mut s) = made();
        let d = std::env::temp_dir().join(format!("yaaihh-home-card-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        s.projects[0].live.project = json!({"project": {"repos": {"rune": "/src/rune", "yaaihh": d.display().to_string()}, "about": {"yaaihh": "the window"}}});
        s.repo_state.shown.insert("walk".into(), vec![
            RepoState { name: "yaaihh".into(), path: Some(d.display().to_string()), branch: Some("main".into()), head: Some("3f2a1c9".into()), subject: Some("Home: one line".into()), dirty: Some(2), pdf: Some(".build/paper.pdf".into()), ..Default::default() },
            RepoState { name: "rune".into(), refused: Some("a place kept out".into()), ..Default::default() },
        ]);
        s.sel = sel(&s.projects[0]);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "main · 2 changed") && t.iter().any(|x| x == "not read"), "{t:?}");
        // what the repositories' cards say, and nothing more: rune, first by name, is where agents start
        let box_ = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("home-repos-grid")).unwrap().rect;
        let inside = |r: [f32; 4]| r[0] >= box_[0] && r[1] >= box_[1] && r[0] + r[2] <= box_[0] + box_[2] + 0.5 && r[1] + r[3] <= box_[1] + box_[3] + 0.5;
        let said: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && inside(e.rect)).filter_map(|e| e.text).filter(|x| !x.trim().is_empty()).collect();
        assert_eq!(said, ["●", "rune", "not read", "yaaihh", "PDF", "main · 2 changed"], "names, a PDF chip and a quiet branch: no path, commit, prose or fold");
        // the owner's N-2003 (3): a PDF chip only where there is one, a Files icon on each that opens, a branch mark
        assert_eq!(keys(&rt, "home-pdf-"), ["home-pdf-yaaihh"], "a PDF chip only where there is one");
        assert_eq!(keys(&rt, "home-files-"), ["home-files-rune", "home-files-yaaihh"]);
        assert_eq!(keys(&rt, "home-branch-"), ["home-branch-yaaihh"], "a branch mark where git gave a branch");
        keyed(&mut rt, "home-pdf-yaaihh");
        assert_eq!(rt.state.sel, Sel::Files, "the PDF chip opens the paper, in one press");
        assert!(rt.state.files.kept["walk"].shown.as_deref().is_some_and(|f| f.ends_with(".build/paper.pdf")));
        rt.update(|s, _| s.sel = sel(&s.projects[0]));
        settle(&mut rt);
        keyed(&mut rt, "home-repo-yaaihh");
        assert_eq!(rt.state.code.error.as_deref(), Some("walk is not reached now."), "where coordd serves the code view, the card asks it");
        // below the code view: the card opens the Files page on its folder
        rt.update(|s, _| {
            (s.projects[0].protocol, s.code.error) = (coord_types::CODE_VIEW_PROTOCOL - 1, None);
            s.sel = sel(&s.projects[0]);
        });
        settle(&mut rt);
        keyed(&mut rt, "home-repo-yaaihh");
        assert_eq!(rt.state.sel, Sel::Files, "the card opens the repository");
        let real = std::fs::canonicalize(&d).unwrap();
        assert!(rt.state.files.roots["walk"].iter().any(|r| r.name == "yaaihh" && r.path == real) && rt.state.files.kept["walk"].open.contains(&real.display().to_string()), "its folder open in Files");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A repository's row opens it in Files (paper-research N-1446 p3387, 3; the owner p1213 "the proejct page with
    /// the repos etc the fiels workign"): from a coordd with no code view, the repository is Files' first root, by
    /// Home's name for it, open, its files listed; on Windows, coordd's root of that path is the one opened.
    #[test]
    fn a_repositorys_row_opens_it_in_files() {
        let (_held, mut s) = made();
        let d = std::env::temp_dir().join(format!("yaaihh-home-repo-{}", std::process::id()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("README.md"), "soft-swarm\n").unwrap();
        s.projects[0].live.project = json!({"project": {"name": "yaaihh", "repos": {"soft-swarm": d.display().to_string()}}});
        s.projects[0].protocol = coord_types::REPO_VIEW_PROTOCOL;
        s.sel = sel(&s.projects[0]);
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(!texts(&rt).iter().any(|x| x.contains("Klaus")), "nothing opens outside the window: {:?}", texts(&rt));
        keyed(&mut rt, "home-repo-soft-swarm");
        assert_eq!(rt.state.sel, Sel::Files);
        let real = std::fs::canonicalize(&d).unwrap();
        let roots = rt.state.files.roots["walk"].clone();
        assert_eq!((roots[0].name.as_str(), &roots[0].path), ("soft-swarm", &real), "{roots:?}");
        assert!(rt.state.files.kept["walk"].open.contains(&real.display().to_string()), "open");
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "README.md") && t.iter().any(|x| x == "src"), "its files listed: {t:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Overview's repository row opens nothing outside the window; on a coordd without the code view its root in Files
    /// has no button to Klaus or anywhere else.
    #[test]
    fn a_repositorys_row_and_its_root_in_files_stay_in_the_window() {
        let (_held, mut s) = made();
        let d = std::env::temp_dir().join(format!("yaaihh-home-view-{}", std::process::id()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        s.projects[0].live.project = json!({"project": {"name": "yaaihh", "repos": {"soft-swarm": d.display().to_string()}}});
        s.sel = sel(&s.projects[0]);
        s.projects[0].protocol = coord_types::REPO_VIEW_PROTOCOL;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert_eq!(keys(&rt, "home-repo-"), ["home-repo-soft-swarm"]);
        assert!(!texts(&rt).iter().any(|x| x.contains("Klaus") || x.contains("browser")), "{:?}", texts(&rt));
        keyed(&mut rt, "home-repo-soft-swarm");
        assert_eq!(rt.state.sel, Sel::Files);
        settle(&mut rt);
        assert!(!texts(&rt).iter().any(|x| x.contains("Klaus") || x.contains("browser")), "{:?}", texts(&rt));
        assert!(keys(&rt, "files-code-").is_empty(), "no button out of Files");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// On Windows a repository's row opens coordd's root of its path (p3387, 3).
    #[test]
    fn on_windows_a_repositorys_row_opens_coordds_root_of_it() {
        let (_held, mut s) = made();
        s.projects[0].live.project = json!({"project": {"repos": {"soft-swarm": "/srv/soft-swarm"}}});
        s.files.roots.insert("walk".into(), vec![crate::files::Root { name: "work".into(), path: "/srv/work".into(), here: true }, crate::files::Root { name: "soft-swarm".into(), path: "/srv/soft-swarm".into(), here: true }]);
        crate::elsewhere::as_on_windows(|| crate::files::open_folder(&mut s, "soft-swarm", "/srv/soft-swarm"));
        assert_eq!(s.sel, Sel::Files);
        assert!(s.files.kept["walk"].open.contains("/srv/soft-swarm"));
    }

    /// New project asks for no folder (accounts-lead N-1446 p2455; the owner N-1104 p999, "none ofc intitally"): its
    /// name and an optional project file; without a file its line says its agents work in its `work` until he adds a
    /// repository on its Home, where Add a repository is.
    #[test]
    fn new_project_asks_no_folder_its_repositories_are_added_on_home() {
        let (_held, mut s) = made();
        s.new_project.open = true;
        s.new_project.name = TextEdit::new("Walk C");
        // the file and the line behind More (p3207, 1; projects.rs `new_project_shows_the_file_and_folder_only_behind_more`)
        s.new_project.more = true;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Name") && t.iter().any(|x| x == "Project file") && !t.iter().any(|x| x == "Folder"), "{t:?}");
        assert!(t.iter().any(|x| x == "Kept in ~/agent-coord/walk-c"), "{t:?}");
        rt.update(|s, _| s.new_project.file = TextEdit::new("/p/project.toml"));
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Kept in ~/agent-coord/walk-c."), "a file given: its folder is the file's to say");
    }

    /// Home's Devices by physical host (N-1860 p2), as the reader names them: the Laptop (his window's machine and WSL)
    /// and the Tower (its build VMs), each one card with one line of what runs there; a VM out of reach says so in words; the red dot only for
    /// a degraded disk; the `⋯` opens Hardware & Disk Health and Running here, a second press closes it.
    #[test]
    fn home_lists_the_devices_marks_those_that_need_him_and_a_press_opens_one() {
        let (_held, mut s) = made();
        s.sel = Sel::View(crate::global::ID.into());
        s.dark = Some(true);
        s.projects[0].live.agents[1].jobs = vec![serde_json::from_value(json!({"id": "j1", "verb": "test", "machine": "yaaihh-build-1", "since": "2026-11-24T17:00:00Z", "step": "compiling", "step_since": "2026-11-24T17:00:10Z"})).unwrap()];
        crate::devices::came(&mut s, Ok(crate::devices::tests::answer()));
        s.devices.this_pc = Some(crate::devices::tests::me());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Devices · 2"), "{t:?}");
        assert_eq!(keys(&rt, "host-more-"), ["host-more-Laptop", "host-more-Tower"]);
        assert!(t.iter().any(|x| x == "yaaihh window · WSL engine"), "{t:?}");
        assert!(t.iter().any(|x| x == "build VM yaaihh-build-1 · build machine yaaihh-build-2"), "{t:?}");
        assert!(t.iter().any(|x| x.starts_with("yaaihh-build-2: no answer")), "{t:?}");
        assert!(keys(&rt, "host-mark-").is_empty(), "no disk degraded: no dot");
        let services = "engine research runs · usage reader does not run";
        assert!(!t.iter().any(|x| x == services || x == "Hardware & Disk Health"), "closed at first: {t:?}");
        keyed(&mut rt, "host-more-Laptop");
        let t = texts(&rt);
        for said in ["Hardware & Disk Health", "31.7 GB", "25 GB free of 953", "Running here", "Windows", "WSL", "server of walk (2 agents)", services] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        let danger = rune_kit::Kit::neutral(true).c.danger.fg;
        let low = rt.elements().into_iter().find(|e| e.text.as_deref() == Some("25 GB free of 953") && e.text_color == Some(danger));
        assert!(low.is_some(), "the nearly full C: is its figure's red");
        keyed(&mut rt, "host-more-Tower");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "its own system: that needs a login there") && t.iter().any(|x| x == "1 job running"), "{t:?}");
        keyed(&mut rt, "host-more-Laptop");
        assert!(!texts(&rt).iter().any(|x| x == services), "a second press closes it");
        // a degraded disk: the dot on its host, and its line red
        rt.update(|s, _| {
            if let Some(me) = s.devices.this_pc.as_mut() {
                me.health = vec![crate::disk_health::DiskHealth { name: Some("Made-up NVMe".into()), wear_pct: Some(91.0), ..Default::default() }];
            }
        });
        settle(&mut rt);
        assert_eq!(keys(&rt, "host-mark-"), ["host-mark-Laptop"]);
        rt.update(|s, _| {
            if let crate::devices::Shown::Answer(_, age) = &mut s.devices.shown {
                *age = Some(75);
            }
        });
        settle(&mut rt);
        let old = format!("Devices · 2 · as of {}", crate::when::at("2026-11-24T17:01:02Z"));
        assert!(texts(&rt).iter().any(|x| *x == old), "{old}: {:?}", texts(&rt));
        // the same old reading at the section's first ask: being read again, so no "as of" and no out-of-reach words
        rt.update(|s, _| (s.devices.since, s.devices.open) = (Some(std::time::Instant::now()), ["Tower".to_string()].into()));
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Devices · 2") && t.iter().any(|x| x == "reading…") && !t.iter().any(|x| x.starts_with("yaaihh-build-2: no answer")), "{t:?}");
        // nothing read yet, asked: the section says it is reading
        rt.update(|s, _| s.devices.shown = crate::devices::Shown::Asking);
        settle(&mut rt);
        assert_eq!(keys(&rt, "devices-reading"), ["devices-reading"]);
    }

    /// Home's Devices as one card per machine (p3058), on the devices test's reading, 1400 wide:
    /// `STILL_OUT=DIR … home::tests::devices_cards_still --ignored --exact`.
    #[test]
    #[ignore = "a still: STILL_OUT"]
    fn devices_cards_still() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        // made-up: his machine's CPU, a healthy NVMe drive and a SATA disk with media errors (the Laptop's dot)
        fn me() -> Option<crate::devices::Device> {
            let h = |v: serde_json::Value| serde_json::from_value(v).unwrap();
            let disks = vec![
                h(json!({"name": "Made-up NVMe 1TB", "wear_pct": 3, "temp_c": 41, "critical": 0, "media_errors": 0, "power_on_hours": 1204})),
                h(json!({"name": "Made-up SATA HDD 2TB", "temp_c": 36, "critical": 0, "media_errors": 2, "power_on_hours": 31877})),
            ];
            Some(crate::devices::Device { cpu_model: Some("Intel Core i7-13700H".into()), cores: Some(20.0), health: disks, ..crate::devices::tests::me() })
        }
        fn init() -> crate::App {
            let (_held, mut s) = made();
            s.sel = Sel::View(crate::global::ID.into());
            s.projects[0].live.agents[1].jobs = vec![serde_json::from_value(json!({"id": "j1", "verb": "test", "machine": "yaaihh-build-1", "since": "2026-11-24T17:00:00Z", "step": "compiling", "step_since": "2026-11-24T17:00:10Z"})).unwrap()];
            // the build machine with made-up sensors, so the still shows the temperature row (none of ours gives them)
            let mut a: serde_json::Value = serde_json::from_str(&crate::devices::tests::answer()).unwrap();
            let mut text: serde_json::Value = serde_json::from_str(a["text"].as_str().unwrap()).unwrap();
            text["devices"][1]["temps"] = json!([{"name": "k10temp Tctl", "c": 71.5, "crit_c": 95.0}, {"name": "nvme Composite", "c": 44.0, "crit_c": 84.0}, {"name": "acpitz", "c": 40.0}]);
            text["devices"][1]["temp_c"] = json!(71.5);
            text["devices"][1]["temp_crit_c"] = json!(95.0);
            text["devices"][1]["cpu_model"] = json!("Intel Core i5-12400");
            text["devices"][1]["gpu"] = json!({"name": "NVIDIA RTX A2000", "mem_used_gb": 0.4, "mem_total_gb": 12.0});
            a["text"] = json!(text.to_string());
            crate::devices::came(&mut s, Ok(a.to_string()));
            s.devices.reader = me;
            s.devices.this_pc = me();
            s.devices.open.extend(["Laptop".to_string(), "Tower".to_string()]);
            s.notice = None;
            s
        }
        let args = vec!["--snap".to_string(), out.join("devices-cards.png").display().to_string(), "--size".into(), "1400x1300".into(), "--at".into(), "1".into()];
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 1300.0), init, crate::view) }).args(args).try_run().unwrap();
    }

    /// Home's Store (his N-1104 p1069, p1071; a2h-speed's shape p2815): under Devices, each part of the store with its
    /// machine named as Devices names it, held against cap and today's counts; a press opens its entries, a second
    /// closes them.
    #[test]
    fn home_lists_the_store_with_its_machines_and_a_press_opens_a_rows_entries() {
        let (_held, mut s) = made();
        s.sel = Sel::View(crate::global::ID.into());
        s.dark = Some(true);
        crate::devices::came(&mut s, Ok(crate::devices::tests::answer()));
        crate::store::came(&mut s, Ok(crate::store::tests::answer()));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        for said in ["Total storage", "Compiled crates", "Binaries", "60.2 GB of 60.0 GB", " · 36,214 entries · today 883 jobs, 98% from the store"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        // each row's fullness as the device cards' bar, no circle; over its cap, its held part and its bar in the
        // danger colour as a card's full disk (paper-research's read on H-1702)
        let danger = rune_kit::Kit::neutral(true).c.danger.fg;
        let els = rt.elements();
        let held = |id: &str| els.iter().find(|e| e.key.as_deref() == Some(&format!("store-held-{id}"))).and_then(|e| e.text_color);
        assert_eq!(held("crates"), Some(danger), "60.2 GB of 60.0 GB: over its cap");
        assert!(held("binaries").is_some_and(|c| c != danger), "6.2 GB of 20.0 GB: quiet");
        for id in ["crates", "binaries"] {
            assert!(els.iter().any(|e| e.key.as_deref() == Some(&format!("store-bar-{id}"))), "{id}'s bar");
        }
        assert!(els.iter().any(|e| e.label.as_deref() == Some("Compiled crates: 100%")), "the bar read out");
        assert!(t.iter().any(|x| x == "yaaihh-build-1") && t.iter().any(|x| x.contains("build VM yaaihh-build-1")), "the crates' machine named as Devices names it: {t:?}");
        keyed(&mut rt, "store-crates");
        rt.update(|s, _| {
            let page = json!({"entries": [{"made_from": "serde 1.0.217", "by": {"who": "docket-core"}, "bytes": 2_202_010}], "total": 36214}).to_string();
            crate::store::paged(s, "crates", Ok(page));
        });
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "serde 1.0.217") && t.iter().any(|x| x == "1 of 36,214"), "{t:?}");
        keyed(&mut rt, "store-crates");
        assert!(!texts(&rt).iter().any(|x| x == "serde 1.0.217"), "a second press closes it");
    }

    /// A v2 project (no coordd of v1, no project file): Home and Overview draw what the machine's reader handed them,
    /// its repositories listed from the reader's answer with Files where it gives a path.
    #[test]
    fn a_v2_project_draws_home_and_overview_from_the_readers_answers() {
        let (_held, mut s) = made();
        let p = &mut s.projects[0];
        (p.protocol, p.link) = (0, None);
        crate::devices::came(&mut s, Ok(crate::devices::tests::answer()));
        crate::store::came(&mut s, Ok(crate::store::tests::answer()));
        let repos = json!([{"name": "o1-paper", "path": "/srv/o1-paper", "branch": "main", "head": "9e1d2aa", "subject": "made-up subject", "dirty": 0}, {"name": "w1-tools", "branch": "dev", "head": "1b2c3d4", "subject": "made-up too", "dirty": 2}]);
        repo_state_came(&mut s, "walk".into(), Ok(repos.to_string()));
        s.sel = Sel::View(crate::global::ID.into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Total storage") && t.iter().any(|x| x == "Compiled crates") && t.iter().any(|x| x == "yaaihh-build-1"), "{t:?}");
        rt.update(|s, _| s.sel = sel(&s.projects[0]));
        settle(&mut rt);
        let t = texts(&rt);
        for said in ["Repositories · 2", "o1-paper", "main", "w1-tools", "dev · 2 changed"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        // no path from the reader: the card opens nothing; with one, Files
        keyed(&mut rt, "home-repo-w1-tools");
        assert_eq!(rt.state.sel, sel(&rt.state.projects[0]), "Files only where the reader gave a path");
        keyed(&mut rt, "home-repo-o1-paper");
        assert_eq!(rt.state.sel, Sel::Files);
    }

    /// What takes space (the owner's walk, items 20 and 23; N-1792 p1): Home's Store shows the walk's row with the
    /// whole and this project's part, every project's part with its bar, and this project's folders, one over 1 GB in
    /// the danger colour; a press on a folder opens its findings; Delete on a build folder asks once more, and only the
    /// second press asks coordd to delete it, as `Request::StoreFree`; an older coordd shows no Delete.
    #[test]
    fn home_shows_what_takes_space_by_project_and_whole_and_a_finding_is_deleted_on_a_second_press() {
        let (_held, mut s) = made();
        s.sel = sel(&s.projects[0]);
        s.dark = Some(true);
        let out = std::env::temp_dir().join(format!("yaaihh-store-free-{}.jsonl", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        crate::store::came(&mut s, Ok(json!({"rows": [crate::store::tests::outside()]}).to_string()));
        s.sel = Sel::View(crate::global::ID.into());
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        let at = crate::when::at("2026-11-25T20:41:57Z");
        // the window's Home: the whole and every project's part, no folder
        for said in ["Total storage", "Outside the store", &format!("9.0 GB outside the store in 3 folders · walked {at}"), "walk", "7.0 GB in 2 folders", "other", "2.0 GB in 1 folder"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        assert!(!t.iter().any(|x| x.starts_with("soft-swarm-try-1-fix")), "no folder on the window's Home: {t:?}");
        assert!(rt.elements().iter().any(|e| e.label.as_deref() == Some("walk: 100%")), "each project's bar, read out");
        // the project's Overview: its folders, folded under a chevron
        rt.update(|s, _| s.sel = sel(&s.projects[0]));
        settle(&mut rt);
        let t = texts(&rt);
        for said in ["Storage", "This project's folders", &format!("7.0 GB outside the store in 2 folders · walked {at}"), FOLD[0]] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "other"), "no other project's part: {t:?}");
        assert!(!t.iter().any(|x| x.starts_with("soft-swarm-try-1-fix")), "folded: {t:?}");
        keyed(&mut rt, "store-outside");
        let t = texts(&rt);
        for said in [FOLD[1], "soft-swarm-try-1-fix · try-1", "walk · walk's own folder"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        assert!(!t.iter().any(|x| x.starts_with("someone · ")), "another project's folder is on its own Overview: {t:?}");
        let danger = rune_kit::Kit::neutral(true).c.danger.fg;
        let els = rt.elements();
        let color = |words: &str| els.iter().find(|e| e.text.as_deref() == Some(words)).and_then(|e| e.text_color);
        assert_eq!(color("5.0 GB outside the store of 5.2 GB · 1 build folder, 2 binaries"), Some(danger), "over 1 GB: its size line");
        assert!(color("soft-swarm-try-1-fix · try-1").is_some_and(|c| c != danger), "its name quiet");
        keyed(&mut rt, "store-folder-/w/soft-swarm-try-1-fix");
        rt.update(|s, _| {
            let page = json!({"entries": [
                {"what": "target", "path": "/w/soft-swarm-try-1-fix/yaaihh/target", "folder": "/w/soft-swarm-try-1-fix", "owner": "try-1", "bytes": 3_328_599_654u64, "files": 12402},
                {"what": "binary", "path": "/w/soft-swarm-try-1-fix/coordd", "folder": "/w/soft-swarm-try-1-fix", "owner": "try-1", "bytes": 98_566_144u64}], "total": 5}).to_string();
            crate::store::paged(s, "outside", Ok(page));
        });
        settle(&mut rt);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "build folder yaaihh/target") && t.iter().any(|x| x == "binary coordd"), "{t:?}");
        let keys: Vec<String> = rt.elements().iter().filter_map(|e| e.key.clone()).filter(|k| k.starts_with("store-free-")).collect();
        assert_eq!(keys, vec!["store-free-/w/soft-swarm-try-1-fix/yaaihh/target".to_string()], "Delete on the build folder only, not on a binary");
        keyed(&mut rt, "store-free-/w/soft-swarm-try-1-fix/yaaihh/target");
        assert!(texts(&rt).iter().any(|x| x == "Delete 3.1 GB for good") && texts(&rt).iter().any(|x| x == "Keep"), "asked once more");
        keyed(&mut rt, "store-free-go-/w/soft-swarm-try-1-fix/yaaihh/target");
        rt.update(|s, _| crate::store::freed(s, "/w/soft-swarm-try-1-fix/yaaihh/target", Ok("/w/soft-swarm-try-1-fix/yaaihh/target: 3328599654 bytes freed".into())));
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|x| x == "Deleted yaaihh/target"), "{:?}", texts(&rt));
        rt.update(|s, _| s.projects[0].link = None);
        drop(child.stdin.take());
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let frees: Vec<&serde_json::Value> = sent.iter().filter_map(|v| v.get("req")).filter(|r| r["op"] == "storefree").collect();
        assert_eq!(frees.len(), 1, "one delete, on the second press: {sent:?}");
        assert_eq!(frees[0]["path"], "/w/soft-swarm-try-1-fix/yaaihh/target");
        // an older coordd: no Delete
        rt.update(|s, _| {
            s.projects[0].protocol = coord_types::STORE_FREE_PROTOCOL - 1;
            s.store.confirming = None;
        });
        settle(&mut rt);
        assert!(!rt.elements().iter().any(|e| e.key.as_deref().is_some_and(|k| k.starts_with("store-free-"))), "no Delete below its protocol");
    }

    /// Home's Layers now (his N-1104 p1069, p1071; a2h-speed's p2815, the names p2904, p2927): under Store, the layers
    /// open now, each by what it is and whose, on what it lies with its own size; a press opens what lies under it, a
    /// second closes it; an ended layer is not listed.
    #[test]
    fn home_lists_the_layers_open_now_and_a_press_opens_what_lies_under_one() {
        let (_held, mut s) = made();
        s.sel = sel(&s.projects[0]);
        s.projects[0].live.agents[1].jobs = vec![serde_json::from_value(json!({"id": "j7", "verb": "replay", "since": "2026-11-24T16:51:00Z", "step": "running", "step_since": "2026-11-24T16:51:05Z"})).unwrap()];
        crate::layers::came(&mut s, Ok(crate::layers::tests::answer()));
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        let t = texts(&rt);
        for said in ["Layers now · 3", "replay", "docket-core", "holders", "today", "N-77"] {
            assert!(t.iter().any(|x| x == said), "{said}: {t:?}");
        }
        assert!(!t.iter().any(|x| x == "old"), "the ended one is not listed: {t:?}");
        let under = "research's record to line 185,206 · 1.3 GB · 2 open layers lie on it";
        assert!(!t.iter().any(|x| x == under), "closed at first");
        keyed(&mut rt, "layer-j7");
        assert!(texts(&rt).iter().any(|x| x == under), "{:?}", texts(&rt));
        keyed(&mut rt, "layer-j7");
        assert!(!texts(&rt).iter().any(|x| x == under), "a second press closes it");
    }

    /// Add a repository (p2452, p2455): beside Repositories for a coordd that takes it; its request names the folder,
    /// and its last part unless he names it, refused in his words before it is asked; with no coordd reached it says so;
    /// coordd's refusal is said in the dialog, and its yes closes it. Repositories' rows are as large as a thread's
    /// (paper-research p2479).
    #[test]
    fn add_a_repository_asks_coordd_and_says_its_answer() {
        let (_held, mut s) = made();
        s.sel = Sel::View(String::new());
        s.projects[0].protocol = coord_types::ADD_REPO_PROTOCOL - 1;
        let mut rt = crate::view_cost::window(s);
        settle(&mut rt);
        assert!(keys(&rt, "home-add-repo").is_empty(), "a coordd from before it: no press");
        rt.update(|s, _| s.projects[0].protocol = coord_types::PROTOCOL);
        settle(&mut rt);
        let height = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect[3];
        assert_eq!(height(&rt, "home-no-repo"), height(&rt, "home-thread-N-1"), "as large as a thread's row");
        // a project with no agent and no thread yet: their none lines as large, their headings with no count (p2487)
        let (agents, tickets) = (rt.state.projects[0].live.agents.clone(), rt.state.projects[0].live.tickets.clone());
        rt.update(|s, _| (s.projects[0].live.agents, s.projects[0].live.tickets) = Default::default());
        settle(&mut rt);
        assert_eq!([height(&rt, "home-no-agent"), height(&rt, "home-no-thread")], [height(&rt, "home-no-repo"); 2], "every none line as large");
        assert!(keys(&rt, "home-agents-grid").is_empty() && keys(&rt, "home-repos-grid").is_empty(), "no empty grid under a none line");
        assert!(["Repositories", "Agents", "Threads"].iter().all(|h| texts(&rt).iter().any(|x| x == h)), "{:?}", texts(&rt));
        rt.update(move |s, _| (s.projects[0].live.agents, s.projects[0].live.tickets) = (agents, tickets));
        settle(&mut rt);
        keyed(&mut rt, "home-add-repo");
        assert!(rt.state.add_repo.open && rt.state.add_repo.project == 0);
        let ask = |p: &Proj, folder: &str, name: &str| add_repo_request(&AddRepo { folder: TextEdit::new(folder), name: TextEdit::new(name), ..Default::default() }, p);
        let p = &rt.state.projects[0];
        assert!(matches!(ask(p, "~/code/tovrin/", ""), Ok(Request::AddRepo { ref name, ref path }) if name == "tovrin" && path == "~/code/tovrin/"));
        assert!(matches!(ask(p, "/srv/p4", "fourth"), Ok(Request::AddRepo { ref name, .. }) if name == "fourth"));
        assert!(ask(p, "code/tovrin", "").unwrap_err().contains("full path"));
        assert!(ask(p, "~/code/my repo", "").unwrap_err().contains("letters, digits"));
        // no coordd reached: said, nothing asked
        rt.update(|s, _| s.add_repo.folder = TextEdit::new("~/code/tovrin"));
        settle(&mut rt);
        keyed(&mut rt, "add-repo-go");
        assert!(texts(&rt).iter().any(|x| x == "walk is not reached now."), "{:?}", texts(&rt));
        // coordd's refusal, in its words; then its yes closes the dialog
        let (tx, rx) = std::sync::mpsc::channel();
        rt.update(|s, _| (s.add_repo.running, s.add_repo.error) = (Some(rx), None));
        tx.send(Err("repos: tovrin = /home/u/code/tovrin is not the top of a git repository".into())).unwrap();
        assert!(rt.update_if(|s, _| add_repo_ended(s)));
        settle(&mut rt);
        assert!(rt.state.add_repo.open && texts(&rt).iter().any(|x| x.ends_with("is not the top of a git repository")));
        let (tx, rx) = std::sync::mpsc::channel();
        rt.update(|s, _| s.add_repo.running = Some(rx));
        tx.send(Ok("tovrin added".into())).unwrap();
        assert!(rt.update_if(|s, _| add_repo_ended(s)));
        assert!(!rt.state.add_repo.open);
    }

    /// A made-up project as he would see it: three repositories, a paper in one, its agents' usage, the store's
    /// answer with its folders, a link up, and a config file of its own.
    fn full() -> App {
        let dir = std::env::temp_dir().join(format!("yaaihh-view-cost-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("projects.toml"), "").unwrap();
        let (_held, mut s) = made();
        // read once at the start; the other tests keep their folder as they found it
        let _ = std::fs::remove_file(dir.join("projects.toml"));
        let p = &mut s.projects[0];
        p.protocol = coord_types::PROTOCOL.max(coord_types::CODE_VIEW_PROTOCOL);
        p.live.project = json!({"project": {"name": "walk", "repos": {"paper": "/srv/paper", "tools": "/srv/tools", "rune": "/srv/rune"}, "repo": "paper"}});
        for (i, a) in p.live.agents.iter_mut().enumerate() {
            let n = 400_000 * (i as u64 + 1);
            a.usage = vec![coord_types::ModelUsage { model: "claude-opus-5-5".into(), tokens: coord_types::Tokens { input: n, output: n / 8, cache_read: n * 6, cache_write: n / 3, reasoning: 0 }, ..Default::default() }];
        }
        let child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
        p.link = Some(crate::link::Link::to_child(child.stdin.unwrap()));
        s.repo_state.shown.insert("walk".into(), vec![
            RepoState { name: "paper".into(), path: Some("/srv/paper".into()), branch: Some("main".into()), dirty: Some(0), pdf: Some("paper.pdf".into()), ..Default::default() },
            RepoState { name: "tools".into(), path: Some("/srv/tools".into()), branch: Some("dev".into()), dirty: Some(3), ..Default::default() },
            RepoState { name: "rune".into(), refused: Some("kept out".into()), ..Default::default() },
        ]);
        let mut store: Value = serde_json::from_str(&crate::store::tests::answer()).unwrap();
        store["rows"].as_array_mut().unwrap().push(crate::store::tests::outside());
        crate::store::came(&mut s, Ok(store.to_string()));
        crate::devices::came(&mut s, Ok(crate::devices::tests::answer()));
        s.sel = sel(&s.projects[0]);
        s
    }

    /// Overview's lower half: Threads in a panel on the left; Usage, Storage and Settings stacked beside it.
    #[test]
    fn overviews_threads_sit_beside_usage_storage_and_settings() {
        let mut rt = crate::view_cost::window(full());
        settle(&mut rt);
        let at = |rt: &Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
        let threads = at(&rt, "home-threads");
        let right: Vec<[f32; 4]> = ["home-usage", "home-store", "home-settings"].iter().map(|k| at(&rt, k)).collect();
        assert!(right.iter().all(|r| r[0] > threads[0] + threads[2]), "beside Threads: {threads:?} {right:?}");
        assert!(right.windows(2).all(|w| w[0][1] + w[0][3] < w[1][1]), "stacked: {right:?}");
        assert!(!texts(&rt).iter().any(|x| x.contains("projects.toml")), "{:?}", texts(&rt));
    }

    /// A still of Overview's cards: STILL_OUT=DIR writes DIR/home-cards.png.
    #[test]
    #[ignore]
    fn still_home_cards() {
        shoot("home-cards.png", full);
    }

    /// The window's Home with its project cards: STILL_OUT=DIR writes DIR/global-cards.png.
    #[test]
    #[ignore]
    fn still_global_cards() {
        fn init() -> App {
            let mut s = full();
            s.sel = Sel::View(crate::global::ID.into());
            s
        }
        shoot("global-cards.png", init);
    }

    /// INIT's window drawn once into STILL_OUT/NAME; nothing without STILL_OUT.
    fn shoot(name: &str, init: fn() -> App) {
        let Ok(dir) = std::env::var("STILL_OUT") else { return };
        let path = format!("{dir}/{name}");
        rune::App::new(rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1400.0, 900.0), init, crate::view) }).args(["--snap", path.as_str(), "--size", "1400x900", "--at", "1.0"].map(String::from)).try_run().unwrap();
    }
}
