//! Texts held by reference (owner m613): a post, a formed answer's text or a delivery is a place in a transcript,
//! read through coordd (`Request::Text`) when a page shows it, and kept for the window's life; and a result in words.

use crate::{App, Pending, Proj};
use coord_types::{Request, TicketView};
use docket_machine::{Loc, ViewStatus};
use serde_json::{Value, json};
use std::collections::HashSet;
use crate::threads::{listed, Page};

pub(crate) fn key(l: &Loc) -> String {
    format!("{}:{}:{}", l.file, l.offset, l.len)
}

pub(crate) fn loc_of(v: &Value) -> Option<Loc> {
    // a span is read at its place and cut: the place is what is fetched
    serde_json::from_value(v.get("loc").or_else(|| v.get("span")?.get("loc"))?.clone()).ok()
}

/// What a text value holds for a pane to draw (the owner, N-1790 part 3): its words once read; `Loading` while its
/// place is being read; `Empty` when it holds none. A chat draws `Loading` as a quiet line of its own, never "…" in a
/// bubble.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TextState {
    Ready(String),
    Loading,
    Empty,
}

impl TextState {
    /// Its words, "…" while loading, none when empty: for a line that says a text inline (a card's row, a title).
    pub(crate) fn words(self) -> String {
        match self {
            TextState::Ready(t) => t,
            TextState::Loading => "…".into(),
            TextState::Empty => String::new(),
        }
    }

    /// Its words for a chat's row: [`LOADING`] while loading, none when empty.
    pub(crate) fn shown(self) -> String {
        match self {
            TextState::Loading => LOADING.into(),
            t => t.words(),
        }
    }

    fn of(t: String) -> TextState {
        if t.trim().is_empty() { TextState::Empty } else { TextState::Ready(t) }
    }
}

/// What a chat says, quietly and in no bubble, where words still being read will stand.
pub(crate) const LOADING: &str = "reading its words…";

/// A text value V (a new text, or `{"loc": ...}`) as words: a reference as the page has read it, "…" while it is not
/// ([`state`] tells the two apart); one never asked noted in `Page::unread`, for [`poll`] to ask for.
pub(crate) fn words(pg: &Page, v: &Value) -> String {
    state(pg, v).words()
}

/// A text value V as [`TextState`]: a reference `Loading` while asked, or never asked (noted in `Page::unread`, for
/// [`poll`] to ask for); pieces read one after the other `Loading` while any is.
pub(crate) fn state(pg: &Page, v: &Value) -> TextState {
    // pieces read one after the other (`text of` over a value held by reference)
    if let Some(ps) = v.get("joined").and_then(Value::as_array) {
        let mut out = String::new();
        for p in ps {
            match state(pg, p) {
                TextState::Loading => return TextState::Loading,
                TextState::Ready(t) => out.push_str(&t),
                TextState::Empty => {}
            }
        }
        return TextState::of(out);
    }
    match v {
        Value::String(s) => TextState::of(s.clone()),
        Value::Null => TextState::Empty,
        _ => match loc_of(v) {
            Some(l) => match pg.texts.get(&key(&l)) {
                // a span: bytes AT to AT+LEN of what the place says (docket_machine::Text::Span)
                Some(Some(Ok(t))) if v.get("span").is_some() => {
                    let sp = &v["span"];
                    let at = sp.get("at").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let len = sp.get("len").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let mut a = at.min(t.len());
                    while !t.is_char_boundary(a) {
                        a -= 1;
                    }
                    let mut b = (at + len).min(t.len());
                    while !t.is_char_boundary(b) {
                        b += 1;
                    }
                    TextState::of(t[a..b].to_string())
                }
                Some(Some(Ok(t))) => TextState::of(t.clone()),
                Some(Some(Err(e))) => TextState::Ready(format!("(not read: {e})")),
                Some(None) => TextState::Loading,
                // never asked: noted, so whatever pane draws it asks for it ([`poll`]; paper-research N-1446 p1556)
                None => {
                    pg.unread.borrow_mut().push((None, l));
                    TextState::Loading
                }
            },
            None => TextState::of(outcome(pg, v)),
        },
    }
}

/// A result value in words: a variant as `Head: its fields`.
pub(crate) fn outcome(pg: &Page, v: &Value) -> String {
    match v {
        Value::Object(o) if o.get("head").is_some_and(Value::is_string) => {
            let head = o["head"].as_str().unwrap_or_default();
            let fields: Vec<String> = o.iter().filter(|(k, x)| *k != "head" && !x.is_null()).map(|(_, x)| first_line_cut(&words(pg, x))).collect();
            if fields.is_empty() { head.to_string() } else { format!("{head}: {}", fields.join("; ")) }
        }
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}

/// Result V as a badge says it: its constructor and its short fields (a commit, a name), not its texts, which the page
/// shows whole below (a text field is the answerer's whole output, its first word the constructor again: C-24's badge
/// read "Dismissed: Dismiss: `mean` is documented...").
pub(crate) fn badge_outcome(v: &Value) -> String {
    match v {
        Value::Object(o) if o.get("head").is_some_and(Value::is_string) => {
            let short = |x: &Value| x.as_str().filter(|s| !s.is_empty() && s.len() <= 64 && !s.contains(char::is_whitespace)).map(str::to_string);
            let fields: Vec<String> = o.iter().filter(|(k, _)| *k != "head").filter_map(|(_, x)| short(x)).collect();
            let head = o["head"].as_str().unwrap_or_default();
            if fields.is_empty() { head.to_string() } else { format!("{head}: {}", fields.join("; ")) }
        }
        Value::String(s) => first_line_cut(s),
        _ => v.to_string(),
    }
}

/// The lines that met answer HEAD as words, `by L1; L2`: a goal's Done by its duty check, each line by its time and
/// what it is (paper-research N-1446 p2255); none when it names none. The lens's answer and the page's answer row.
pub(crate) fn met_by(head: &Value) -> Option<String> {
    let by: Vec<&str> = head["by"].as_array()?.iter().filter_map(Value::as_str).collect();
    (!by.is_empty()).then(|| format!("by {}", by.join("; ")))
}

/// S with each commit hash (40 hex digits) cut to its first 9, as git shows them.
pub(crate) fn short_hashes(s: &str) -> String {
    let mut out = String::new();
    for w in s.split_inclusive(|c: char| !c.is_ascii_hexdigit()) {
        let hex = w.trim_end_matches(|c: char| !c.is_ascii_hexdigit());
        if hex.len() == 40 {
            out.push_str(&hex[..9]);
            out.push_str(&w[40..]);
        } else {
            out.push_str(w);
        }
    }
    out
}

/// The texts of the question tickets the page has not read: their asks, contexts and results.
pub(crate) fn questions_wanted(pg: &Page, p: &Proj) -> Vec<Loc> {
    let mut out = Vec::new();
    for v in p.live.tickets.values().filter(|v| v.kind == "question") {
        let result = match &v.status {
            ViewStatus::Done { result } => result.get("text"),
            _ => None,
        };
        out.extend([v.form.get("ask"), v.form.get("context"), result].into_iter().flatten().filter_map(loc_of));
        // the owner's step on a question thought through: its answer, reasoning and passages
        if let ViewStatus::Awaits { args, .. } = &v.status {
            locs_in(args, &mut out);
        }
    }
    out.retain(|l| !pg.texts.contains_key(&key(l)));
    out
}

/// Every text held by reference anywhere in X.
pub(crate) fn locs_in(x: &Value, out: &mut Vec<Loc>) {
    if let Some(l) = loc_of(x) {
        out.push(l);
        return;
    }
    match x {
        Value::Array(a) => a.iter().for_each(|y| locs_in(y, out)),
        Value::Object(o) => o.values().for_each(|y| locs_in(y, out)),
        _ => {}
    }
}

/// The texts ticket V shows that the page has not read: its form's, its posts', its unfolded deliveries'.
pub(crate) fn wanted(pg: &Page, v: &TicketView) -> Vec<Loc> {
    let mut out: Vec<Loc> = Vec::new();
    let mut add = |x: &Value| {
        if let Some(l) = loc_of(x) {
            out.push(l);
        }
    };
    v.form.as_object().into_iter().flat_map(|o| o.values()).for_each(&mut add);
    for p in &v.posts {
        add(&serde_json::to_value(&p.text).unwrap_or(Value::Null));
    }
    if let ViewStatus::Done { result } = &v.status {
        result.as_object().into_iter().flat_map(|o| o.values()).for_each(&mut add);
    }
    if let ViewStatus::Failed { raised } = &v.status {
        add(&raised.value);
    }
    for g in v.given.iter().filter(|g| pg.unfolded.contains(&g.delivery)) {
        if let Some(l) = &g.place {
            out.push(l.clone());
        }
    }
    out.retain(|l| !pg.texts.contains_key(&key(l)));
    out
}

/// The texts ticket V shows, by key, for a scene that reads them once, and the rows' last posts (`rows_wanted`).
pub fn scene_texts(pg: &Page, v: Option<&TicketView>, p: Option<&Proj>) -> Vec<(String, Loc)> {
    let mut ls: Vec<Loc> = v.map(|v| wanted(pg, v)).unwrap_or_default();
    // the words of an answer about to be taken (slow mode's halts)
    if let (Some(v), Some(p)) = (v, p) {
        ls.extend(crate::slow::wanted(p, v).iter().filter_map(loc_of).filter(|l| !pg.texts.contains_key(&key(l))));
    }
    if let (Some(v), Some(p)) = (v, p) {
        let cx = crate::ran::Ctx::of(p, v);
        ls.extend(crate::ran::wanted(pg, &cx, &crate::ran::lines(&cx, v)));
    }
    ls.extend(p.map(|p| rows_wanted(pg, p)).unwrap_or_default());
    // the agents' questions and their answers, as the change and the list show them
    ls.extend(p.map(|p| questions_wanted(pg, p)).unwrap_or_default());
    let mut seen = HashSet::new();
    ls.into_iter().filter(|l| seen.insert(key(l))).map(|l| (key(&l), l)).collect()
}

fn rows_wanted(pg: &Page, p: &Proj) -> Vec<Loc> {
    let rows = || p.live.tickets.values().filter(|v| v.parent.is_none() && listed(v));
    let last = rows()
        .filter_map(|v| v.posts.iter().rev().find(|x| x.visible() && x.moved.is_none()))
        .filter_map(|x| loc_of(&serde_json::to_value(&x.text).unwrap_or(Value::Null)));
    // an untitled ticket's words in a record of its form (an ask's question), which its row shows
    let asked = rows().filter(|v| v.title.is_empty()).filter_map(|v| loc_of(crate::ask::asked_words(v)?));
    last.chain(asked).filter(|l| !pg.texts.contains_key(&key(l))).collect()
}

/// EACH, a text and the project whose pipe asks for it, asked when it is not read nor asked yet: the words of the
/// posts the Map's cards draw (map/canvas.rs `worded`), of every project's island, not the current one's alone
/// (consult-3 N-1479 p180, proto-lead p181); and what any pane drew as "…" never asked ([`words`], [`drawn_in`]), a
/// pane of the split being of its own project (paper-research N-1446 p1556). coordd reads a text only where its own
/// record gives it (main.rs `Request::Text`), so each goes through its own project's pipe. A text's key is its place's
/// file as coordd gives it, a path in that coord's own folder (coordd main.rs `pin`) or a transcript's, so one map
/// holds every project's texts apart.
fn ask_each(s: &mut App, each: Vec<(usize, Loc)>) -> bool {
    let mut any = false;
    for (pi, l) in each {
        let k = key(&l);
        if s.tickets.texts.contains_key(&k) {
            continue;
        }
        let Some(link) = s.projects.get_mut(pi).and_then(|p| p.link.as_mut()) else { continue };
        if let Some(id) = link.ask(&Request::Text { loc: l }) {
            s.projects[pi].pending.insert(id, Pending::TicketText { key: k.clone() });
            s.tickets.texts.insert(k, None);
            any = true;
        }
    }
    any
}

/// F, drawn for project PI where the focused page may be another's: the split's pane that is not the focused one
/// (panes.rs `area`; its rows built later, ask.rs `in_pane`), and an island of the Map (map/canvas.rs a post's card,
/// map/deep.rs `step`). What it draws as "…" never asked ([`words`]) is asked through PI's pipe, not the focused
/// page's (paper-research N-1446 p1556). Within another, the inner one's project stands: it is what is drawn.
pub(crate) fn drawn_in<T>(s: &App, pi: usize, f: impl FnOnce() -> T) -> T {
    let from = s.tickets.unread.borrow().len();
    let out = f();
    for (of, _) in s.tickets.unread.borrow_mut().iter_mut().skip(from) {
        of.get_or_insert(pi);
    }
    out
}

/// The texts thread V draws whole that page PG has not read: a halt's words (an answer about to be taken, read as a
/// post's are), its lines (its prompts as sent, and the posts and heads of the tickets under it), and its own
/// ([`wanted`]).
fn thread_wanted(pg: &Page, p: &Proj, v: &TicketView) -> Vec<Loc> {
    let mut want: Vec<Loc> = crate::slow::wanted(p, v).iter().filter_map(loc_of).collect();
    let cx = crate::ran::Ctx::of(p, v);
    want.extend(crate::ran::wanted(pg, &cx, &crate::ran::lines(&cx, v)));
    want.extend(wanted(pg, v));
    want
}

/// The agent whose With you the page draws (withyou.rs `view`): its page showing its chat with him (chat.rs), or its
/// With you lens (lens.rs `of_agent`).
fn with_you_of(s: &App) -> Option<&str> {
    match &s.sel {
        crate::Sel::Agent(name) => {
            let key = (s.cur, name.clone());
            (s.streams.which.get(&key).copied().unwrap_or_default() == crate::streams::Which::You && !s.term.pages.contains(&key)).then_some(name.as_str())
        }
        crate::Sel::Lens(l) => l.with_you(),
        _ => None,
    }
}

/// The texts of the current project the page draws and has not read: on Threads (and Kinds) the thread shown and the
/// list's rows; on an agent's page the stream it shows; With you's thread, drawn whole beside its list with every
/// agent's posts in it, and its list's rows (build 66 read only the agent's own posts there, so agy-study's turn-end
/// reply N-1104 p806 stood as "…" in his With you of another agent: paper-research N-1446 p1514, proto-window p1549);
/// on any page the question tickets' words, which Decisions and an item's page show (each read once).
fn page_wanted(s: &App) -> Vec<Loc> {
    let Some(p) = s.projects.get(s.cur) else { return Vec::new() };
    let pg = &s.tickets;
    let mut want: Vec<Loc> = Vec::new();
    if s.sel.shows_threads() || s.sel == crate::Sel::Kinds {
        let shown = pg.shown.and_then(|n| p.live.tickets.get(&n).or_else(|| pg.find.fetched.get(&p.id).and_then(|m| m.get(&n))));
        want.extend(shown.map(|v| thread_wanted(pg, p, v)).unwrap_or_default());
        want.extend(rows_wanted(pg, p));
    }
    // an agent's chat: the words of the stream it shows (streams.rs)
    if let crate::Sel::Agent(name) = &s.sel {
        let which = s.streams.which.get(&(s.cur, name.clone())).copied().unwrap_or_default();
        if which != crate::streams::Which::Work {
            want.extend(crate::streams::wanted(pg, p, name, which));
        }
    }
    if let Some(name) = with_you_of(s) {
        want.extend(crate::withyou::beside(s, p, s.cur, name).0.map(|v| thread_wanted(pg, p, v)).unwrap_or_default());
        want.extend(rows_wanted(pg, p));
    }
    want.extend(questions_wanted(pg, p));
    want
}

/// Each update while the page shows: the kinds once, and the texts the page draws ([`page_wanted`], asked as it opens);
/// while the Map shows, the texts its cards draw; and whatever any pane drew as "…" never asked, through the pipe of
/// that pane's project ([`ask_each`]): what any pane draws is what is asked for (paper-research N-1446 p1556).
pub fn poll(s: &mut App) -> bool {
    let mut each = crate::map::canvas::worded(s);
    each.extend(s.tickets.unread.take().into_iter().map(|(pi, l)| (pi.unwrap_or(s.cur), l)));
    let cards = ask_each(s, each);
    let page = s.sel.shows_threads() || s.sel == crate::Sel::Kinds;
    let want = page_wanted(s);
    // these asks are the page's own, not the owner's: while the box is not connected (starting, or a scene) they wait,
    // quietly, rather than putting "not sent" at the top of the page, and are asked again once it is
    let Some(p) = s.projects.get_mut(s.cur) else { return cards };
    if !page && want.is_empty() {
        return cards;
    }
    // another project's kinds held (he changed project): let go, so the page says it reads them rather than drawing
    // the other's, and asked of the project shown (paper-research N-1446 p1859); a late answer of the other's is not
    // taken (lib.rs `take`)
    if page && s.tickets.kinds_of.as_ref().is_some_and(|o| *o != p.id) {
        s.tickets.kinds = None;
        s.tickets.kinds_asked = false;
        s.tickets.kinds_of = None;
    }
    let Some(link) = p.link.as_mut() else { return cards };
    let mut any = cards;
    if page && !s.tickets.kinds_asked {
        if let Some(id) = link.ask(&Request::Kinds) {
            p.pending.insert(id, Pending::TicketKinds);
            s.tickets.kinds_asked = true;
            s.tickets.kinds_of = Some(p.id.clone());
            any = true;
        }
    }
    {
        for l in want {
            if s.tickets.texts.contains_key(&key(&l)) {
                continue;
            }
            if let Some(id) = link.ask(&Request::Text { loc: l.clone() }) {
                p.pending.insert(id, Pending::TicketText { key: key(&l) });
                s.tickets.texts.insert(key(&l), None);
                any = true;
            }
        }
    }
    any
}

/// S's first line, with " …" when more lines follow: a badge holds a result's start, the page shows it whole.
pub(crate) fn first_line_cut(s: &str) -> String {
    let l = s.lines().next().unwrap_or("");
    if s.lines().nth(1).is_some() { format!("{l} …") } else { l.to_string() }
}

/// A line of Markdown as words: no list mark, no emphasis or code marks.
pub(crate) fn plain(l: &str) -> String {
    let mut t = l.trim();
    loop {
        let before = t;
        for m in ["- ", "* ", "+ ", "● ", "○ ", "> ", "#"] {
            t = t.strip_prefix(m).unwrap_or(t).trim_start();
        }
        if let Some((n, rest)) = t.split_once(". ").filter(|(n, _)| !n.is_empty() && n.len() <= 3 && n.chars().all(|c| c.is_ascii_digit())) {
            let _ = n;
            t = rest.trim_start();
        }
        if t == before {
            break;
        }
    }
    t.replace("**", "").replace("__", "").replace('`', "")
}

#[cfg(test)]
mod moved_tests {
    use super::*;
    #[test]
    fn a_badge_says_the_result_without_its_texts() {
        assert_eq!(badge_outcome(&json!({"head": "Fixed", "commit": "c76ba21c488b7b3a8fcee160d10bdeb67ade01a2"})), "Fixed: c76ba21c488b7b3a8fcee160d10bdeb67ade01a2");
        assert_eq!(badge_outcome(&json!({"head": "Dismissed", "why": {"loc": {"file": "/t.jsonl", "offset": 1, "len": 2}}})), "Dismissed");
        assert_eq!(badge_outcome(&json!({"head": "No", "why": "Not until a caller needs it"})), "No");
        assert_eq!(badge_outcome(&json!({"head": "Green"})), "Green");
    }

    #[test]
    fn a_goals_done_says_the_lines_that_met_it_and_a_hands_says_none() {
        // the page's answer row: the badge, then a check's lines; a hand's words are its post's body, as before
        let by = ["2026-11-23T17:08:47Z job end 41-17", "2026-11-23T17:56:46Z window_release"];
        let check = json!({"head": "Done", "why": null, "by": by});
        assert_eq!(badge_outcome(&check), "Done");
        assert_eq!(met_by(&check).as_deref(), Some("by 2026-11-23T17:08:47Z job end 41-17; 2026-11-23T17:56:46Z window_release"));
        let hand = json!({"head": "Done", "why": "landed in 71", "by": null});
        assert_eq!((badge_outcome(&hand), met_by(&hand)), ("Done".to_string(), None));
        assert_eq!(met_by(&json!({"head": "Done", "by": []})), None);
        assert_eq!(met_by(&json!({"head": "Dropped", "why": "done elsewhere"})), None);
    }

    #[test]
    fn hashes_are_cut() {
        assert_eq!(short_hashes("Fixed: 6217ce8247e10fd6d81defe2718c46424c900b51."), "Fixed: 6217ce824.");
        assert_eq!(short_hashes("abc 12"), "abc 12");
    }

    /// His chat N-1104 as it stood at 13:49 on 22 Nov: his p805 to agy-study, agy-study's turn-end reply p806 (its words
    /// a place in agy-study's session, read through coordd), accounts-lead's p807 to him. And p806's place.
    fn chat_with_a_reply_held() -> (std::sync::MutexGuard<'static, ()>, App, Loc) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        let agent = |name: &str| -> coord_types::AgentView {
            serde_json::from_value(json!({"id": format!("AGT-{name}"), "name": name, "parent": "owner", "kind": "claude", "model": "m", "state": "idle",
                "since": "2026-11-22T13:49:49Z", "tools": [], "items": []}))
            .unwrap()
        };
        p.live.agents = vec![agent("accounts-lead"), agent("agy-study")];
        let loc = Loc { file: "/s/agy-study.jsonl".into(), offset: 1313554, len: 3155 };
        let v: TicketView = serde_json::from_value(json!({"id": "N-1104", "n": 1104, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "paper-research"},
            "title": "the owner's chat", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": "2026-11-19T08:00:00Z",
            "given": [], "posts": [
                {"n": 805, "at": "2026-11-22T13:46:22Z", "by": "owner", "text": "okay well then nevermind about agy", "names": [], "told": ["agy-study"]},
                {"n": 806, "at": "2026-11-22T13:47:02Z", "by": "agy-study", "text": {"loc": loc.clone()}, "names": ["owner"], "told": [], "auto": true},
                {"n": 807, "at": "2026-11-22T13:49:49Z", "by": "accounts-lead", "text": "May yaaihh keep your Go key?", "names": ["owner"], "told": []}]}))
        .unwrap();
        p.live.tickets.insert(1104, v);
        s.projects = vec![p];
        s.cur = 0;
        (held, s, loc)
    }

    /// paper-research N-1446 p1556: what any pane draws is what the poll asks for. A split: the focused page another
    /// project's Threads with no thread open, the pane beside it research's lens kept at N-1104 (drawn whole as With you
    /// draws a thread: lens.rs `apart`), which no page list covers. p806's words, drawn "…" in that pane, are asked once,
    /// through research's pipe (the pane's project; coordd reads a text only where its own record gives it), nothing
    /// through the focused page's; read, its words are drawn there.
    #[test]
    #[cfg(not(windows))]
    fn a_pane_beside_the_focused_one_asks_for_the_words_it_draws_through_its_own_projects_pipe() {
        let (_held, mut s, loc) = chat_with_a_reply_held();
        let mut other = Proj::new(&crate::projects::ProjectConfig { name: "other".into(), id: "other".into(), boxed: false }, Vec::new());
        other.down = None;
        s.projects.push(other);
        let mut cats = Vec::new();
        for p in &mut s.projects {
            let mut cat = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).spawn().unwrap();
            p.link = Some(crate::link::Link::to_child(cat.stdin.take().unwrap()));
            cats.push(cat);
        }
        let kept = crate::lens::Lens { filter: crate::lens::Filter::Threads, depth: crate::lens::Depth::Structure, at: Some("N-1104".into()) };
        (s.cur, s.sel, s.focus) = (1, crate::Sel::Tickets, 0);
        s.panes = vec![(1, crate::Sel::Tickets), (0, crate::Sel::Lens(kept))];
        assert!(!page_wanted(&s).contains(&loc), "no page list holds the pane beside's thread");
        let said = |rt: &rune::runtime::Runtime<App>| -> Vec<String> {
            rt.elements().iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == "said-N-1104-806")).filter_map(|e| e.text.clone()).collect()
        };
        let asked = |rt: &rune::runtime::Runtime<App>, pi: usize| -> Vec<String> {
            rt.state.projects[pi].pending.values().filter_map(|w| if let Pending::TicketText { key } = w { Some(key.clone()) } else { None }).collect()
        };
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        assert_eq!(said(&rt), [LOADING], "drawn in the pane beside, not read: a quiet line, no bubble of \"…\"");
        assert!(poll(&mut rt.state), "the poll asks");
        assert_eq!(asked(&rt, 0), vec![key(&loc)], "through research's pipe, the pane's project");
        assert!(asked(&rt, 1).is_empty(), "nothing through the focused page's: {:?}", asked(&rt, 1));
        rt.invalidate();
        rt.frame(0.016);
        poll(&mut rt.state);
        assert_eq!(asked(&rt, 0).len(), 1, "asked once while it is out");
        let id = *rt.state.projects[0].pending.iter().find(|(_, w)| matches!(w, Pending::TicketText { key: k } if *k == key(&loc))).unwrap().0;
        rt.state.projects[0].pending.remove(&id);
        crate::threads::replied(&mut rt.state, Pending::TicketText { key: key(&loc) }, true, "agy is parked.".into());
        rt.invalidate();
        rt.frame(0.016);
        assert_eq!(said(&rt), ["agy is parked."], "read: its words in the pane beside");
        for p in &mut rt.state.projects {
            p.link = None;
        }
        for mut c in cats {
            let _ = c.wait();
        }
    }

    /// paper-research N-1446 p1514 (proto-window p1549): With you draws its thread whole, every agent's posts in it, so
    /// it reads every post's words held by reference there, as Threads does. Build 66 read only the agent's own posts:
    /// agy-study's turn-end reply p806 stood as "…" in his With you of another agent, and he asked why it answered him
    /// "...". Read, its words are drawn where "…" was.
    #[test]
    fn with_you_reads_the_words_of_every_post_in_the_thread_it_draws() {
        let (_held, mut s, loc) = chat_with_a_reply_held();
        let wants = |s: &App| page_wanted(s).contains(&loc);
        s.sel = crate::Sel::Agent("accounts-lead".into());
        s.withyou.inside.insert((0, "accounts-lead".into()), 1104);
        assert!(wants(&s), "With you of accounts-lead draws agy-study's p806 on N-1104: its words are asked for");
        s.sel = crate::Sel::Lens(crate::lens::Lens { filter: crate::lens::Filter::WithYou { agent: "accounts-lead".into() }, depth: crate::lens::Depth::Structure, at: Some("N-1104".into()) });
        assert!(wants(&s), "and in accounts-lead's With you lens");
        s.sel = crate::Sel::Agent("accounts-lead".into());
        s.streams.which.insert((0, "accounts-lead".into()), crate::streams::Which::Agents);
        assert!(!wants(&s), "With agents draws accounts-lead's posts with the others, and N-1104 not: nothing of it is asked");
        s.streams.which.clear();
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = Some(1104);
        assert!(wants(&s), "Threads open at N-1104, as before");
        s.tickets.shown = None;
        // drawn in With you: "…" while its words are not read, its words once they are
        s.sel = crate::Sel::Agent("accounts-lead".into());
        let said = |rt: &rune::runtime::Runtime<App>| -> Vec<String> {
            rt.elements().iter().filter(|e| !e.exiting && e.path.split('/').any(|k| k == "said-N-1104-806")).filter_map(|e| e.text.clone()).collect()
        };
        let mut rt = crate::view_cost::window(s);
        rt.frame(0.016);
        rt.frame(0.016);
        assert_eq!(said(&rt), [LOADING], "not read: a quiet line where build 66 left the one character \"…\"");
        rt.update(|s, _| {
            s.tickets.texts.insert(key(&loc), Some(Ok("agy is parked. Until you have a Google account you don't mind losing, any Google model comes through an API key or your opencode Go subscription.".into())));
        });
        rt.frame(0.016);
        rt.frame(0.016);
        assert!(said(&rt).iter().any(|w| w.starts_with("agy is parked.")), "read: its words, {:?}", said(&rt));
    }

}
