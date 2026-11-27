//! `coord label ID...` (the owner N-286.p1, paper-research N-101 p764 and p800): what each reference reads as beside
//! its id, from one place for the app's pages, the hover and the step texts alike. Tickets, steps and agents are
//! labelled by `docket_machine::label` (their texts read here, since a form's value may be a place in a session
//! file); a post is `BY: its first line`; a commit is its subject, read from git off the lock and kept by hash, since a
//! commit never changes.

use crate::core::Core;
use coord_types::Label;
use docket_machine::{label, Machine, Text};
use std::collections::HashMap;
use std::sync::Mutex;

/// How long a post's first line may run in its label.
const POST: usize = 80;

/// A text's words: a new one as it is, a place or a span read from its file; none when it is gone.
pub fn read(t: &Text) -> Option<String> {
    match t {
        Text::New(s) => Some(s.clone()),
        Text::Loc { loc } => crate::follow::text_at(&loc.file, loc.offset, loc.len),
        Text::Span { span } => crate::follow::text_at(&span.loc.file, span.loc.offset, span.loc.len).and_then(|s| s.get(span.at as usize..(span.at + span.len) as usize).map(str::to_string)),
        Text::Part { .. } | Text::Post { .. } => None,
        Text::Joined { joined } => joined.iter().map(read).collect(),
    }
}

/// A commit's id as asked: `REPO:SHA` with REPO among REPOS, or a bare SHA of 7 to 40 hex digits.
fn commit_of(id: &str, repos: &[(String, String)]) -> Option<(Option<String>, String)> {
    let hex = |s: &str| (7..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit());
    match id.split_once(':') {
        Some((r, sha)) if hex(sha) && repos.iter().any(|(n, _)| n == r) => Some((Some(r.to_string()), sha.to_string())),
        None if hex(id) => Some((None, id.to_string())),
        _ => None,
    }
}

/// The label of ID by machine M, given the agents the registry knows (AGENTS); a commit is left to `fill_commits`.
pub fn of(m: &Machine, agents: &[String], id: &str) -> Label {
    let (mut out, words) = of_each(m, agents, &[id]);
    words.fill(&mut out);
    out.pop().unwrap_or_default()
}

/// The labels of IDS as [`of`] gives each, made together: each ticket's label made once (a post's and a step's `on`,
/// the ticket's own), the agents' read in one pass over the tickets (`label::agents`), and each post's words left in
/// [`Words`], to be read off the lock, its label `BY` until they are (paper-research N-1446 p2432, p2532: a cloud
/// answer's 2,557 posts, 3,006 nodes, labelled under coordd's lock).
pub fn of_each(m: &Machine, agents: &[String], ids: &[&str]) -> (Vec<Label>, Words) {
    let mut tickets: HashMap<u64, String> = HashMap::new();
    let mut ticket = |t: u64| tickets.entry(t).or_insert_with(|| label::ticket(m, t, &read)).clone();
    let mut words = Words(Vec::new());
    let mut agent_at: Vec<usize> = Vec::new();
    let mut out: Vec<Label> = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        let mut l = Label { id: id.to_string(), ..Default::default() };
        if let Some(t) = m.ticket_named(id) {
            (l.is, l.label) = ("ticket".into(), ticket(t));
        } else if let Some((t, n)) = docket_machine::step_id(id).filter(|(_, _, itself)| *itself).and_then(|(_, n, _)| Some((m.ticket_named(id.split_once('.')?.0)?, n))) {
            if let Some(s) = label::step(m, t, n) {
                (l.is, l.label, l.on) = ("step".into(), s, Some(ticket(t)));
            }
        } else if let Some((t, n)) = docket_machine::post_ref(id).and_then(|(t, n)| Some((m.ticket_named(t)?, n))) {
            if let Some(p) = m.tickets[&t].posts.iter().find(|p| p.n == n) {
                (l.is, l.label, l.on) = ("post".into(), p.by.clone(), Some(ticket(t)));
                words.0.push((i, m.text(&p.text).unwrap_or_else(|| p.text.clone())));
            }
        } else if agents.iter().any(|a| a == id) || !m.roles_of(id).is_empty() || m.made_in(id).is_some() {
            l.is = "agent".into();
            agent_at.push(i);
        }
        out.push(l);
    }
    let named: Vec<&str> = agent_at.iter().map(|i| ids[*i]).collect();
    for (i, a) in agent_at.iter().zip(label::agents(m, &named, &read)) {
        out[*i].label = a;
    }
    (out, words)
}

/// The posts' words a label is still to be given, by its place in the answer: read off coordd's lock, as a transcript
/// page is (they are in session files), and put in by [`Words::fill`].
#[derive(Debug, Default)]
pub struct Words(Vec<(usize, Text)>);

impl Words {
    /// Each post's label `BY: its first line`, its words read; `BY` alone where it has none.
    pub fn fill(self, out: &mut [Label]) {
        for (i, t) in self.0 {
            let words = read(&t).map(|w| label::first_line(&w, POST)).unwrap_or_default();
            if let Some(l) = out.get_mut(i).filter(|_| !words.is_empty()) {
                l.label = format!("{}: {words}", l.label);
            }
        }
    }
}

impl Core {
    /// The labels of IDS that the tickets and the registry give, the commits among them (by their place in the answer,
    /// their repository when named, their hash) and the project's repositories: what `subjects` reads off the lock.
    /// The posts' words read here; [`Core::labels_held`] leaves them to be read off the lock, as coordd does.
    #[cfg(test)]
    pub fn labels(&self, ids: &[String]) -> (Vec<Label>, Vec<(usize, Option<String>, String)>, Vec<(String, String)>) {
        let (mut out, words, commits, repos) = self.labels_held(ids);
        words.fill(&mut out);
        (out, commits, repos)
    }

    /// [`Core::labels`] with the posts' words not yet read: what is taken under the lock (`Request::Label`).
    pub fn labels_held(&self, ids: &[String]) -> (Vec<Label>, Words, Vec<(usize, Option<String>, String)>, Vec<(String, String)>) {
        let repos = self.project.as_ref().map(|p| p.repos()).unwrap_or_default();
        let agents: Vec<String> = self.agents.keys().cloned().collect();
        let asked: Vec<&str> = ids.iter().map(String::as_str).collect();
        let (out, words) = of_each(&self.tickets.m, &agents, &asked);
        let commits = out.iter().enumerate().filter(|(_, l)| l.is.is_empty()).filter_map(|(i, l)| commit_of(&l.id, &repos).map(|(r, sha)| (i, r, sha))).collect();
        (out, words, commits, repos)
    }
}

impl Core {
    /// The part of the record's graph ASK names (`Request::Graph`; proto-window N-1446 p1383, head 1), each of its nodes
    /// labelled as [`Core::labels`] labels its id (an agent by its name), a commit by the kept graph's read of it, its
    /// subject and its repository (p1476: nothing here asks git); with the folders where agents work that are gone
    /// (p1455); what is worked on now when the ask asks it (`Ask::now`, head 2), its posts labelled after the slice's
    /// nodes; and around his post its course (p1429, 1). A node no label is known for has its `is` empty. An ask that
    /// names an agent's folder is given that alone ([`docket_machine::graph::Graph::folder`]; the file view,
    /// paper-research N-1446 p1441, p1530), and with a session that session's requests alone
    /// (`Graph::session_requests`, p1642, 3); one that names an agent's system prompt its rows alone the same way, and
    /// with a version that version's requests (`Graph::prompt`; fix 4, docket-edge6 p2496, read right p2504).
    /// Made whole here; coordd reads the posts' words off the lock ([`Core::graph_slice_held`]).
    #[cfg(test)]
    pub fn graph_slice(&self, ask: &docket_machine::graph_slice::Ask) -> coord_types::GraphAnswer {
        let (mut answer, words) = self.graph_slice_held(ask);
        words.fill(&mut answer.labels);
        answer
    }

    /// [`Core::graph_slice`] with its posts' words not yet read: what is taken under the lock (`Request::Graph`), the
    /// words read and the answer written off it (paper-research N-1446 p2432).
    pub fn graph_slice_held(&self, ask: &docket_machine::graph_slice::Ask) -> (coord_types::GraphAnswer, Words) {
        if let Some(agent) = &ask.prompt {
            let rows = self.tickets.graph.graph.prompt(agent, ask.version.as_deref());
            return (coord_types::GraphAnswer { folder: Some(rows), ..Default::default() }, Words::default());
        }
        if let Some(agent) = &ask.folder {
            let g = &self.tickets.graph.graph;
            let rows = match &ask.session {
                Some(s) => g.session_requests(agent, s),
                None => g.folder(agent),
            };
            return (coord_types::GraphAnswer { folder: Some(rows), ..Default::default() }, Words::default());
        }
        let slice = self.tickets.graph.graph.slice(ask);
        // what is worked on now, at coordd's clock (`Ask::now`), its asks' posts and each agent's last post labelled
        // after the slice's nodes (paper-research N-1446 p1469, 6: each agent's "last post")
        let now = ask.now.then(|| self.tickets.graph.graph.now(&crate::core::at_secs(chrono::Utc::now().timestamp())));
        let mut ids: Vec<String> = slice.nodes.iter().map(|n| &n.node.id).map(|id| id.strip_prefix("agent:").or_else(|| id.strip_prefix("commit:")).unwrap_or(id).to_string()).collect();
        for post in now.iter().flat_map(|n| n.asks.iter().map(|a| &a.times.post).chain(n.agents.iter().filter_map(|a| a.last_post.as_ref().map(|p| &p.id)))) {
            if !slice.nodes.iter().any(|n| n.node.id == *post) && !ids.contains(post) {
                ids.push(post.clone());
            }
        }
        let (mut labels, words, _, _) = self.labels_held(&ids);
        for (l, n) in labels.iter_mut().zip(&slice.nodes) {
            l.id = n.node.id.clone();
            if let Some(c) = self.tickets.graph_git.commit(&n.node.id) {
                (l.is, l.label, l.on) = ("commit".into(), c.subject.clone(), Some(c.repo.clone()));
            }
        }
        // around his post, its course (paper-research N-1446 p1429, 1): `now` holds it already while it is not in his
        // window yet, else it is read from the graph
        let g = &self.tickets.graph.graph;
        let course = ask.around.as_ref().filter(|a| g.get(a).is_some_and(|n| n.kind == docket_machine::graph::NodeKind::Post)).and_then(|a| {
            now.iter().flat_map(|n| &n.asks).find(|x| x.times.post == *a).map(|x| x.times.clone()).or_else(|| g.ask_times().into_iter().find(|t| t.post == *a))
        });
        (coord_types::GraphAnswer { slice, labels, now, course, gone: self.tickets.graph_git.gone(), folder: None }, words)
    }
}

/// The subjects already read, by repository folder and full hash: a commit never changes.
static SUBJECTS: Mutex<Option<HashMap<(String, String), String>>> = Mutex::new(None);

/// Each commit's subject, read from its repository (the one named, else the first of REPOS that has it), by its
/// place in the answer: its repository's name, its full hash and its subject.
pub fn subjects(commits: &[(usize, Option<String>, String)], repos: &[(String, String)]) -> Vec<(usize, String, String)> {
    let mut out = Vec::new();
    for (i, r, sha) in commits {
        for (name, dir) in repos.iter().filter(|(n, _)| r.as_ref().is_none_or(|r| r == n)) {
            let git = || crate::ticket::work_repos::git(std::path::Path::new(dir));
            let full = git().args(["rev-parse", "--verify", "--quiet", &format!("{sha}^{{commit}}")]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            let Some(full) = full else { continue };
            let key = (dir.clone(), full.clone());
            let kept = SUBJECTS.lock().ok().and_then(|s| s.as_ref().and_then(|s| s.get(&key).cloned()));
            let subject = match kept {
                Some(s) => s,
                None => {
                    let Some(s) = git().args(["log", "-1", "--format=%s", &full]).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()) else { continue };
                    if let Ok(mut m) = SUBJECTS.lock() {
                        m.get_or_insert_with(HashMap::new).insert(key, s.clone());
                    }
                    s
                }
            };
            out.push((*i, name.clone(), subject));
            break;
        }
    }
    out
}

/// The commits' labels put in OUT: the subject, the repository it was found in as what it is on.
pub fn fill_commits(out: &mut [Label], subjects: &[(usize, String, String)]) {
    for (i, repo, subject) in subjects {
        if let Some(l) = out.get_mut(*i) {
            (l.is, l.label, l.on) = ("commit".into(), subject.clone(), Some(repo.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commit_is_a_named_repository_and_hex() {
        let repos = vec![("a".to_string(), "/x/a".to_string())];
        assert_eq!(commit_of("a:aac8c60", &repos), Some((Some("a".into()), "aac8c60".into())));
        assert_eq!(commit_of("aac8c60", &repos), Some((None, "aac8c60".into())));
        assert_eq!(commit_of("c:aac8c60", &repos), None);
        assert_eq!(commit_of("orch5", &repos), None);
        assert_eq!(commit_of("abc", &repos), None);
    }
}
