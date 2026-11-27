//! Find and the stars (the owner N-234.p3: "yes mostly also the agents need to be able to mark too", "also thigns
//! like search by id etc is needed"; he could not find an earlier long message again). One box on the Threads page:
//! an id typed opens it (a ticket `N-221`, `Q-56`, a post `N-221.p5`, an agent by its name); any other words list the
//! posts that hold them, over every ticket the record holds, closed long ago too (coordd's `Request::Find`). Beside it
//! the Starred list: every star on a thread or a post, the owner's first, with who wrote it and who starred it. A
//! click on a hit opens its thread at that post. A ticket the app does not hold (closed more than a day ago) is
//! fetched from coordd (`Request::Ticket`) and kept while the window lives.
//!
//! One field, at the sidebar's top, in its header, on every page while the sidebar stands open ([`sidebar_field`];
//! consult-lead, N-412 post 1900, 5; paper-research N-1446 p2121, (a)). Over a list of threads, the Threads page's or
//! an agent's With you list, it searches that list, its hits in the list's place (the owner, N-925 post 5: "at the top
//! when i see thread any view o nthreads i need to search"); on any other page its Enter opens the hits on the Threads
//! page, which makes a place for Back. With the sidebar folded away the list's own field stands on top of it
//! ([`search`]), and on a page with no list the box opens over the page ([`over`]). Words are searched as he types,
//! once he pauses ([`typed`]), one search on its way at a time; with `titles only` by the field, the titles of the
//! threads the window holds at each key, coordd not asked ([`titled`]). An id or an agent's name still opens on Enter.
//! In With you the hits are those of the threads between him and that agent, then one row `in all threads` that shows
//! them all there; a hit opens beside the list, in the page he is in (the owner, N-1104 post 512). Clearing the field
//! brings the list back. Wherever it stands, the field stays put while the list scrolls (consult-lead, N-412 post
//! 1212). One field on the screen: in a split, it searches the focused pane's list.
//!
//! The Questions page's search (ask.rs) stays as it is, beside this: it narrows that page's own list, the questions
//! and the mail from before tickets. So does the decisions view's (domain.rs). Neither held the owner's posts.

use crate::{App, Proj};
use coord_types::{FindHit, Request, TicketView};
use rune::{El, TextEdit, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, TextInput};

/// What the list shows in place of the threads: hits of a search, or the starred list.
#[derive(Clone, Debug, PartialEq)]
pub enum Shown {
    Hits { q: String, hits: Result<Vec<FindHit>, String> },
    Starred(Result<Vec<FindHit>, String>),
    /// The agents' stars for the owner not opened yet, newest first (stars.rs; the left menu's Starred).
    ForOwner,
}

#[derive(Default)]
pub struct State {
    pub q: TextEdit,
    pub shown: Option<Shown>,
    /// Tickets fetched from coordd that the live view does not hold, by project id and number. Nothing is dropped:
    /// a thread one project's pin opened stays while another project is shown, and numbers of two projects never meet.
    /// The brain's passes come whole (`passes_asked`), the snapshot's too; where both hold one, the snapshot's is read
    /// first ([`views`]).
    pub fetched:std::collections::HashMap<String, std::collections::HashMap<u64, TicketView>>,
    /// The owner's stars on threads as coordd lists them, by project: read when a project's pipe is first up and when
    /// its Starred row is pressed (a thread closed more than a day ago is no longer in the snapshot).
    pub owned: std::collections::HashMap<String, Vec<FindHit>>,
    /// The projects whose list of his stars has been asked over the pipe.
    pub owned_asked: std::collections::HashSet<String>,
    /// The projects whose brain passes have been asked over the pipe, as his stars are (lib.rs `asks_on_pipe_up`): every
    /// ticket of kind `brain`, kept in `fetched` beside the snapshot's copy, so a pass that left the snapshot a day
    /// after it closed still stands in its agent's chat (agents.rs `passes`).
    pub passes_asked: std::collections::HashSet<String>,
    /// A post to bring into view on its thread's page: ticket, post, frames left to try.
    pub seek: Option<(u64, u32, u8)>,
    /// The post last opened from a hit, drawn marked.
    pub found: Option<(u64, u32)>,
    /// The box and its results open over another page, from the bar's ⌕ or Ctrl+K ([`over`]).
    pub over: bool,
    /// The words searched last (sent, or asked on Enter): what typing is compared with.
    pub searched: String,
    /// The words in the field at the last frame, and when they last changed: a search waits for a pause.
    pub typing: (String, Option<std::time::Instant>),
    /// Each list's offset as its viewport reports it ([`watch`]), by the list's key.
    pub offsets: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, f32>>>,
    /// The offsets of the lists when hits took their place, given back when the list returns.
    pub saved: std::collections::HashMap<String, f32>,
    /// Enter in the sidebar's field opened the Threads page: the focus stays in the field, the same words in it, so
    /// his typing goes on there.
    pub refocus: bool,
    /// Polls left in which the list returned is scrolled back to its saved offset (two: the frame it is drawn again,
    /// and the next, once its rows are laid out).
    pub restore: u8,
    /// Which search's hits are drawn: their viewports are keyed by it, so each search's hits open at their top, the
    /// newest first in sight, whatever the list or the hits before them were scrolled to.
    pub hits_n: u32,
    /// Titles only, the box beside the field (the owner, N-1104 post 496: "typing into the search is way way too slow
    /// eh maybe eends  toggle titles only"): his words are looked for in the titles of the threads this window holds,
    /// at each key, and coordd is not asked ([`titled`]). Kept in ui.json.
    pub titles: bool,
    /// The hits shown are titles only's.
    pub titled: bool,
    /// With you shows every hit, not only those in the threads with its agent ([`agent_results`]); until the field
    /// is emptied.
    pub all: bool,
    /// The end of the pause a wake was asked for. One wake per pause: asked on each frame of it, as before, rune kept
    /// every deadline and the pause asked 13 to 22 wakes a second while he typed (his frames.jsonl on build 27,
    /// `find.rs:287`).
    pub woken: Option<std::time::Instant>,
}

/// How long typing pauses before its words are searched: short enough that the list follows his typing, long enough
/// that each letter does not ask coordd.
const PAUSE: std::time::Duration = std::time::Duration::from_millis(250);

/// The titles-only box's key.
pub const TITLES: &str = "find-titles";

/// The find box's key: the bar's ⌕ and Ctrl+K put the focus in it.
pub const INPUT: &str = "find-input";

/// The bar's ⌕, after the arrows on every page: it opens the find box ([`open`]).
pub fn mark(k: &Kit) -> El {
    crate::history::quiet(k, "⌕", "Find in all threads (Ctrl+K)", false, "find-mark", |s: &mut App, tx: &mut rune::Tx| open(s, tx))
}

/// Opens the find box from wherever he is (the owner, N-925.p1: "i need search across threads eh"), the bar's ⌕ and
/// Ctrl+K alike: the cursor in the sidebar's field while the sidebar stands open, on every page; with it folded away,
/// in the list's own field, or on a page with no list in the same box and results over the page, under the ⌕
/// ([`over`]). The box no longer opens beside the sidebar's field, which it took from the sidebar while it was open.
pub fn open(s: &mut App, tx: &mut rune::Tx) {
    s.tickets.find.over = !has_list(s) && !side_shown(s);
    tx.focus(INPUT, true);
}

/// Whether the sidebar stands open at its width (not folded, not the narrow window's icons).
fn side_shown(s: &App) -> bool {
    s.sidebar.is_open() && !(s.narrow.get() && !s.rail)
}

/// Whether the page shown (the focused pane's) draws a list of threads the field searches ([`list_of`]).
pub fn has_list(s: &App) -> bool {
    list_of(s).is_some()
}

/// The list of threads the page shown (the focused pane's) draws, which the field searches: the Threads page's
/// (`Some(None)`), or an agent's With you list, on its page or as a lens (lens.rs: With you, or a search within it;
/// `Some(Some(agent))`); none on any other page.
fn list_of(s: &App) -> Option<Option<&str>> {
    match &s.sel {
        sel if sel.shows_threads() => Some(None),
        crate::Sel::Agent(a) => crate::withyou::lists(s, s.cur, a).then_some(Some(a.as_str())),
        crate::Sel::Lens(l) => l.with_you().filter(|a| crate::withyou::list_shown(s, s.cur, a)).map(Some),
        _ => None,
    }
}

/// Ctrl+K ([`open`]). Whether the key was it.
pub fn key(s: &mut App, ev: &rune::KeyEvent, tx: &mut rune::Tx) -> bool {
    if !ev.is_command('k') || ev.mods.shift || ev.mods.alt {
        return false;
    }
    open(s, tx);
    true
}

/// The find box over a page other than Threads: [`bar`] and [`results`] themselves, the Threads page's state, in a
/// popover under the bar's ⌕. Esc or a press outside closes it and leaves the page as it was; a hit opens its thread
/// at that post ([`open_at`], which closes it) and so makes a place for Back (history.rs).
pub fn over(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if !s.tickets.find.over || has_list(s) {
        return;
    }
    let Some(p) = s.proj() else { return };
    let mut body = col().key("find-over").padding(10.0).gap(8.0).w(560.0).min_h(0.0).child(bar(s, k));
    if let Some(r) = results(s, p, k) {
        body = body.child(rune_kit::popover::fit_scroll(r));
    }
    let panel = rune_kit::popover::raised(k, body, k.radius.md).modal_tx(true, |s: &mut App, _| s.tickets.find.over = false);
    layer.push(panel.anchored("find-mark", rune::Side::Bottom, rune::Align::Start, 6.0));
}

impl State {
    /// The threads fetched for project `pid`.
    pub fn fetched_in(&self, pid: &str) -> impl Iterator<Item = &TicketView> {
        self.fetched.get(pid).into_iter().flat_map(|m| m.values())
    }
}

/// The find box, on top of the Threads list: [`search`] for all threads.
pub fn bar(s: &App, k: &Kit) -> El {
    search(s, None, k)
}

/// The field on top of a list of threads, the Threads page's (AGENT none) or AGENT's With you list, while the sidebar
/// has none ([`in_sidebar`]). One field and one search wherever it stands: the words, the hits and Enter are the
/// Threads page's own.
pub fn search(s: &App, agent: Option<&str>, k: &Kit) -> El {
    field(s, agent.map_or_else(|| "find-bar".to_string(), |a| format!("find-bar-{a}")), k)
}

/// Whether the sidebar draws the field: on every page, while the sidebar stands open and the box over the page is shut
/// (paper-research N-1446 p2121, (a)). A list of threads then draws none of its own ([`search`]): never two fields with
/// the same words on the screen.
pub fn in_sidebar(s: &App) -> bool {
    !s.tickets.find.over && side_shown(s)
}

/// The sidebar's field ([`in_sidebar`]). It stands in the sidebar's header, at its top above Pinned and Projects,
/// where it stays while the sidebar scrolls (the kit's place for a search field, `Sidebar::header`; consult-lead, N-412
/// post 1900, 5: "the top is where a search field is looked for").
pub fn sidebar_field(s: &App, k: &Kit) -> Option<El> {
    in_sidebar(s).then(|| field(s, "find-bar-sidebar".into(), k))
}

/// What Enter does with an id or a name, said under the field while it has the cursor and is empty (consult-lead,
/// N-412: it keeps what the old `Find: N-221, N-221.p5, an agent, or words` told him).
pub const HINT: &str = "Enter opens an id or agent";

/// [`HINT`] under the sidebar's field, in words that fit under it at the sidebar's width as he first has it, and on a
/// second line in a narrower one: the whole line ran past the sidebar's edge ("an id or an agent's name opens on Ent",
/// e8-side's still).
pub const HINT_SIDE: &str = "an id or an agent opens on Enter";

/// What the field says it searches ([`field`]). At the sidebar's top it names the project, so that above Projects it
/// does not read as a search of every project (consult-lead, N-412 post 1900, 5), in as few words as leave the name
/// room in the narrow sidebar ("Search threads in Research" lost its last letter there, consult-3 N-412 p2163), with
/// `titles only` under it saying the rest; over an agent's With you list it names that list, by the tab's own words (an
/// agent's name was cut there: "Search with paper-researc", e8-side's still). Over a list, which is the project's own,
/// `titles only` beside the field leaves no room for the name.
pub(crate) fn says(s: &App, sidebar: bool) -> String {
    match (sidebar, list_of(s), s.proj()) {
        (true, Some(Some(_)), _) => "Search With you".to_string(),
        (true, _, Some(p)) => format!("Search in {}", p.name),
        _ if s.tickets.find.titles => "Search titles".to_string(),
        _ => "Search threads".to_string(),
    }
}

/// The field itself, keyed KEY, saying what it searches ([`says`]). Under it, one faint line: what Enter does with an
/// id or a name, while it has the cursor and is empty ([`HINT`], in the sidebar [`HINT_SIDE`]); "searching…" while his
/// words wait for coordd's answer ([`searching`]). Beside it, `titles only`; under it in the sidebar, which is narrow.
fn field(s: &App, key: String, k: &Kit) -> El {
    let f = &s.tickets.find;
    let sidebar = key == "find-bar-sidebar";
    let input = TextInput::new(&f.q)
        .placeholder(says(s, sidebar))
        .width(None)
        .on_edit(|s: &mut App| &mut s.tickets.find.q)
        .on_change(|s: &mut App, tx: &mut rune::Tx| edited(s, tx))
        .on_submit(|s: &mut App, _| go(s))
        .build(k)
        .key(INPUT);
    let mut c = col().grow(1.0).w(0.0).gap(2.0).child(input);
    if f.q.focused && f.q.text.is_empty() {
        let hint = text(if sidebar { HINT_SIDE } else { HINT }).key("find-hint").font(k.ty.small).color(k.c.text_faint);
        c = c.child(if sidebar { hint.wrap() } else { hint });
    } else if searching(s) {
        c = c.child(text("searching…").key("find-searching").font(k.ty.small).color(k.c.text_faint));
    } else if sidebar {
        // the line's room kept at the sidebar's top, so the cursor put in the field (the ⌕, Ctrl+K, a click) or a search
        // on its way moves no row beneath it (they moved 14 points at the ⌕ in e8-side's still)
        c = c.child(text(" ").key("find-line-room").font(k.ty.small).color(k.c.text_faint));
    }
    let titles = rune_kit::Checkbox::new(f.titles)
        .label("titles only")
        .on_toggle(|s: &mut App| {
            let f = &mut s.tickets.find;
            f.titles = !f.titles;
            // the words in the field searched again, the other way
            f.searched.clear();
        })
        .build(k)
        .key(TITLES);
    // the stars are the left menu's Starred, one way in (paper-research N-371.p5)
    if sidebar {
        return row().key(key).w_full().child(c.child(titles));
    }
    row().key(key).w_full().gap(8.0).center_y().child(c).child(titles)
}

/// His edit in the field, as it is made. A frame polls before it takes its keys, so [`typed`] sees a key's words a
/// frame later, and only once something wakes the window: here the pause starts at the key, and titles only answers
/// in the key's own frame.
fn edited(s: &mut App, tx: &mut rune::Tx) {
    let q = s.tickets.find.q.text.trim().to_string();
    if s.tickets.find.typing.0 != q {
        s.tickets.find.typing = (q, Some(std::time::Instant::now()));
    }
    typed(s, tx);
}

/// Whether the field's words wait for coordd's answer: words, not an id or a name, searched the slow way, and no
/// answer to them shown yet, while they are still to be asked after the pause or are on their way.
pub(crate) fn searching(s: &App) -> bool {
    let f = &s.tickets.find;
    let q = f.q.text.trim();
    let shown = f.titled == f.titles && matches!(&f.shown, Some(Shown::Hits { q: was, .. }) if was == q);
    !f.titles && !q.is_empty() && !shown && !waits_for_enter(s, q) && (f.searched != q || on_its_way(s))
}

/// An id or an agent's name: opened on Enter, never searched as he types.
fn waits_for_enter(s: &App, q: &str) -> bool {
    place(q).is_some() || s.proj().is_some_and(|p| p.live.agents.iter().any(|a| a.name == q))
}

/// Whether a search by words is on its way to a coordd. One goes at a time: asked at each pause, as before, the
/// searches waited for one another in coordd, about 1.5 s each, so the answer to his last words came up to 12.7 s
/// after he stopped typing, and the answers to the words before were thrown away (his window's log on build 27,
/// 20 Nov 11:41 to 11:42: `find took` 1684 ms, then 2474, 3742 … 12677). A pipe that goes down drops what was
/// asked through it (lib.rs), so a search lost with it is not waited for.
fn on_its_way(s: &App) -> bool {
    s.projects.iter().any(|p| p.pending.values().any(|w| matches!(w, crate::Pending::Find { .. })))
}

/// Titles only: the threads and items this window holds (the live view's, and those fetched), each word of Q in its
/// label, its title or its id, by coordd's own rule (`coord_types::holds_words`), newest first by their last post;
/// each with its state. Threads closed more than a day ago are not held, so only the search by words finds them.
fn titled(s: &App, q: &str) -> Vec<FindHit> {
    let Some(p) = s.proj() else { return Vec::new() };
    let words = coord_types::query_words(q);
    let mut seen = std::collections::HashSet::new();
    let hits = views(s, p).filter(|v| (v.kind == "thread" || v.kind == "item") && seen.insert(v.n));
    let hits = hits.filter(|v| coord_types::holds_words(&format!("{}\n{}\n{}", crate::outline::label(p, v), v.title, v.id), &words));
    newest_first(
        hits.map(|v| FindHit {
            id: v.id.clone(),
            t: v.n,
            post: None,
            title: v.title.clone(),
            by: v.opener.clone().unwrap_or_default(),
            at: v.posts.last().map_or(&v.opened_at, |x| &x.at).to_string(),
            words: crate::outline::state(v).0,
            star: None,
            archived: v.archived || v.archived_under.is_some(),
        })
        .collect(),
    )
}

/// What list KEY's viewport reports of its offset (`El::on_scroll_offset`), kept so the list comes back where he left
/// it once the hits that took its place are cleared.
pub fn watch(s: &App, key: String) -> impl Fn(rune::ScrollPos) + 'static {
    let at = s.tickets.find.offsets.clone();
    move |pos: rune::ScrollPos| {
        if let Ok(mut m) = at.lock() {
            m.insert(key.clone(), pos.offset[1]);
        }
    }
}

/// The hits of Q shown in place of the lists, their offsets kept when the lists were showing until now. An answer
/// never moves him (consult-lead, N-412: he is taken to Threads by his own act, Enter in the sidebar's field, [`go`]):
/// one that comes after he went on is kept, and drawn on the next list he opens with these words in the field.
fn show_hits(s: &mut App, q: String, hits: Result<Vec<FindHit>, String>, titled: bool) {
    let f = &mut s.tickets.find;
    f.titled = titled;
    match &f.shown {
        Some(Shown::Hits { q: was, .. }) if *was == q => {}
        Some(Shown::Hits { .. }) => f.hits_n += 1,
        _ => {
            f.saved = f.offsets.lock().map(|m| m.clone()).unwrap_or_default();
            f.hits_n += 1;
        }
    }
    f.shown = Some(Shown::Hits { q, hits });
}

/// Words Q asked of coordd (`Request::Find`), the answer coming as [`came`]: on Enter, or once he pauses.
fn ask_words(s: &mut App, q: String) {
    s.tickets.find.searched = q.clone();
    s.request_in(s.cur, Request::Find { q: q.clone(), starred: false, wide: false }, crate::Pending::Find { q });
}

/// The hits taken away: the lists come back, scrolled back to where he left them.
fn hide_hits(f: &mut State) {
    f.shown = None;
    f.restore = 2;
    f.all = false;
}

/// The hits for AGENT's With you list, when words are searched: those in the threads between him and AGENT, newest
/// first, then one row `in all threads` that shows every hit there, in With you, and back (`only with AGENT`). A press
/// on a hit opens its thread beside the list ([`beside_row`]): a search opens in the page he is in (the owner, N-1104
/// post 512: "its within the context eh nto a new one").
pub fn agent_results(s: &App, pi: usize, agent: &str, k: &Kit) -> Option<El> {
    let f = &s.tickets.find;
    let Some(Shown::Hits { q, hits }) = &f.shown else { return None };
    let p = s.projects.get(pi)?;
    let who = crate::refs::person(agent);
    let head = match (f.all, f.titled) {
        (true, true) => format!("IN ALL TITLES: {q}"),
        (true, false) => format!("FOUND IN ALL THREADS: {q}"),
        (false, true) => format!("IN TITLES WITH {who}: {q}"),
        (false, false) => format!("FOUND WITH {who}: {q}"),
    };
    let mut c = col().key(format!("find-results-{agent}")).w_full().gap(6.0).px(12.0).child(text(head).font(k.ty.small_medium).color(k.c.text_muted));
    let hs = match hits {
        Err(e) => return Some(c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap())),
        Ok(hs) => hs,
    };
    // the threads of the agent's list (the one list narrowed to it, outline.rs), closed ones too; or every hit
    let mine: Vec<&FindHit> = hs.iter().filter(|h| f.all || p.live.tickets.get(&h.t).is_some_and(|v| crate::outline::is_with(v, agent))).collect();
    if mine.is_empty() {
        let none = match (f.all, f.titled) {
            (true, true) => "No title matches; try with titles only off.",
            (true, false) => "No post holds those words.",
            (false, true) => "No title matches.",
            (false, false) => "No post with this agent holds those words.",
        };
        c = c.child(text(none).font(k.ty.small).color(k.c.text_faint));
    }
    c = c.children(mine.into_iter().map(|h| beside_row(s, pi, agent, p, h, k)));
    let (widen, label, key) = if f.all { (false, format!("only with {who}"), "find-only-with") } else { (true, format!("in all threads · {}", hs.len()), "find-in-all") };
    Some(c.child(Button::new(label).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| s.tickets.find.all = widen).build(k).key(key)))
}

/// A hit in AGENT's With you: a press opens its thread beside the list, at the post found, as a press on the list's
/// line does (outline.rs `open_beside`). A thread the window does not hold (closed before today) has no place there,
/// so it opens on the Threads page, which fetches it ([`open_at`]).
fn beside_row(s: &App, pi: usize, agent: &str, p: &Proj, h: &FindHit, k: &Kit) -> El {
    let (t, id, post, who) = (h.t, h.id.clone(), h.post, agent.to_string());
    hit_el(s, p, h, k).on_press(move |s: &mut App| {
        if !s.projects.get(pi).is_some_and(|p| p.live.tickets.contains_key(&t)) {
            return open_at(s, t, &id, post);
        }
        crate::outline::open_beside(s, pi, &who, t);
        if let Some(n) = post {
            seek_post(s, t, &id, n);
        }
    })
}

/// Each frame: words typed into the field are searched once he pauses, the reply coming as [`came`]; an emptied
/// field brings the list back. An id or an agent's name waits for Enter ([`go`]). True when the list changed.
pub fn typed(s: &mut App, tx: &mut rune::Tx) -> bool {
    let q = s.tickets.find.q.text.trim().to_string();
    let f = &mut s.tickets.find;
    if std::mem::take(&mut f.refocus) {
        tx.focus(INPUT, true);
    }
    // at once, not a glide: the list is back where he left it, as if it had never gone
    if f.restore > 0 {
        f.restore -= 1;
        for (key, y) in &f.saved {
            tx.scroll_to_y(key.clone(), *y);
        }
    }
    if f.typing.0 != q {
        f.typing = (q.clone(), Some(std::time::Instant::now()));
    }
    if q.is_empty() {
        f.searched.clear();
        let shown = matches!(f.shown, Some(Shown::Hits { .. }));
        if shown {
            hide_hits(f);
        }
        return shown;
    }
    if f.searched == q || waits_for_enter(s, &q) {
        return false;
    }
    // titles only: looked for here at each key, nothing waited for
    if s.tickets.find.titles {
        titles_shown(s, q);
        return true;
    }
    // one search on its way at a time: the words in the field by its answer go then, their pause long over
    if on_its_way(s) {
        return false;
    }
    let f = &mut s.tickets.find;
    if !paused(f.typing.1, PAUSE, &mut f.woken, tx) {
        return false;
    }
    ask_words(s, q);
    false
}

/// Whether words typed at AT (none: nothing typed) have paused for LONG by now; if not, one wake asked for the pause's
/// end and kept in WOKEN: one per pause, not one per frame (`State::woken`). The search's pause, and the live edit's
/// reach (edit.rs).
pub fn paused(at: Option<std::time::Instant>, long: std::time::Duration, woken: &mut Option<std::time::Instant>, tx: &mut rune::Tx) -> bool {
    let now = std::time::Instant::now();
    let due = at.map_or(now, |at| at + long);
    if now < due {
        if *woken != Some(due) {
            *woken = Some(due);
            tx.wake_after(due - now + std::time::Duration::from_millis(1));
        }
        return false;
    }
    true
}

/// The titles-only hits of Q in the list's place ([`titled`]), as an answer of coordd's would be.
fn titles_shown(s: &mut App, q: String) {
    let hits = titled(s, &q);
    s.tickets.find.searched = q.clone();
    show_hits(s, q, Ok(hits), true);
}

/// coordd's answer to the search of Q: its hits shown newest first, unless the field holds other words by now (then
/// nothing is drawn: the words in the field are asked in their turn, or the field is empty and the list stays), or
/// titles only was set since it was asked (the titles' hits stay).
pub(crate) fn came(s: &mut App, q: String, ok: bool, out: String) {
    if s.tickets.find.q.text.trim() != q || s.tickets.find.titles {
        return;
    }
    let hits = if ok { parse(&out).map(newest_first) } else { Err(out) };
    show_hits(s, q, hits, false);
}

/// The hits or the starred list, when one is asked for: in the list's place, a way back on top.
pub fn results(s: &App, p: &Proj, k: &Kit) -> Option<El> {
    let shown = s.tickets.find.shown.as_ref()?;
    if *shown == Shown::ForOwner {
        return Some(for_owner(s, p, k));
    }
    let (head, hits) = match shown {
        Shown::Hits { q, hits } if s.tickets.find.titled => (format!("IN TITLES: {q}"), hits),
        Shown::Hits { q, hits } => (format!("FOUND: {q}"), hits),
        Shown::Starred(hits) => ("STARRED: yours first".to_string(), hits),
        Shown::ForOwner => return None,
    };
    let back = Button::new("‹ the threads").variant(ButtonVariant::Ghost).on_press(|s: &mut App| clear(s)).build(k);
    let mut c = col().key("find-results").w_full().gap(6.0).child(row().w_full().center_y().child(text(head).font(k.ty.small_medium).color(k.c.text_muted).grow(1.0)).child(back));
    match hits {
        Err(e) => c = c.child(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap()),
        Ok(hs) if hs.is_empty() => c = c.child(text(if matches!(shown, Shown::Starred(_)) { "Nothing starred yet." } else if s.tickets.find.titled { "No title matches; try with titles only off." } else { "No post holds those words." }).font(k.ty.small).color(k.c.text_faint)),
        Ok(hs) => c = c.children(hs.iter().map(|h| hit_row(s, p, h, k))),
    }
    Some(c)
}

/// The list back in place of the hits: the field emptied with them, so the two agree.
fn clear(s: &mut App) {
    let f = &mut s.tickets.find;
    hide_hits(f);
    f.q = TextEdit::default();
    f.searched.clear();
}

/// The agents' stars for him, newest first; a click opens the post and it leaves the list.
fn for_owner(s: &App, p: &Proj, k: &Kit) -> El {
    let hs = crate::stars::for_owner(s, p);
    let mine = crate::stars::own(p, s.tickets.find.owned.get(&p.id).map_or(&[][..], |v| v));
    let back = Button::new("‹ the threads").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.tickets.find.shown = None).build(k);
    let mut c = col().key("stars-for-owner").w_full().gap(6.0).child(row().w_full().center_y().child(text("STARRED: for you, then yours").font(k.ty.small_medium).color(k.c.text_muted).grow(1.0)).child(back));
    if hs.is_empty() && mine.is_empty() {
        return c.child(text("Nothing starred.").font(k.ty.small).color(k.c.text_faint));
    }
    let pi = s.cur;
    for h in hs {
        let open = h.clone();
        c = c.child(hit_el(s, p, &h, k).on_press(move |s: &mut App| crate::stars::open_hit(s, pi, &open)));
    }
    // his own stars on threads stay until he takes them off (the thread's ⋯)
    for h in mine {
        c = c.child(hit_el(s, p, &h, k).on_press(move |s: &mut App| open_at(s, h.t, &h.id, None)));
    }
    c
}

/// One hit: its id and its thread's title, the post's first line (or the line found), who wrote it and when, and
/// who starred it; a click opens the thread at that post.
fn hit_row(s: &App, p: &Proj, h: &FindHit, k: &Kit) -> El {
    let (t, id, post) = (h.t, h.id.clone(), h.post);
    hit_el(s, p, h, k).on_press(move |s: &mut App| open_at(s, t, &id, post))
}

/// A hit's card, without what a click does: its thread as a reference (`label · id`: coordd's label, else the title
/// Find gave), the words, who wrote it and when; and, in his Archive (archive.rs), that it is.
fn hit_el(s: &App, p: &Proj, h: &FindHit, k: &Kit) -> El {
    let who = crate::refs::person(&h.by);
    let mut sub = vec![format!("{who} · {}", crate::when::at(&h.at))];
    // a thread he archived is still found, and says so
    if h.archived || p.live.tickets.get(&h.t).is_some_and(|v| v.archived) {
        sub.push("archived".into());
    }
    if p.live.tickets.get(&h.t).is_some_and(|v| crate::archive::from_archive(s, p, v)) {
        sub.push("in the Archive".into());
    }
    if let Some((by, at)) = &h.star {
        // who starred it, when that is not who wrote it
        let whose = if *by == h.by { String::new() } else { format!("{} ", crate::refs::person(by)) };
        sub.push(format!("★ {whose}{}", crate::when::at(at)));
    }
    let tid = h.id.split('.').next().unwrap_or(&h.id);
    let label = p.labels.get(tid).map(str::to_string).or_else(|| (!h.title.is_empty()).then(|| h.title.clone()));
    let r = crate::refs::Ref { label: label.unwrap_or_else(|| crate::refs::ticket_id(p, tid).label), id: h.id.clone() };
    let words = if h.words.is_empty() { "(the thread itself)".to_string() } else { h.words.clone() };
    k.interactive(
        col()
            .key(format!("hit-{}", h.id))
            .w_full()
            .gap(2.0)
            .px(10.0)
            .py(8.0)
            .radius(k.radius.sm)
            .fill(k.c.layer_01)
            .child(crate::refs::el(&r, k.ty.small_medium, k.c.text_base, k))
            .child(text(words).font(k.ty.small).color(k.c.text_base).wrap())
            .child(text(sub.join(" · ")).font(k.ty.small).color(k.c.text_muted)),
    )
}

/// coordd's find: Q's hits, or every star.
pub(crate) fn ask(s: &App, q: String, starred: bool) -> Result<Vec<FindHit>, String> {
    let p = s.proj().ok_or_else(|| "no project".to_string())?;
    parse(&p.ask_once(&Request::Find { q, starred, wide: false })?)
}

/// Find's answer: one hit per line.
fn parse(out: &str) -> Result<Vec<FindHit>, String> {
    out.lines().filter(|l| !l.trim().is_empty()).map(|l| serde_json::from_str(l).map_err(|e| format!("search failed: {e}"))).collect()
}

/// What the box holds, on Enter: an id opens it; an agent's name its page; other words are searched. What opened
/// leaves the field empty ([`clear`]), so the next words he types there are a search of their own, not added to the id
/// (73 kept it: "N-1446", Enter, then "docket-core" typed read "N-1446docket-core", docket-edge8's walk, N-1446
/// p2353); an id that opens nothing stays, for him to put right.
pub(crate) fn go(s: &mut App) {
    let q = s.tickets.find.q.text.trim().to_string();
    if q.is_empty() {
        hide_hits(&mut s.tickets.find);
        return;
    }
    let Some(p) = s.proj() else { return };
    if p.live.agents.iter().any(|a| a.name == q) {
        s.tickets.find.over = false;
        s.select_agent(&q);
        clear(s);
        return;
    }
    if let Some((id, post)) = place(&q) {
        match number(s, &id) {
            Ok(t) => {
                open_at(s, t, &q, post);
                clear(s);
            }
            Err(e) => s.tickets.find.shown = Some(Shown::Hits { q, hits: Err(e) }),
        }
        return;
    }
    // Enter in the sidebar's field on a page with no list takes him to the Threads page at once, before any answer: a
    // place Back returns from, his typing going on in the field (consult-lead, N-412); the hits draw there when they
    // come. Over a list, they draw in its place, where he is
    if !has_list(s) && in_sidebar(s) {
        s.sel = crate::Sel::Tickets;
        s.tickets.shown = None;
        s.tickets.find.refocus = true;
    }
    // asked as the pause asks, the answer shown newest first by [`came`]; titles only, at once; with a search on its
    // way, the words go when it is answered ([`typed`])
    if s.tickets.find.titles {
        titles_shown(s, q);
    } else if !on_its_way(s) {
        ask_words(s, q);
    }
}

/// HITS ordered by when each post was written, newest first (the times are coordd's RFC 3339, all UTC).
fn newest_first(mut hits: Vec<FindHit>) -> Vec<FindHit> {
    hits.sort_by(|a, b| b.at.cmp(&a.at));
    hits
}

/// `N-221` or `N-221.p5` as a ticket's id and a post: letters, a dash, digits (the case of the letters as typed or
/// not).
pub fn place(q: &str) -> Option<(String, Option<u32>)> {
    let (id, post) = match q.split_once('.') {
        Some((id, p)) => (id, Some(p.strip_prefix('p').or_else(|| p.strip_prefix('P'))?.parse::<u32>().ok()?)),
        None => (q, None),
    };
    let (l, n) = id.split_once('-')?;
    (!l.is_empty() && l.chars().all(|c| c.is_ascii_alphabetic()) && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then(|| (format!("{}-{n}", l.to_ascii_uppercase()), post))
}

/// Ticket ID's number: from the live view, else fetched from coordd (and kept).
pub(crate) fn number(s: &mut App, id: &str) -> Result<u64, String> {
    let Some(p) = s.proj() else { return Err("no project".into()) };
    if let Some(n) = held(s, p, id) {
        return Ok(n);
    }
    let pid = p.id.clone();
    let out = p.ask_once(&Request::Ticket { t: id.to_string() })?;
    let v: TicketView = serde_json::from_str(&out).map_err(|e| format!("{id}: {e}"))?;
    let n = v.n;
    s.tickets.find.fetched.entry(pid).or_default().insert(n, v);
    Ok(n)
}

/// Project P's tickets the window holds: the snapshot's, then those a fetch has ([`number`]).
pub(crate) fn views<'a>(s: &'a App, p: &'a Proj) -> impl Iterator<Item = &'a TicketView> {
    p.live.tickets.values().chain(s.tickets.find.fetched_in(&p.id))
}

/// The number of project P's ticket ID when the snapshot holds it or a fetch has it ([`number`]), asking nothing.
pub(crate) fn held(s: &App, p: &Proj, id: &str) -> Option<u64> {
    views(s, p).find(|v| v.id == id).map(|v| v.n)
}

/// Open ticket T (ID) at POST: its thread shown, the post brought into view and marked.
pub fn open_at(s: &mut App, t: u64, id: &str, post: Option<u32>) {
    if s.proj().is_some_and(|p| !p.live.tickets.contains_key(&t) && !s.tickets.find.fetched.get(&p.id).is_some_and(|m| m.contains_key(&t))) {
        let tid = id.split('.').next().unwrap_or(id).to_string();
        if let Err(e) = number(s, &tid) {
            s.notice = Some(e);
            return;
        }
    }
    // the popups over the page close, so the post shows: a post of the thread under them makes no new place
    // (history.rs), and they would stay over it (popup.rs)
    crate::popup::close(s);
    // under a lens the thread opens beside its list, as on Threads (lens.rs)
    if !s.sel.shows_threads() {
        s.sel = crate::Sel::Tickets;
    }
    s.tickets.find.over = false;
    s.tickets.shown = Some(t);
    s.tickets.opening = None;
    s.tickets.details.remove(&t);
    s.tickets.how.remove(&t);
    if let Some(n) = post {
        seek_post(s, t, id, n);
    }
}

/// Post N of ticket T (ID) to be brought into view on its page, wherever that is drawn, and marked.
fn seek_post(s: &mut App, t: u64, id: &str, n: u32) {
    // the post sought wins over where he was reading it before a swap (read_place.rs; consult-2, N-586.p37)
    if let Some(pid) = s.proj().map(|p| p.id.clone()) {
        let tid = id.split('.').next().unwrap_or(id);
        s.read_places.sought(&crate::read_place::thread_view(&pid, tid));
    }
    s.tickets.folded.remove(&(t, n));
    s.tickets.find.found = Some((t, n));
    s.tickets.find.seek = Some((t, n, 3));
}

/// The post sought, once its page is drawn: the lines it hangs under opened, then brought into view (a few frames,
/// as the lines open first).
pub fn seek(s: &mut App, tx: &mut rune::Tx) -> bool {
    let Some((t, n, left)) = s.tickets.find.seek else { return false };
    let Some(p) = s.proj() else { return false };
    let Some(v) = ticket(s, p, t) else {
        s.tickets.find.seek = None;
        return false;
    };
    let id = v.id.clone();
    let open = crate::ran::above(p, v, &format!("{id}.p{n}"));
    s.tickets.ran.extend(open);
    tx.scroll_into_view(format!("post-{id}-{n}"));
    s.tickets.find.seek = (left > 1).then_some((t, n, left - 1));
    true
}

/// Who starred post POST of V (None: the thread), the owner first: "you", then the agents.
pub fn starred_by(v: &TicketView, post: Option<u32>) -> Vec<String> {
    let mut by: Vec<&str> = v.stars.iter().filter(|x| x.post == post).map(|x| x.by.as_str()).collect();
    // a thread starred before the stars were kept by whom (`starred` alone) is the owner's
    if post.is_none() && v.starred && !by.contains(&"owner") {
        by.push("owner");
    }
    by.sort_by_key(|b| *b != "owner");
    by.into_iter().map(crate::refs::person).collect()
}

/// The star of post POST of V (None: the thread): ☆, or ★ and who starred it; a click sets or takes off the owner's.
pub fn star(v: &TicketView, post: Option<u32>, k: &Kit) -> El {
    let by = starred_by(v, post);
    let mine = by.first().is_some_and(|b| b == "you");
    let label = if by.is_empty() { "☆".to_string() } else { format!("★ {}", by.join(", ")) };
    let color = if by.is_empty() { k.c.text_faint } else { k.c.warning.fg };
    let place = match post {
        Some(n) => format!("{}.p{n}", v.id),
        None => v.id.clone(),
    };
    let key = format!("pstar-{place}");
    k.interactive(row().key(key).child(text(label).font(k.ty.small).color(color)))
        .on_press(move |s: &mut App| {
            s.request_in(s.cur, Request::Star { t: place.clone(), on: !mine }, crate::Pending::TicketDone { clear: crate::threads::Cleared::Quiet });
        })
}

/// Ticket T as the page draws it: the live view's, else the one fetched.
pub fn ticket<'a>(s: &'a App, p: &'a Proj, t: u64) -> Option<&'a TicketView> {
    p.live.tickets.get(&t).or_else(|| s.tickets.find.fetched.get(&p.id).and_then(|m| m.get(&t)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn hits_are_newest_first() {
        let hit = |id: &str, at: &str| -> coord_types::FindHit { serde_json::from_value(serde_json::json!({"id": id, "t": 1, "at": at})).unwrap() };
        let got = super::newest_first(vec![hit("N-1.p1", "2026-11-18T09:00:00Z"), hit("N-2.p4", "2026-11-19T08:00:00Z"), hit("N-1.p2", "2026-11-18T10:00:00Z")]);
        assert_eq!(got.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["N-2.p4", "N-1.p2", "N-1.p1"]);
    }

    use super::*;
    use crate::Sel;
    use rune::{KeyEvent, Tx};

    /// The window on project research: agent pr, 40 threads he wrote to pr (N-10 to N-49) and N-99 to someone else;
    /// the window's poll is find.rs's and history.rs's parts (the rest reads pipes this project lacks).
    fn window() -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
        use serde_json::json;
        let (held, mut s) = crate::view_cost::test_app();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.live.agents = vec![serde_json::from_value(json!({"id": "AGT-1", "name": "pr", "parent": "owner", "kind": "claude", "model": "m",
            "state": "idle", "since": "2026-11-19T08:00:00Z", "tools": [], "items": []})).unwrap()];
        let thread = |n: u64, to: &str| -> coord_types::TicketView {
            serde_json::from_value(json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": to},
                "title": format!("thread {n}"), "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
                "opened_at": "2026-11-19T08:00:00Z", "given": [],
                "posts": [{"n": 1, "at": format!("2026-11-19T07:{:02}:00Z", n % 60), "by": "owner", "text": format!("note {n}"), "names": [to], "told": [to]}]})).unwrap()
        };
        for n in 10..50 {
            p.live.tickets.insert(n, thread(n, "pr"));
        }
        p.live.tickets.insert(99, thread(99, "qa"));
        s.projects = vec![p];
        s.cur = 0;
        s.sel = Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        fn poll_parts(s: &mut App, _: f64, tx: &mut Tx) -> bool {
            typed(s, tx) | crate::history::poll(s)
        }
        rt.poll = Some(poll_parts);
        rt.on_key = Some(crate::on_key);
        rt.frame(0.016);
        (held, rt)
    }

    fn frames(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..4 {
            rt.frame(0.016);
        }
    }

    fn press(rt: &mut rune::runtime::Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is drawn"));
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        frames(rt);
    }

    fn type_in(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        for c in words.chars() {
            rt.key(KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
            rt.frame(0.016);
        }
    }

    /// Words typed, the pause waited out: the search is asked of coordd (here its answer is given as coordd's).
    fn searched(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        assert_eq!(rt.state.tickets.find.q.text, words, "typed into the field");
        frames(rt);
        // the pause, on the clock `typed` reads and on the window's, whose wake brings the poll
        std::thread::sleep(PAUSE + std::time::Duration::from_millis(50));
        for _ in 0..(PAUSE.as_millis() / 16 + 4) {
            rt.frame(0.016);
        }
        let f = &rt.state.tickets.find;
        assert_eq!(f.searched, words, "searched once he paused: typing {:?}, notice {:?}, shown {:?}", f.typing, rt.state.notice, f.shown.is_some());
    }

    /// coordd's answer: hits on N-12.p1 (07:12) and N-30.p1 (07:30), and N-99.p1 (07:39, to qa), in Find's order.
    fn answer(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        let hit = |t: u64, at: &str| serde_json::json!({"id": format!("N-{t}.p1"), "t": t, "post": 1, "at": at, "by": "owner", "words": format!("the {words} in note {t}")}).to_string();
        let out = [hit(12, "2026-11-19T07:12:00Z"), hit(99, "2026-11-19T07:39:00Z"), hit(30, "2026-11-19T07:30:00Z")].join("\n");
        rt.update(|s, _| came(s, words.into(), true, out));
        frames(rt);
    }

    fn y(rt: &rune::runtime::Runtime<App>, key: &str) -> f32 {
        rt.find(key).unwrap_or_else(|| panic!("{key} is drawn")).1
    }

    fn offset(rt: &rune::runtime::Runtime<App>, list: &str) -> f32 {
        rt.state.tickets.find.offsets.lock().unwrap().get(list).copied().unwrap_or(0.0)
    }

    /// LIST scrolled down by the wheel over it; the field stands where it stood (it does not scroll with the list).
    /// The offset scrolled to.
    fn scrolled(rt: &mut rune::runtime::Runtime<App>, list: &str) -> f32 {
        let field = rt.find(INPUT).expect("the field is drawn");
        let (lx, ly) = rt.find(list).unwrap_or_else(|| panic!("{list} is drawn"));
        for _ in 0..6 {
            rt.wheel(lx, ly, -120.0);
            rt.frame(0.016);
        }
        frames(rt);
        let was = offset(rt, list);
        assert!(was > 0.0, "{list} scrolled: offsets {:?}", rt.state.tickets.find.offsets.lock().unwrap());
        assert_eq!(rt.find(INPUT), Some(field), "the field stays above {list} while it scrolls");
        was
    }

    /// The field emptied: the hits go and the list comes back at offset WAS.
    fn cleared(rt: &mut rune::runtime::Runtime<App>, list: &str, was: f32) {
        for _ in 0..32 {
            if rt.state.tickets.find.q.text.is_empty() {
                break;
            }
            rt.key(KeyEvent { key: rune::Key::Backspace, mods: Default::default() });
            rt.frame(0.016);
        }
        frames(rt);
        assert!(rt.state.tickets.find.q.text.is_empty(), "the field emptied: {:?}", rt.state.tickets.find.q.text);
        assert!(rt.state.tickets.find.shown.is_none() && rt.find("hit-N-30.p1").is_none(), "the list is back");
        assert!((offset(rt, list) - was).abs() < 1.0, "{list} at the place he left: {} for {was}", offset(rt, list));
    }

    /// The Threads page: the field at the list's top, where it stays while the list scrolls under it; a word typed
    /// there after a scroll is searched once he pauses, the posts holding it shown newest first; emptying the field
    /// brings the list back where he had scrolled it.
    #[test]
    fn the_threads_list_has_a_search_field_and_clearing_it_brings_the_list_back_where_it_was() {
        let (_held, mut rt) = window();
        assert!(rt.find(INPUT).is_some(), "the field is there at first sight");
        // the list by time: every one of the 40 threads a row, more than the window holds
        rt.update(|s, _| s.tickets.by_time = true);
        frames(&mut rt);
        let was = scrolled(&mut rt, "ticket-list");
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        assert!(y(&rt, "hit-N-99.p1") < y(&rt, "hit-N-30.p1") && y(&rt, "hit-N-30.p1") < y(&rt, "hit-N-12.p1"), "newest first");
        cleared(&mut rt, "ticket-list", was);
    }

    /// pr's With you list, searched from the sidebar's field, which stays where it is while the list scrolls; the hits
    /// are those of the threads between him and pr, newest first; emptying the field brings the list back where he had
    /// scrolled it; `in all threads` shows every hit there, in With you (the owner, N-1104 post 512: "its within the
    /// context eh nto a new one"), and `only with pr` narrows them again.
    #[test]
    fn with_you_searches_the_agents_threads_first_and_offers_all_threads() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.sel = Sel::Agent("pr".into()));
        frames(&mut rt);
        assert!(rt.find("find-bar-sidebar").is_some(), "the sidebar's field over pr's list at first sight");
        // the 40 threads with pr each a row, the earlier ones unfolded: more than the window holds
        if rt.find("with-you-earlier").is_some() {
            press(&mut rt, "with-you-earlier");
        }
        let list = "with-you-list-0-pr";
        let was = scrolled(&mut rt, list);
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        assert!(y(&rt, "hit-N-30.p1") < y(&rt, "hit-N-12.p1"), "newest first");
        assert!(rt.find("hit-N-99.p1").is_none(), "only the threads with pr");
        cleared(&mut rt, list, was);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        press(&mut rt, "find-in-all");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "he stays in With you");
        assert!(rt.find("hit-N-99.p1").is_some(), "every hit, there");
        press(&mut rt, "find-only-with");
        assert!(rt.find("hit-N-99.p1").is_none() && rt.find("hit-N-30.p1").is_some(), "the threads with pr again");
    }

    /// A hit pressed in pr's With you opens its thread beside the list, at the post found, as a press on the list's
    /// line does: no other page and no other viewer (the owner, N-1104 post 512: "wghen i earch and open a thread i get
    /// some other thread viewer that doesn to have iwht you with other temrian letc").
    #[test]
    fn a_hit_in_with_you_opens_its_thread_beside_the_list() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.sel = Sel::Agent("pr".into()));
        frames(&mut rt);
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        press(&mut rt, "hit-N-30.p1");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "he stays in With you");
        assert_eq!(rt.state.withyou.inside.get(&(0, "pr".to_string())), Some(&30), "the thread beside the list");
        assert_eq!(rt.state.tickets.shown, None, "not the Threads page's thread");
        assert_eq!(rt.state.tickets.find.found, Some((30, 1)), "the post found is marked");
        assert!(rt.find("with-you-thread-N-30").is_some(), "drawn in With you");
    }

    /// The sidebar's field, on a page with no list of threads (the Agents page): words typed there are searched once
    /// he pauses, and their answer does not move him (consult-lead, N-412: an answer never moves him); his Enter takes
    /// him to the Threads page, the hits drawn there, the cursor going on in the field; Back returns to the Agents
    /// page. On the Threads page the field stays in the sidebar, and the page has none of its own (N-1446 p2121).
    #[test]
    fn the_sidebars_field_on_a_page_without_a_list_opens_the_hits_on_the_threads_page() {
        let (_held, mut rt) = window();
        assert!(rt.find("find-bar-sidebar").is_some() && rt.find("find-bar").is_none(), "the sidebar's on the Threads page, none of its own");
        rt.update(|s, _| s.sel = Sel::Agents);
        frames(&mut rt);
        assert!(rt.find("find-bar-sidebar").is_some(), "the sidebar's field on the Agents page");
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        assert_eq!(rt.state.sel, Sel::Agents, "the answer by the pause leaves him where he is");
        rt.key(KeyEvent { key: rune::Key::Enter, mods: Default::default() });
        frames(&mut rt);
        assert_eq!(rt.state.sel, Sel::Tickets, "his Enter: the hits on the Threads page");
        answer(&mut rt, "deep");
        assert!(y(&rt, "hit-N-99.p1") < y(&rt, "hit-N-30.p1"), "newest first");
        assert!(rt.find("find-bar-sidebar").is_some() && rt.find("find-bar").is_none(), "one field, the sidebar's");
        rt.key(KeyEvent { key: rune::Key::Text("e".into()), mods: Default::default() });
        rt.frame(0.016);
        assert_eq!(rt.state.tickets.find.q.text, "deepe", "his typing goes on in the field");
        crate::history::go(&mut rt.state, true, 1);
        assert_eq!(rt.state.sel, Sel::Agents, "Back: where he searched from");
    }

    /// Ctrl+K puts the cursor in the field of the page he is on: the list's, else the sidebar's.
    #[test]
    fn ctrl_k_goes_to_the_field_of_the_page_or_over_a_page_without_one() {
        let (_held, mut rt) = window();
        let ctrl_k = KeyEvent { key: rune::Key::Command('k'), mods: Default::default() };
        for (sel, over) in [(Sel::Tickets, false), (Sel::Agent("pr".into()), false), (Sel::Agents, false)] {
            rt.update(|s, _| {
                s.sel = sel.clone();
                s.tickets.find.over = false;
            });
            frames(&mut rt);
            rt.key(ctrl_k.clone());
            frames(&mut rt);
            assert_eq!(rt.state.tickets.find.over, over, "{sel:?}");
            assert!(rt.find(INPUT).is_some(), "{sel:?}: the field is drawn");
            rt.key(KeyEvent { key: rune::Key::Text("x".into()), mods: Default::default() });
            rt.frame(0.016);
            assert_eq!(rt.state.tickets.find.q.text, "x", "{sel:?}: the cursor is in it");
            rt.update(|s, _| s.tickets.find.q = TextEdit::default());
            rt.key(KeyEvent { key: rune::Key::Escape, mods: Default::default() });
            frames(&mut rt);
        }
    }

    /// One field, in the sidebar, on every page (paper-research N-1446 p2121, (a)): on the Threads page, over pr's With
    /// you list and on the Agents page the sidebar has the field and the page none of its own, the field saying what it
    /// searches; the ⌕ puts the cursor in it, as Ctrl+K does, and opens no box over the page. Over pr's list his Enter
    /// leaves him there, the hits of pr's threads in the list's place. With the sidebar folded away the list's own
    /// field is back on top of it, and the ⌕ opens the box over a page with no list.
    #[test]
    fn one_field_in_the_sidebar_on_every_page_searches_the_list_under_it() {
        let (_held, mut rt) = window();
        let mut rows: Option<f32> = None;
        for (sel, says) in [(Sel::Tickets, "Search in research"), (Sel::Agent("pr".into()), "Search With you"), (Sel::Agents, "Search in research")] {
            rt.update(|s, _| s.sel = sel.clone());
            frames(&mut rt);
            assert!(rt.find("find-bar-sidebar").is_some(), "{sel:?}: the sidebar's field");
            assert!(rt.find("find-bar").is_none() && rt.find("find-bar-pr").is_none(), "{sel:?}: none on the page");
            // what it says it searches: its placeholder, which names the field's editor (rune-kit `name_field`)
            let named: Vec<String> = rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("/find-bar-sidebar/")).filter_map(|e| e.label).collect();
            assert!(named.iter().any(|l| l == says), "{sel:?}: the field says {says:?}: {named:?}");
            // the sidebar's rows stand where they stood on Threads, and the cursor put in the field moves none (the walk's 1)
            let at = y(&rt, "projects");
            assert_eq!(at, *rows.get_or_insert(at), "{sel:?}: Projects where it stood on Threads");
            press(&mut rt, "find-mark");
            assert_eq!(y(&rt, "projects"), at, "{sel:?}: the ⌕ moves no row of the sidebar");
            assert!(!rt.state.tickets.find.over && rt.find("find-over").is_none(), "{sel:?}: the ⌕ opens no box over the page");
            rt.key(KeyEvent { key: rune::Key::Text("x".into()), mods: Default::default() });
            rt.frame(0.016);
            assert_eq!(rt.state.tickets.find.q.text, "x", "{sel:?}: the ⌕ put the cursor in the sidebar's field");
            rt.update(|s, _| s.tickets.find.q = TextEdit::default());
            rt.key(KeyEvent { key: rune::Key::Escape, mods: Default::default() });
            frames(&mut rt);
        }
        rt.update(|s, _| s.sel = Sel::Agent("pr".into()));
        frames(&mut rt);
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer(&mut rt, "deep");
        rt.key(KeyEvent { key: rune::Key::Enter, mods: Default::default() });
        frames(&mut rt);
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "Enter over pr's list leaves him there");
        assert!(rt.find("hit-N-30.p1").is_some() && rt.find("hit-N-99.p1").is_none(), "the hits of pr's threads in the list's place");
        rt.update(|s, _| {
            s.tickets.find.q = TextEdit::default();
            s.sidebar.set_open(false);
        });
        rt.invalidate();
        frames(&mut rt);
        assert!(rt.find("find-bar-sidebar").is_none() && rt.find("find-bar-pr").is_some(), "folded away: the list's own field");
        rt.update(|s, _| s.sel = Sel::Agents);
        frames(&mut rt);
        press(&mut rt, "find-mark");
        assert!(rt.state.tickets.find.over && rt.find("find-over").is_some(), "folded away: the ⌕ opens the box over the Agents page");
    }

    /// The empty field with the cursor in it says, faint, under it, what Enter does with an id or a name; a letter
    /// typed takes the line away (consult-lead, N-412). Under the sidebar's field the words stay within the field: on
    /// one line at the sidebar's width as he first has it, on two in a narrower sidebar, never cut at its edge (they
    /// ran past it, "…opens on Ent", e8-side's still); under the list's own field, the sidebar folded, the whole line.
    #[test]
    fn the_empty_field_with_the_cursor_says_what_enter_does_with_an_id() {
        let (_held, mut rt) = window();
        let small = rune_kit::Kit::neutral(true).ty.small;
        let line = rune::text::measure("Ag", &[], small, None).1;
        let el = |rt: &rune::runtime::Runtime<App>, key: &str| rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key));
        assert!(rt.find("find-hint").is_none(), "not while the cursor is elsewhere");
        press(&mut rt, INPUT);
        frames(&mut rt);
        for (wide, lines) in [(220.0, 1.0), (170.0, 2.0)] {
            rt.update(move |s: &mut App, _| s.sidebar.set_width(wide));
            rt.invalidate();
            frames(&mut rt);
            let hint = el(&rt, "find-hint").unwrap_or_else(|| panic!("under the sidebar's field with the cursor in it at {wide}"));
            let field = el(&rt, INPUT).expect("the field").rect;
            let words = hint.text.clone().unwrap_or_default();
            assert!(hint.path.contains("/find-bar-sidebar/"), "the sidebar's field at {wide}: {}", hint.path);
            assert!(hint.rect[0] + hint.rect[2] <= field[0] + field[2] + 0.5, "within the field at {wide}: {words} at {:?}, the field at {field:?}", hint.rect);
            let high = rune::text::measure(&words, &[], small, Some(hint.rect[2])).1;
            assert!(high <= hint.rect[3] + 0.5 && hint.rect[3] < line * (lines + 0.5), "{words} whole on {lines} line(s) at {wide}: {high} high, at {:?}", hint.rect);
        }
        rt.update(|s, _| s.sidebar.set_open(false));
        rt.invalidate();
        frames(&mut rt);
        press(&mut rt, INPUT);
        frames(&mut rt);
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(HINT)), "under the list's own field, the sidebar folded");
        rt.key(KeyEvent { key: rune::Key::Text("x".into()), mods: Default::default() });
        frames(&mut rt);
        assert!(rt.find("find-hint").is_none(), "gone once he types");
    }

    /// coordd's answer with more hits than the window holds: N-10.p1 to N-49.p1 (07:10 to 07:49), oldest first.
    fn answer_many(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        let hit = |t: u64| serde_json::json!({"id": format!("N-{t}.p1"), "t": t, "post": 1, "at": format!("2026-11-19T07:{t:02}:00Z"), "by": "owner", "words": format!("the {words} in note {t}")}).to_string();
        let out = (10..50).map(hit).collect::<Vec<_>>().join("\n");
        rt.update(|s, _| came(s, words.into(), true, out));
        frames(rt);
    }

    /// Whether KEY is in sight: drawn under the field, inside the window (900 points high).
    fn in_sight(rt: &rune::runtime::Runtime<App>, key: &str) -> bool {
        rt.find(key).is_some_and(|(_, y)| y > self::y(rt, INPUT) && y < 900.0)
    }

    /// Enter in the sidebar's field, before any pause: he is on the Threads page at once and the words are asked; their
    /// answer draws the hits there, newest first (consult-5, N-412: Enter there drew nothing; consult-lead: Enter moves
    /// him, before any answer).
    #[test]
    fn enter_in_the_sidebars_field_opens_the_hits_on_the_threads_page() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.sel = Sel::Agents);
        frames(&mut rt);
        press(&mut rt, INPUT);
        // the letters and Enter with no frame between: no frame has seen the words, so no pause can have asked them
        for c in "deep".chars() {
            rt.key(KeyEvent { key: rune::Key::Text(c.to_string()), mods: Default::default() });
        }
        rt.key(KeyEvent { key: rune::Key::Enter, mods: Default::default() });
        rt.frame(0.016);
        assert_eq!(rt.state.tickets.find.searched, "deep", "asked on Enter");
        assert_eq!(rt.state.sel, Sel::Tickets, "Enter takes him to the Threads page at once, before any answer");
        answer(&mut rt, "deep");
        assert_eq!(rt.state.sel, Sel::Tickets, "the hits on the Threads page");
        assert!(y(&rt, "hit-N-99.p1") < y(&rt, "hit-N-30.p1") && y(&rt, "hit-N-30.p1") < y(&rt, "hit-N-12.p1"), "newest first");
    }

    /// What Enter opened leaves the field empty (docket-edge8's walk of 73, N-1446 p2353: "N-1446", Enter, then
    /// "docket-core" typed read "N-1446docket-core"): an id opens its thread and an agent's name its page, each from
    /// an empty field; an id that opens nothing stays in it, to be put right.
    #[test]
    fn what_enter_opened_leaves_the_field_empty_for_the_next_words() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.sel = Sel::Agents);
        frames(&mut rt);
        let enter = |rt: &mut rune::runtime::Runtime<App>, words: &str| {
            press(rt, INPUT);
            type_in(rt, words);
            rt.key(KeyEvent { key: rune::Key::Enter, mods: Default::default() });
            frames(rt);
        };
        enter(&mut rt, "N-30");
        assert_eq!(rt.state.tickets.shown, Some(30), "N-30 opens");
        assert_eq!(rt.state.tickets.find.q.text, "", "and leaves the field empty");
        enter(&mut rt, "pr");
        assert_eq!(rt.state.sel, Sel::Agent("pr".into()), "pr's page: its name not added to N-30");
        assert_eq!(rt.state.tickets.find.q.text, "", "and the field empty again");
        enter(&mut rt, "N-7");
        assert_eq!(rt.state.tickets.find.q.text, "N-7", "an id that opens nothing stays");
    }

    /// The field changed while its words were out (consult-5, N-412): `deep` asked, then `er` typed, so `deep`'s
    /// answer draws nothing and `deeper`'s draws; the field emptied while `deep` is out, so its answer draws nothing
    /// and the list stays where he scrolled it.
    #[test]
    fn an_answer_to_words_no_longer_in_the_field_draws_nothing() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.tickets.by_time = true);
        frames(&mut rt);
        let was = scrolled(&mut rt, "ticket-list");
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        type_in(&mut rt, "er");
        answer(&mut rt, "deep");
        assert!(rt.state.tickets.find.shown.is_none() && rt.find("hit-N-30.p1").is_none(), "deep's answer draws nothing");
        searched(&mut rt, "deeper");
        answer(&mut rt, "deeper");
        assert!(rt.find("hit-N-30.p1").is_some(), "deeper's answer draws");
        cleared(&mut rt, "ticket-list", was);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        cleared(&mut rt, "ticket-list", was);
        answer(&mut rt, "deep");
        assert!(rt.state.tickets.find.shown.is_none() && rt.find("hit-N-30.p1").is_none(), "the emptied field's answer draws nothing");
        assert!(rt.find("ticket-list").is_some() && (offset(&rt, "ticket-list") - was).abs() < 1.0, "the list stays at {was}: {}", offset(&rt, "ticket-list"));
    }

    /// A search from the sidebar's field answered once he has gone on to another page: the answer does not take him
    /// from that page (consult-5, N-412: chosen, he went there since); the sidebar's field keeps his words, and the hits
    /// are on the Threads page when he opens it.
    #[test]
    fn a_sidebar_search_answered_after_he_moved_on_leaves_him_where_he_went() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.sel = Sel::Agents);
        frames(&mut rt);
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        rt.update(|s, _| s.sel = Sel::Kinds);
        frames(&mut rt);
        answer(&mut rt, "deep");
        assert_eq!(rt.state.sel, Sel::Kinds, "he stays on the page he went to");
        assert!(rt.find("find-bar-sidebar").is_some() && rt.state.tickets.find.q.text == "deep", "the sidebar's field keeps his words");
        rt.update(|s, _| s.sel = Sel::Tickets);
        frames(&mut rt);
        assert!(y(&rt, "hit-N-99.p1") < y(&rt, "hit-N-30.p1"), "the hits on the Threads page, newest first");
    }

    /// More hits than the window holds, after the list was scrolled: they open at their top, the newest in sight under
    /// the field; and so do the next words' hits after the hits before them were scrolled (consult-5, N-412).
    #[test]
    fn the_newest_hit_is_in_sight_whatever_was_scrolled_before() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.tickets.by_time = true);
        frames(&mut rt);
        scrolled(&mut rt, "ticket-list");
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        answer_many(&mut rt, "deep");
        assert!(in_sight(&rt, "hit-N-49.p1"), "the newest in sight after the list's scroll: {:?}", rt.find("hit-N-49.p1"));
        let hits = format!("ticket-hits-{}", rt.state.tickets.find.hits_n);
        let (hx, hy) = rt.find(&hits).unwrap_or_else(|| panic!("{hits} is drawn"));
        for _ in 0..6 {
            rt.wheel(hx, hy, -120.0);
            rt.frame(0.016);
        }
        frames(&mut rt);
        assert!(!in_sight(&rt, "hit-N-49.p1"), "the hits scrolled past the newest: {:?}", rt.find("hit-N-49.p1"));
        type_in(&mut rt, "er");
        searched(&mut rt, "deeper");
        answer_many(&mut rt, "deeper");
        assert!(in_sight(&rt, "hit-N-49.p1"), "the newest in sight after the hits' scroll: {:?}", rt.find("hit-N-49.p1"));
    }

    /// Split on pr's With you shows pr twice: one field on the screen, the sidebar's (N-1446 p2121), in neither pane.
    /// With the sidebar folded away it is in the focused pane's list; once the other pane is focused, the field is there.
    #[test]
    fn a_split_has_one_field_in_the_focused_pane() {
        let (_held, mut rt) = window();
        rt.update(|s, _| {
            s.sel = Sel::Agent("pr".into());
            s.split();
        });
        frames(&mut rt);
        let fields = |rt: &rune::runtime::Runtime<App>| rt.elements().iter().filter(|e| !e.exiting && e.key.as_deref() == Some(INPUT)).count();
        let within = |rt: &rune::runtime::Runtime<App>, pane: &str| {
            let (x, y) = rt.find(INPUT).expect("the field is drawn");
            rt.contains(pane, x, y)
        };
        assert_eq!(rt.state.panes.len(), 2);
        assert_eq!(fields(&rt), 1, "one field on the screen");
        assert!(rt.find("find-bar-sidebar").is_some() && !within(&rt, "pane-0") && !within(&rt, "pane-1"), "the sidebar's, in neither pane");
        rt.update(|s, _| s.sidebar.set_open(false));
        rt.invalidate();
        frames(&mut rt);
        assert_eq!(fields(&rt), 1, "folded away: one field on the screen");
        assert!(within(&rt, &format!("pane-{}", rt.state.focus)), "in the focused pane");
        let other = 1 - rt.state.focus;
        rt.update(|s, _| s.focus_pane(other));
        frames(&mut rt);
        assert_eq!(fields(&rt), 1, "one field on the screen");
        assert!(within(&rt, &format!("pane-{other}")), "in the pane focused now");
    }

    /// WORDS on their way to coordd, as lib.rs's `request_in` leaves an ask (this window has no pipe).
    fn on_way(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        let q = words.to_string();
        rt.update(|s, _| {
            s.projects[0].pending.insert(1, crate::Pending::Find { q });
        });
    }

    /// coordd's answer to WORDS on their way: the ask taken off as lib.rs takes it, then [`answer`].
    fn answered(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        rt.update(|s, _| {
            s.projects[0].pending.remove(&1);
        });
        answer(rt, words);
    }

    /// One search by words on its way at a time (his window on build 27: asked at each pause, the searches waited for
    /// one another in coordd, and the answer to his last words came 12.7 s after he stopped). `deep` is out; he types
    /// `er` and pauses: `deeper` is not asked while `deep` is out, and the field says it is searching. `deep`'s answer
    /// draws nothing, and `deeper` goes at once, with no second pause; its answer draws.
    #[test]
    fn one_search_is_on_its_way_at_a_time_and_the_words_typed_meanwhile_go_when_it_is_answered() {
        let (_held, mut rt) = window();
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        searched(&mut rt, "deep");
        on_way(&mut rt, "deep");
        frames(&mut rt);
        assert!(rt.find("find-searching").is_some(), "the field says its words are being searched");
        type_in(&mut rt, "er");
        std::thread::sleep(PAUSE + std::time::Duration::from_millis(50));
        frames(&mut rt);
        assert_eq!(rt.state.tickets.find.searched, "deep", "deeper waits while deep is on its way");
        assert!(rt.find("find-searching").is_some(), "still searching");
        answered(&mut rt, "deep");
        assert_eq!(rt.state.tickets.find.searched, "deeper", "asked the moment deep's answer came");
        assert!(rt.state.tickets.find.shown.is_none() && rt.find("hit-N-30.p1").is_none(), "deep's answer draws nothing");
        on_way(&mut rt, "deeper");
        answered(&mut rt, "deeper");
        assert!(rt.find("hit-N-30.p1").is_some(), "deeper's answer draws");
        assert!(rt.find("find-searching").is_none(), "the field no longer searching");
    }

    /// Titles only (the owner, N-1104 post 496), the box beside the field: each key looks in the titles of the threads
    /// the window holds at once, with no pause and nothing waited for, and the hits are the threads. No title holding
    /// the words says where else to look. Taken off, the words in the field are searched by words once more.
    #[test]
    fn titles_only_finds_threads_by_their_titles_at_each_key() {
        let (_held, mut rt) = window();
        rt.update(|s, _| s.projects[0].live.tickets.get_mut(&30).unwrap().title = "Dark mode switch".into());
        press(&mut rt, TITLES);
        assert!(rt.state.tickets.find.titles, "the box is ticked");
        press(&mut rt, INPUT);
        type_in(&mut rt, "da");
        assert_eq!(rt.state.tickets.find.searched, "da", "looked for at the key, no pause waited for");
        frames(&mut rt);
        assert!(rt.find("hit-N-30").is_some() && rt.find("hit-N-12").is_none(), "the one thread whose title holds it");
        type_in(&mut rt, "rk sw");
        frames(&mut rt);
        assert!(rt.find("hit-N-30").is_some(), "every word in its title: dark sw");
        assert!(rt.find("find-searching").is_none(), "nothing is waited for");
        type_in(&mut rt, "q");
        frames(&mut rt);
        assert!(rt.find("hit-N-30").is_none(), "no title holds dark swq");
        assert!(rt.elements().iter().any(|e| e.text.as_deref().is_some_and(|t| t.starts_with("No title matches"))), "and it says so");
        press(&mut rt, TITLES);
        assert!(!rt.state.tickets.find.titles, "the box taken off");
        assert!(rt.find("find-searching").is_some(), "the titles' hits are not the answer to the words");
        searched(&mut rt, "dark swq");
    }

    /// The pause asks one wake, not one each frame (his frames.jsonl on build 27: 13 to 22 wakes a second from
    /// `find.rs:287` while he typed): the frames inside one pause ask nothing more; a key starts a pause of its own.
    #[test]
    fn the_pause_asks_one_wake_not_one_each_frame() {
        let (_held, mut rt) = window();
        press(&mut rt, INPUT);
        type_in(&mut rt, "deep");
        let first = rt.state.tickets.find.woken.expect("the pause asked its wake");
        for _ in 0..3 {
            rt.frame(0.016);
        }
        assert_eq!(rt.state.tickets.find.woken, Some(first), "the frames of the pause ask nothing more");
        type_in(&mut rt, "e");
        assert!(rt.state.tickets.find.woken.is_some_and(|w| w > first), "the key's own pause");
    }

    #[test]
    fn an_id_is_told_from_words() {
        assert_eq!(place("N-221"), Some(("N-221".into(), None)));
        assert_eq!(place("n-221.p5"), Some(("N-221".into(), Some(5))));
        assert_eq!(place("Q-56"), Some(("Q-56".into(), None)));
        assert_eq!(place("the parser"), None);
        assert_eq!(place("docket-edge"), None);
        assert_eq!(place("N-221.s3"), None);
    }
}
