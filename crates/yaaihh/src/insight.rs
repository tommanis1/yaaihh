//! Insights into his data, as lenses (S6; his p646: "should be in 3.0 as a insight i can get into my data"; proto-lead
//! N-1446 p388). An insight is a lens (lens.rs `Filter`, `Filter::insight`): its rows asked of coordd
//! (`Request::Insight`), its numbers drawn from them by docket-machine's one function (`docket_machine::insight`), and
//! each number opening what it counts, each zooming to its lines (Events, then Record: `lens::zoom`). Nothing here
//! reads a file, asks a model or the gateway.
//!
//! Insight (3), **open → done** (his N-1002, N-1020): every thread and item of the outline, from when it was opened to
//! when it ended (rows read under coordd's lock from its machine), but those moved onto another thread. His (he or his
//! message opened them) are four tiles:
//! - **Opened**: every one, newest first;
//! - **Ended**: those that ended (closed done, dropped, kept or failed, and owing him nothing; when it still owed him,
//!   it ended when it stopped), the completion rate (ended over opened) and the median time from open to end, the
//!   latest end first;
//! - **Open now**: the others, the longest open first; a closed thread that still owes him is open, as in the outline;
//! - **Open for days**: of those, the ones open a day or more now, the longest first. Shown when the lens opens.
//!
//! The threads opened by agents and programs (an agent, a clock, a `when`, an agent's event: not he or his message) are
//! a row of the same four under them, not in his completion: an agents' thread ended when its agents ended it
//! (proto-lead N-1446 p473, 2; p499, 1; p581).
//!
//! A thread's row zooms to the Events depth at its lines: an ended one's from its last post, where its end follows,
//! any other's from its opening (consult-3 p451, 2). A row opened by an agent or a program names it after its state,
//! from the row's `by` ([`by_words`]: "by dev", "by a clock, every 1 h"; proto-window N-1446 p646).
//!
//! Its third row, **your asks** (his N-1002 p2: "track threads initial time -> completion and estimate "utility"";
//! paper-research N-1446 p1786), from a coordd at ASKS_PROTOCOL: each post of his the ledger counts as asking an agent
//! (`Insight::Asks`, docket-machine `insight::Ask`), a row per ask with its thread as its second label, since his
//! asks share threads that never end. Five tiles: **Asked**; **Nobody moves**, the agents' to move, shown when the lens
//! opens; **Answered in words**; **In your window**, whose line says in red how many wait for his word, the only red;
//! **Signed off**, with his words' counts. An ask's row says when he asked, how long it has been open (to his sign-off
//! once signed), where it stands (the graph page's words: lanes.rs `leg_words`), what it took (the requests sent for
//! it and their tokens; those for posts naming it beside, never added in; his posts on it once it was in his window),
//! and his word of worth, or "waits for you" in red when it is in his window and not signed off. Its `⋯` opens his
//! three words, Worth it, Fine and Not worth it, in the row's place, and Take back once signed: one press is one
//! `Request::SignOff`, and the asks are asked again once coordd has taken it. The row zooms to his post's lines at the
//! Events depth. The counts are the ledger's asks as coordd gives them; nothing here picks among them (p1786, 3).
//!
//! Insight (2), **agents per topic** (his N-1104.p646; proto-lead N-1446 p653), from (3)'s rows: a topic is a thread
//! with the threads and items under it, or an item. Two tiles, **Threads** and **Items**, each with the median number
//! of agents and the median time from open to done; a press lists their topics, the most agents first, each with its
//! agents by name and its time beside them (Threads when the lens opens). A topic lists its lines, each with its own
//! agents (given anything on it; an item's starters too), whose names are the topic's (proto-lead N-1446 p671); a line
//! zooms to the Events depth, as (3)'s rows do, where its turns show.
//!
//! Insight (1), **work and talk** (his p640, p641): every turn the record holds, as its `turn_ended` line says what it
//! did (rows read off coordd's lock from the record's file). Its tiles: **Work** turns (and their share of the turns
//! known), **Talk** turns, **Not known**; then the turns by agent, by model and by hour, each row a count of its own.
//! A press on a tile or a row lists the turns it counts, the newest first (Work when the lens opens); a turn's row
//! zooms to the Events depth at the post it was given, where the turn shows.

use crate::lens::{Depth, Filter, Lens};
use crate::App;
use coord_types::Insight;
use docket_machine::insight::{Ask, Asks, Counts, Did, Tally, Thread, Topic, Topics, Turn, WorkTalk, ask_took, asks, open_to_done, per_topic, took, work_talk};
use rune::{El, GrowthSpec, ScrollSpec, TextStyle, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

/// What insight (3)'s page says it counts (the still shows this line: proto-lead p388, "its doc says what it counts").
const COUNTS: &str = "Threads you opened, from open to done.";

/// What insight (1)'s page says it counts: what a work turn is (plan.md S6).
const COUNTS_TURNS: &str = "Each agent's turns, by work or talk.";

/// What insight (2)'s page says it counts.
const COUNTS_TOPICS: &str = "Agents on each topic, and its time to done.";

/// What Open → done's asks row says it counts: what an ask is, the ledger's rule, said and not filtered here
/// (paper-research N-1446 p1786, 3).
const COUNTS_ASKS: &str = "Your asks, from asked to signed off.";

/// Rows asked of coordd, or why there are none, and when they came (seconds since the epoch).
type Rows<T> = (Result<Vec<T>, String>, i64);

/// Pages as counted, by project and the node (the number open) each was counted for: the page and a pane beside it
/// (lens.rs `apart`) open at two numbers keep one each (proto-window N-1446 p626).
type Kept<P> = RefCell<HashMap<(usize, Option<String>), Arc<P>>>;

/// The insights' rows asked of coordd, by project: the lines of the outline (insights (3) and (2)), insight (1)'s turns;
/// and each project's pages as counted from them, one for each number open ([`OpenToDonePage`], [`TopicsPage`],
/// [`WorkTalkPage`]). A build draws a page as counted and counts nothing, so it costs only the rows in view; a number
/// first open counts it, and its rows coming or forgotten count every number again (proto-lead N-1446 p582:
/// `work_talk` ran over every turn at each build, 11,737 on research's record).
#[derive(Default)]
pub struct Insights {
    pub threads: HashMap<usize, Rows<Thread>>,
    pub turns: HashMap<usize, Rows<Turn>>,
    /// His asks, Open → done's third row (`Insight::Asks`, from ASKS_PROTOCOL; paper-research N-1446 p1786).
    pub asks: HashMap<usize, Rows<Ask>>,
    open_to_done: Kept<OpenToDonePage>,
    per_topic: Kept<TopicsPage>,
    work_talk: Kept<WorkTalkPage>,
    asks_page: Kept<AsksPage>,
    /// The ask whose `⋯` is open, by project and post (`N-1104.p113`): his three words in its row's place.
    pub ask_more: Option<(usize, String)>,
    /// His presses on their way, by project and post: the word pressed (`none` takes it back), shown in its row until
    /// the asks after it come.
    pub signing: HashMap<(usize, String), String>,
    /// How many times a page was counted.
    counted: Cell<u64>,
}

impl Insights {
    /// Project PI's page in KEPT, as counted for node OPEN: the one kept for it, else COUNT's, kept from now on.
    fn page<P>(&self, kept: &Kept<P>, pi: usize, open: Option<&str>, count: impl FnOnce() -> P) -> Arc<P> {
        let counted = || {
            self.counted.set(self.counted.get() + 1);
            Arc::new(count())
        };
        kept.borrow_mut().entry((pi, open.map(String::from))).or_insert_with(counted).clone()
    }

    /// Whether project PI's rows of WHICH are held.
    fn held(&self, pi: usize, which: Insight) -> bool {
        match which {
            Insight::OpenToDone => self.threads.contains_key(&pi),
            Insight::WorkTalk => self.turns.contains_key(&pi),
            Insight::Asks => self.asks.contains_key(&pi),
        }
    }

    /// Project PI's rows of WHICH forgotten, to be asked again, and its pages as counted from them, at every number.
    fn forget(&mut self, pi: usize, which: Insight) {
        match which {
            Insight::OpenToDone => {
                self.threads.remove(&pi);
                self.open_to_done.get_mut().retain(|(p, _), _| *p != pi);
                self.per_topic.get_mut().retain(|(p, _), _| *p != pi);
            }
            Insight::WorkTalk => {
                self.turns.remove(&pi);
                self.work_talk.get_mut().retain(|(p, _), _| *p != pi);
            }
            Insight::Asks => {
                self.asks.remove(&pi);
                self.asks_page.get_mut().retain(|(p, _), _| *p != pi);
            }
        }
    }
}

/// A tile as drawn: the node a press opens (WORD), the COUNT, its LABEL and the line UNDER it; OPEN when its number is
/// the one listed; RED when its line says what waits for him (his asks in his window he has not signed off: the only
/// red, paper-research N-1446 p1786, 2).
struct Tile {
    word: String,
    count: usize,
    label: String,
    under: String,
    open: bool,
    red: bool,
}

/// Insight (3)'s page as counted ([`count_open_to_done`]): his four tiles, the four of the threads agents and programs
/// opened, the heading, and the threads the number open lists, each with its seconds.
struct OpenToDonePage {
    his: Vec<Tile>,
    others: Vec<Tile>,
    head: String,
    listed: Arc<Vec<(Thread, Option<i64>)>>,
}

/// A row of insight (1)'s tables as drawn: its count C, its NAME, its LINE (work, talk, not known) and SHARE; OPEN when
/// it is the count listed.
struct TallyRow {
    c: Count,
    name: String,
    line: String,
    share: String,
    open: bool,
}

/// Open → done's asks row as counted ([`count_asks`]): its five tiles; OPEN when one of its numbers is the node, then
/// its heading and the asks it lists; AT, when its rows came, the time an open ask is open to.
struct AsksPage {
    tiles: Vec<Tile>,
    open: bool,
    head: String,
    listed: Arc<Vec<Ask>>,
    at: i64,
}

/// Insight (1)'s page as counted ([`count_work_talk`]): its tiles, its tables (by agent, model and hour), the heading,
/// and the turns the count open lists.
struct WorkTalkPage {
    tiles: Vec<Tile>,
    tables: [(&'static str, Vec<TallyRow>); 3],
    head: String,
    listed: Arc<Vec<Turn>>,
}

/// Insight (2)'s page as counted ([`count_per_topic`]): its two tiles, the heading, and what the node open lists.
struct TopicsPage {
    tiles: Vec<Tile>,
    head: String,
    listed: Arc<Vec<Listed>>,
}

/// A row insight (2) lists: a topic, with its agents by name, how many lines it holds and its seconds (a press lists
/// its lines); or one of a topic's lines, with its seconds (a press zooms to its lines at the Events depth).
enum Listed {
    Topic { line: Thread, agents: Vec<String>, lines: usize, secs: Option<i64> },
    Line(Thread, Option<i64>),
}

/// What insight (2) lists under its tiles: the threads' topics, the items', or the lines of the topic at an ID. Its
/// word is the lens's node (`threads`, `items`, `topic:N-2`), so a pane or a pin keeps it; no node, or a word this
/// window does not know, is Threads.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Listing {
    Threads,
    Items,
    Topic(String),
}

impl Listing {
    fn word(&self) -> String {
        match self {
            Listing::Threads => "threads".into(),
            Listing::Items => "items".into(),
            Listing::Topic(id) => format!("topic:{id}"),
        }
    }

    fn of(at: Option<&str>) -> Listing {
        match at.unwrap_or_default().split_once(':') {
            Some(("topic", id)) => Listing::Topic(id.into()),
            None if at == Some("items") => Listing::Items,
            _ => Listing::Threads,
        }
    }
}

/// Whose threads a number of insight (3) counts: his (he or his message opened them: the four tiles) or the others'
/// (an agent or a program opened them: the row under them, "Opened by agents and programs").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Whose {
    His,
    Agents,
}

/// A number of insight (3), the threads it counts listed under the tiles: its word, with `agents-` before it for the
/// agents' row, is the lens's node (`Lens.at`), so a pane or a pin keeps it. No node, or a word this window does not
/// know (a newer window's), is Open for days.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Number {
    Opened,
    Ended,
    Open,
    Days,
}

impl Number {
    const ALL: [Number; 4] = [Number::Opened, Number::Ended, Number::Open, Number::Days];

    fn word(self) -> &'static str {
        match self {
            Number::Opened => "opened",
            Number::Ended => "ended",
            Number::Open => "open",
            Number::Days => "days",
        }
    }

    /// The word of WHOSE number (the lens's node).
    fn node(self, whose: Whose) -> String {
        match whose {
            Whose::His => self.word().to_string(),
            Whose::Agents => format!("agents-{}", self.word()),
        }
    }

    /// Whose number the node AT is.
    fn of(at: Option<&str>) -> (Whose, Number) {
        let (whose, word) = match at.and_then(|a| a.strip_prefix("agents-")) {
            Some(w) => (Whose::Agents, Some(w)),
            None => (Whose::His, at),
        };
        (whose, Number::ALL.into_iter().find(|n| Some(n.word()) == word).unwrap_or(Number::Days))
    }

    /// The threads it counts in N, each with the seconds it took or has been open, in the order its doc gives.
    fn threads<'a>(self, n: &Counts<'a>) -> Vec<(&'a Thread, Option<i64>)> {
        match self {
            Number::Opened => n.opened.clone(),
            Number::Ended => n.ended.clone(),
            Number::Open => n.open.clone(),
            Number::Days => n.days.iter().map(|(x, s)| (*x, Some(*s))).collect(),
        }
    }
}

/// A number of his asks, Open → done's third row (paper-research N-1446 p1786): the asks it counts, listed under the
/// tiles. Its word is the lens's node (`asks-stuck`), so a pane or a pin keeps it; an `asks-` word this window does not
/// know (a newer window's) is Nobody moves. No node is Nobody moves where coordd gives his asks, else Open for days, as
/// before ([`Number`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AskNumber {
    Asked,
    Stuck,
    Words,
    Window,
    Signed,
}

impl AskNumber {
    const ALL: [AskNumber; 5] = [AskNumber::Asked, AskNumber::Stuck, AskNumber::Words, AskNumber::Window, AskNumber::Signed];

    fn word(self) -> &'static str {
        match self {
            AskNumber::Asked => "asks-asked",
            AskNumber::Stuck => "asks-stuck",
            AskNumber::Words => "asks-words",
            AskNumber::Window => "asks-window",
            AskNumber::Signed => "asks-signed",
        }
    }

    /// The number the node AT is; none when it is one of the threads'.
    fn of(at: Option<&str>) -> Option<AskNumber> {
        let at = at.filter(|a| a.starts_with("asks-"))?;
        Some(AskNumber::ALL.into_iter().find(|n| n.word() == at).unwrap_or(AskNumber::Stuck))
    }

    /// The asks it counts in N, in the order `insight::Asks` gives.
    fn asks<'a>(self, n: &Asks<'a>) -> Vec<&'a Ask> {
        match self {
            AskNumber::Asked => n.asked.clone(),
            AskNumber::Stuck => n.stuck.clone(),
            AskNumber::Words => n.words.clone(),
            AskNumber::Window => n.window.clone(),
            AskNumber::Signed => n.signed.clone(),
        }
    }

    /// The heading over its N asks.
    fn head(self, n: usize) -> String {
        let asks = many(n, "ask");
        match self {
            AskNumber::Asked => format!("Asked · {asks}, the newest first"),
            AskNumber::Stuck => format!("Nobody moves · {asks}, the longest waiting first"),
            AskNumber::Words => format!("Answered in words · {asks}"),
            AskNumber::Window => format!("In your window · {asks}, the latest first"),
            AskNumber::Signed => format!("Signed off · {asks}, the latest sign-off first"),
        }
    }
}

/// A count of insight (1), the turns it counts listed under it: a tile's (the turns that worked, talked, or whose work
/// is not known) or a row's (an agent's, a model's, an hour's, every turn of it). Its word is the lens's node
/// (`work`, `talk`, `not-known`, `agent:NAME`, `model:NAME`, `hour:2026-11-20T18`), so a pane or a pin keeps it; no
/// node, or a word this window does not know, is Work.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Count {
    Did(Did),
    Agent(String),
    Model(String),
    Hour(String),
}

impl Count {
    fn word(&self) -> String {
        match self {
            Count::Did(Did::Work) => "work".into(),
            Count::Did(Did::Talk) => "talk".into(),
            Count::Did(Did::NotKnown) => "not-known".into(),
            Count::Agent(a) => format!("agent:{a}"),
            Count::Model(m) => format!("model:{m}"),
            Count::Hour(h) => format!("hour:{h}"),
        }
    }

    fn of(at: Option<&str>) -> Count {
        match at.unwrap_or_default().split_once(':') {
            None if at == Some("talk") => Count::Did(Did::Talk),
            None if at == Some("not-known") => Count::Did(Did::NotKnown),
            Some(("agent", a)) => Count::Agent(a.into()),
            Some(("model", m)) => Count::Model(m.into()),
            Some(("hour", h)) => Count::Hour(h.into()),
            _ => Count::Did(Did::Work),
        }
    }

    /// The turns it counts in N, the newest first.
    fn turns<'a>(&self, n: &WorkTalk<'a>) -> Vec<&'a Turn> {
        let every = |rows: &[Tally<'a>], label: &str| {
            let mut v: Vec<&'a Turn> = rows.iter().filter(|t| t.label == label).flat_map(|t| t.work.iter().chain(&t.talk).chain(&t.not_known).copied()).collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.line));
            v
        };
        match self {
            Count::Did(Did::Work) => n.all.work.clone(),
            Count::Did(Did::Talk) => n.all.talk.clone(),
            Count::Did(Did::NotKnown) => n.all.not_known.clone(),
            Count::Agent(a) => every(&n.agents, a),
            Count::Model(m) => every(&n.models, m),
            Count::Hour(h) => every(&n.hours, h),
        }
    }

    /// The heading over its N turns.
    fn head(&self, n: usize) -> String {
        let turns = if n == 1 { "1 turn".to_string() } else { format!("{n} turns") };
        match self {
            Count::Did(Did::Work) => format!("Work · {turns}, the newest first"),
            Count::Did(Did::Talk) => format!("Talk · {turns}, the newest first"),
            Count::Did(Did::NotKnown) => format!("Not known · {turns}, the newest first"),
            Count::Agent(a) => format!("{a} · {turns}, the newest first"),
            Count::Model(m) => format!("{} · {turns}, the newest first", model_name(m)),
            Count::Hour(h) => format!("{} · {turns}, the newest first", hour_name(h)),
        }
    }
}

/// Insight WHICH's rows asked of project PI's coordd, unless held or on their way; a coordd before `INSIGHT_PROTOCOL` is
/// not asked (the page says so), nor for his asks one before `ASKS_PROTOCOL` (Open → done is as it was).
pub fn ask(s: &mut App, pi: usize, which: Insight) {
    let Some(p) = s.projects.get(pi) else { return };
    let from = if which == Insight::Asks { coord_types::ASKS_PROTOCOL } else { coord_types::INSIGHT_PROTOCOL };
    if p.protocol < from || s.insights.held(pi, which) || p.pending.values().any(|w| matches!(w, crate::Pending::Insight(x) if *x == which)) {
        return;
    }
    s.request_in(pi, coord_types::Request::Insight { which }, crate::Pending::Insight(which));
}

/// coordd's answer for project PI's insight WHICH: its rows, one JSON line each, or why there are none; the page as
/// counted from the rows before is forgotten.
pub(crate) fn came(s: &mut App, pi: usize, which: Insight, ok: bool, out: String) {
    let at = chrono::Utc::now().timestamp();
    s.insights.forget(pi, which);
    match which {
        Insight::OpenToDone => {
            s.insights.threads.insert(pi, (rows(ok, &out), at));
        }
        Insight::WorkTalk => {
            s.insights.turns.insert(pi, (rows(ok, &out), at));
        }
        Insight::Asks => {
            s.insights.asks.insert(pi, (rows(ok, &out), at));
            // his presses are in these rows, or were refused before them
            s.insights.signing.retain(|(p, _), _| *p != pi);
        }
    }
}

/// His press on ask POST of project PI: his word WORTH (one of `docket_machine::WORTHS`; `none` takes it back) sent to
/// coordd (`Request::SignOff`), and said in its row until the asks after it come; its `⋯` closes.
fn sign_off(s: &mut App, pi: usize, post: &str, worth: &str) {
    s.insights.ask_more = None;
    let req = coord_types::Request::SignOff { post: post.to_string(), worth: worth.to_string() };
    if s.request_in(pi, req, crate::Pending::SignOff { post: post.to_string() }) {
        s.insights.signing.insert((pi, post.to_string()), worth.to_string());
    }
}

/// coordd's answer to his press on POST of project PI: taken, his asks are asked again, so the row says what the record
/// holds now; refused, the row says what it said before, and the notice why.
pub(crate) fn signed(s: &mut App, pi: usize, post: String, ok: bool, out: String) {
    if !ok {
        s.insights.signing.remove(&(pi, post));
        s.notice = Some(out);
        return;
    }
    // asked even when an ask is on its way already: that one may be from before the press
    s.request_in(pi, coord_types::Request::Insight { which: Insight::Asks }, crate::Pending::Insight(Insight::Asks));
}

/// OUT's JSON lines when coordd answered OK; else OUT is why there are none.
fn rows<T: serde::de::DeserializeOwned>(ok: bool, out: &str) -> Result<Vec<T>, String> {
    if !ok {
        return Err(out.to_string());
    }
    out.lines().map(|l| serde_json::from_str(l).map_err(|e| format!("insight failed: {e}"))).collect()
}

/// Lens L, an insight, of project PI: what it counts, its numbers, and what the number open counts. The page is one list
/// ([`in_view`]): its top (what it counts, the numbers, the list's heading) first, then the threads or turns the number
/// open counts.
pub fn page(s: &App, pi: usize, l: &Lens, k: &Kit) -> El {
    let (Some(p), Some(which)) = (s.projects.get(pi), l.filter.insight()) else { return col() };
    let faint = |w: String| text(w).font(k.ty.small).color(k.c.text_muted).wrap();
    let asked = l.filter.insights();
    let read = Button::new("Read again").variant(ButtonVariant::Ghost).help("Count again").on_press(move |s: &mut App| {
        for &w in &asked {
            s.insights.forget(pi, w);
            ask(s, pi, w);
        }
    });
    let head = row().w_full().gap(8.0).center_y().child(text(l.filter.name()).font(k.ty.ui_medium).color(k.c.text_base)).child(rune::el().grow(1.0)).child(read.build(k).key("insight-read-again"));
    let counts = match l.filter {
        Filter::OpenToDone => COUNTS,
        Filter::AgentsPerTopic => COUNTS_TOPICS,
        Filter::WorkTalk => COUNTS_TURNS,
        _ => return col(),
    };
    let top = col().w_full().gap(12.0).px(16.0).pt(12.0).pb(12.0).child(head).child(faint(counts.into()).key("insight-counts"));
    let counted = |at: i64| faint(format!("Counted over the whole record at {}.", crate::when::secs(at)));
    let (i, open) = (&s.insights, l.at.as_deref());
    // no node on Open → done: his asks' Nobody moves where coordd gives them, else Open for days (paper-research N-1446
    // p1786, 2: Nobody moves opens first)
    let gives = p.protocol >= coord_types::ASKS_PROTOCOL || i.asks.contains_key(&pi);
    let open = open.or_else(|| (l.filter == Filter::OpenToDone && gives).then_some(AskNumber::Stuck.word()));
    let list = match which {
        Insight::OpenToDone => match held(i.threads.get(&pi), p.protocol, k) {
            Ok((rows, at)) if l.filter == Filter::AgentsPerTopic => per_topic_body(top.child(counted(at)), &i.page(&i.per_topic, pi, open, || count_per_topic(rows, at, open)), at, l, k),
            Ok((rows, at)) => {
                let asks = asks_held(i, pi, p.protocol, open, k);
                open_to_done_body(top.child(counted(at)), &i.page(&i.open_to_done, pi, open, || count_open_to_done(rows, at, open)), asks, pi, at, l, k)
            }
            Err(why) => top.child(why),
        },
        Insight::WorkTalk => match held(i.turns.get(&pi), p.protocol, k) {
            Ok((rows, at)) => work_talk_body(top.child(counted(at)), &i.page(&i.work_talk, pi, open, || count_work_talk(rows, open)), at, l, k),
            Err(why) => top.child(why),
        },
        // no lens's own: Open → done's third row
        Insight::Asks => top,
    };
    col().w_full().h_full().min_h(0.0).key(format!("insight-{pi}-{}", l.filter.key())).scroll(ScrollSpec { stick_bottom: false }).child(list)
}

/// TOP, then a row for each of ROWS, keyed by KEY (its place in the list; the row drawn keeps its own) and drawn by
/// DRAW, as the page's one list: only the rows in view are
/// built (rune's `virtual_rows`, as a thread's are: threads.rs `rows_in_view`), since a count may list every turn of the
/// record (11,737 on research's, all Not known on the day this coordd goes in; proto-window N-1446 p545). TOP is its
/// first row and scrolls with the rest, as a thread's header does. Its keys are taken again only when VERSION (the rows
/// held, the number open) or their count changes; DRAW reads the window as it is when a row is built (an ask's `⋯`).
fn in_view<T: 'static>(top: El, rows: Arc<Vec<T>>, key: impl Fn(&T) -> String + 'static, draw: impl Fn(&App, &T) -> El + 'static, version: impl std::hash::Hash) -> El {
    let keyed = rows.clone();
    col()
        .key("insight-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            rows.len() + 1,
            44.0,
            move |_: &App, i| std::borrow::Cow::Owned(i.checked_sub(1).map_or_else(|| "listed-top".to_string(), |j| key(&keyed[j]))),
            move |s: &App, i| match i.checked_sub(1) {
                None => top.clone(),
                Some(j) => col().w_full().px(16.0).child(draw(s, &rows[j])),
            },
        )
        .virtual_rows_version(version)
}

/// The rows HELD and when they came; else what the page says in their place: that a coordd at PROTOCOL, before
/// `INSIGHT_PROTOCOL`, gives no insights yet, that they are being counted, or why coordd gave none.
fn held<'a, T>(held: Option<&'a Rows<T>>, protocol: u32, k: &Kit) -> Result<(&'a [T], i64), El> {
    let faint = |w: &str| text(w.to_string()).font(k.ty.small).color(k.c.text_muted).wrap();
    match held {
        None if protocol < coord_types::INSIGHT_PROTOCOL => Err(faint("Needs a newer engine.")),
        None => Err(faint("Counting…")),
        Some((Err(e), _)) => Err(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap()),
        Some((Ok(rows), at)) => Ok((rows, *at)),
    }
}

/// Insight (3) of ROWS as counted when they came, at AT (the page says so, and a thread open is open at that time),
/// with the number at node OPEN listed: his four tiles, the others', the heading and the threads listed; none listed
/// and no tile marked when OPEN is a number of his asks (their row lists them).
fn count_open_to_done(rows: &[Thread], at: i64, open: Option<&str>) -> OpenToDonePage {
    let n = open_to_done(rows, at);
    let asks = AskNumber::of(open).is_some();
    let (whose, number) = Number::of(open);
    let listed: Vec<(Thread, Option<i64>)> = if asks {
        Vec::new()
    } else {
        number.threads(match whose {
            Whose::His => &n.his,
            Whose::Agents => &n.agents,
        }).into_iter().map(|(x, secs)| (x.clone(), secs)).collect()
    };
    let four = |w: Whose, c: &Counts| {
        let rate = c.rate().map(|r| format!(" · {:.0}%", r * 100.0)).unwrap_or_default();
        let median = c.median.map(|m| format!("median {} open → end", crate::when::took(m))).unwrap_or_else(|| "none ended yet".into());
        [
            (Number::Opened, c.opened.len(), "Opened".to_string(), "threads and items".to_string()),
            (Number::Ended, c.ended.len(), format!("Ended{rate}"), median),
            (Number::Open, c.open.len(), "Open now".to_string(), "not ended".to_string()),
            (Number::Days, c.days.len(), "Open for days".to_string(), "open a day or more".to_string()),
        ]
        .into_iter()
        .map(|(num, count, label, under)| Tile { word: num.node(w), count, label, under, open: !asks && (w, num) == (whose, number), red: false })
        .collect()
    };
    let head = if asks { String::new() } else { listed_head(whose, number, &listed) };
    OpenToDonePage { his: four(Whose::His, &n.his), others: four(Whose::Agents, &n.agents), head, listed: Arc::new(listed) }
}

/// Insight (3)'s page P of project PI, its rows held since AT, at lens L: under TOP his tiles, the others' row, his
/// asks' row (ASKS: none from a coordd that gives none, else as counted or what it says in its place) and the heading,
/// then the threads listed, or the asks when one of their numbers is open, built in view only.
#[allow(clippy::too_many_arguments)]
fn open_to_done_body(top: El, p: &OpenToDonePage, asks: Option<Result<Arc<AsksPage>, El>>, pi: usize, at: i64, l: &Lens, k: &Kit) -> El {
    let row_of = |tiles: &[Tile]| row().w_full().gap(8.0).equal_columns(tiles.len().max(4)).children(tiles.iter().map(|t| tile(t, l, k)));
    let mut tiles = col().w_full().gap(8.0)
        .child(row_of(&p.his))
        .child(text("Opened by agents and programs").font(k.ty.small).color(k.c.text_muted).wrap().key("insight-agents"))
        .child(row_of(&p.others));
    let mut listed = None;
    match asks {
        None => {}
        Some(Err(why)) => tiles = tiles.child(why.key("insight-asks-why")),
        Some(Ok(a)) => {
            tiles = tiles.child(text(COUNTS_ASKS).font(k.ty.small).color(k.c.text_muted).wrap().key("insight-asks-counts")).child(row_of(&a.tiles));
            listed = Some(a).filter(|a| a.open);
        }
    }
    match listed {
        Some(a) => {
            let top = top.child(tiles).child(text(a.head.clone()).font(k.ty.small_medium).color(k.c.text_base).key("insight-listed"));
            let (now, kit) = (a.at, k.clone());
            in_view(top, a.listed.clone(), |x: &Ask| format!("ask-{}", x.ask.times.post), move |s, x| ask_row(s, pi, x, now, &kit), (at, a.at, &l.at))
        }
        None => {
            let top = top.child(tiles).child(text(p.head.clone()).font(k.ty.small_medium).color(k.c.text_base).key("insight-listed"));
            let kit = k.clone();
            in_view(top, p.listed.clone(), |(x, _)| format!("listed-{}", x.id), move |_, (x, secs)| thread_row(x, *secs, false, &kit), (at, &l.at))
        }
    }
}

/// Open → done's asks row of project PI at node OPEN, as counted from the asks held; what it says in its place while
/// they are on their way, or why coordd gave none; nothing from a coordd before ASKS_PROTOCOL that gave none (Open →
/// done as it was).
fn asks_held(i: &Insights, pi: usize, protocol: u32, open: Option<&str>, k: &Kit) -> Option<Result<Arc<AsksPage>, El>> {
    match i.asks.get(&pi) {
        None if protocol < coord_types::ASKS_PROTOCOL => None,
        None => Some(Err(text("Counting your asks…").font(k.ty.small).color(k.c.text_muted).wrap())),
        Some((Err(e), _)) => Some(Err(text(e.clone()).font(k.ty.small).color(k.c.danger.fg).wrap())),
        Some((Ok(rows), at)) => Some(Ok(i.page(&i.asks_page, pi, open, || count_asks(rows, *at, open)))),
    }
}

/// Open → done's asks row of ROWS as counted when they came, at AT, with the number at node OPEN listed: its five
/// tiles, In your window's line saying in red how many wait for his word (the only red, paper-research N-1446 p1786,
/// 2) and Signed off's his words' counts; when one of them is open, its heading and its asks. The counts are those of
/// the ledger's asks as coordd gives them; none is filtered here (p1786, 3).
fn count_asks(rows: &[Ask], at: i64, open: Option<&str>) -> AsksPage {
    let n = asks(rows);
    let open = AskNumber::of(open);
    let worth = |w: &str| n.signed.iter().filter(|a| a.ask.times.signed.as_ref().is_some_and(|s| s.worth == w)).count();
    let window = if n.yours.is_empty() { "built and installed there".to_string() } else { format!("{} wait for your word", n.yours.len()) };
    let signed = format!("{} worth it · {} fine · {} not worth it", worth("worth_it"), worth("fine"), worth("not_worth_it"));
    let tiles = [
        (AskNumber::Asked, n.asked.len(), "Asked", "posts of yours that ask an agent".to_string(), false),
        (AskNumber::Stuck, n.stuck.len(), "Nobody moves", "not answered, nothing done".to_string(), false),
        (AskNumber::Words, n.words.len(), "Answered in words", "answered, no commit".to_string(), false),
        (AskNumber::Window, n.window.len(), "In your window", window, !n.yours.is_empty()),
        (AskNumber::Signed, n.signed.len(), "Signed off", signed, false),
    ]
    .into_iter()
    .map(|(num, count, label, under, red)| Tile { word: num.word().to_string(), count, label: label.to_string(), under, open: Some(num) == open, red })
    .collect();
    let listed: Vec<Ask> = open.map(|o| o.asks(&n)).unwrap_or_default().into_iter().cloned().collect();
    AsksPage { tiles, open: open.is_some(), head: open.map(|o| o.head(listed.len())).unwrap_or_default(), listed: Arc::new(listed), at }
}

/// His word of worth as the row says it.
fn worth_words(w: &str) -> &str {
    match w {
        "worth_it" => "Worth it",
        "fine" => "Fine",
        "not_worth_it" => "Not worth it",
        w => w,
    }
}

/// Where ask A stands: its last point and who or what reached it, in the graph page's words (lanes.rs `leg_words`:
/// "taken by fixer", "first commit 0f40917b"), and when, with the agent behind a commit or a job.
fn stands(a: &Ask) -> (String, String) {
    let last = a.ask.last.as_ref();
    let by = last.map(|r| r.id.strip_prefix("agent:").unwrap_or(&r.id).to_string()).unwrap_or_default();
    let words = crate::map::lanes::leg_words(&crate::map::lanes::Leg { point: a.ask.point, by, at: 0, took: None });
    let when = last.map(|r| crate::when::at(&r.at)).unwrap_or_default();
    let when = match a.ask.who.as_deref().and_then(|w| w.strip_prefix("agent:")) {
        Some(w) if !words.contains(w) && !when.is_empty() => format!("{when} · {w}"),
        _ => when,
    };
    (words, when)
}

/// What ask A took, beside his word: the requests sent for it and their tokens (the graph's `handed`); under them, the
/// requests sent for posts naming it (`named`, never added in) and his posts on it once it was in his window. Where
/// coordd read no request for it, says so.
fn took_ask(a: &Ask) -> (String, String) {
    let sum = |xs: &[docket_machine::graph::Spent]| (xs.iter().map(|x| x.requests).sum::<usize>(), xs.iter().map(|x| x.tokens.total()).sum::<u64>());
    let (requests, tokens) = a.cost.as_ref().map_or((0, 0), |c| sum(&c.handed));
    let top = if requests == 0 { "no request read for it".to_string() } else { format!("{} · {} tokens", many(requests, "request"), crate::spend::compact(tokens as f64)) };
    let mut under = Vec::new();
    if let Some((named, _)) = a.cost.as_ref().map(|c| sum(&c.named)).filter(|(n, _)| *n > 0) {
        under.push(format!("{named} more naming it"));
    }
    if !a.after.is_empty() {
        under.push(format!("then {} of yours", many(a.after.len(), "post")));
    }
    (top, under.join(" · "))
}

/// His ask A of project PI as the asks row lists it, at NOW (when its rows came): his post's words, its id and its
/// thread's label; when he asked; how long it has been open, or took to his sign-off; where it stands ([`stands`]);
/// what it took ([`took_ask`]); and his word of worth (the one pressed while on its way, dimmed), or "waits for you"
/// in red when it is in his window and not signed off. A press zooms to his post's lines at the Events depth; its `⋯`
/// opens his three words on a line under the row ([`sign_menu`]), the row kept whole, so what it took stays beside them
/// and the row's columns stay under the others'.
fn ask_row(s: &App, pi: usize, a: &Ask, now: i64, k: &Kit) -> El {
    let x = &a.ask.times;
    let post = x.post.clone();
    let key = (pi, post.clone());
    let pressed = s.insights.signing.get(&key).map(String::as_str);
    let word = match pressed {
        Some("none") => None,
        Some(w) => Some(w),
        None => x.signed.as_ref().map(|w| w.worth.as_str()),
    };
    let open = match ask_took(a, now) {
        Some(secs) if x.signed.is_some() => format!("took {}", crate::when::took(secs)),
        Some(secs) => format!("open {}", crate::when::took(secs)),
        None => String::new(),
    };
    let more_open = s.insights.ask_more.as_ref() == Some(&key);
    let ((stands, when), (took, under)) = (stands(a), took_ask(a));
    let read = a.cost.as_ref().is_some_and(|c| !c.handed.is_empty());
    let main = row().grow(1.0).shrink().min_w(0.0).gap(10.0).center_y()
        .child(col().grow(1.0).shrink().min_w(0.0)
            .child(text(crate::refs::cut(&a.words, 80)).font(k.ty.small).color(k.c.text_base))
            .child(text(crate::refs::cut(&format!("{post} · {}", a.on), 100)).font(k.ty.small).color(k.c.text_faint)))
        .child(text(x.asked.as_deref().map(crate::when::at).unwrap_or_default()).font(k.ty.small).color(k.c.text_muted).w(104.0).min_w(104.0))
        .child(text(open).font(k.ty.small_medium).color(k.c.text_base).w(96.0).min_w(96.0))
        .child(col().w(190.0).min_w(190.0)
            .child(text(crate::refs::cut(&stands, 32)).font(k.ty.small).color(k.c.text_base))
            .child(text(when).font(k.ty.small).color(k.c.text_faint)))
        .child(col().w(170.0).min_w(170.0)
            .child(text(took).font(k.ty.small).color(if read { k.c.text_base } else { k.c.text_faint }))
            .child(text(crate::refs::cut(&under, 40)).font(k.ty.small).color(k.c.text_faint)));
    let zoom = post.clone();
    let main = k.interactive(main.child(text("Events ›").font(k.ty.small).color(k.c.text_accent)).key(format!("insight-ask-{post}")))
        .help("Your post's lines")
        .on_press(move |s: &mut App| crate::lens::zoom(s, Depth::Events, &zoom));
    let you = match (word, x.in_window.is_some()) {
        (Some(w), _) => text(worth_words(w).to_string()).font(k.ty.small_medium).color(if pressed.is_some() { k.c.text_muted } else { k.c.text_base }),
        (None, true) => text("waits for you").font(k.ty.small_medium).color(k.c.danger.fg),
        (None, false) => text(String::new()).font(k.ty.small).color(k.c.text_faint),
    };
    let at = key.clone();
    let more = k.interactive(row().key(format!("insight-ask-more-{post}")).child(text("⋯").font(k.ty.small).color(k.c.text_muted)))
        .help("Sign it off with your word of worth")
        .on_press(move |s: &mut App| s.insights.ask_more = if s.insights.ask_more.as_ref() == Some(&at) { None } else { Some(at.clone()) });
    let right = row().gap(8.0).center_y().child(you.w(96.0).min_w(96.0).key(format!("insight-ask-you-{post}"))).child(more);
    let line = row().w_full().gap(10.0).center_y().child(main).child(right);
    let mut whole = col().w_full().gap(6.0).py(4.0).border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak).key(format!("insight-ask-row-{post}")).child(line);
    if more_open {
        whole = whole.child(row().w_full().justify_end().child(sign_menu(pi, &post, word, k)));
    }
    whole
}

/// His three words of worth on ask POST of project PI, under its row once its `⋯` is pressed (as ending.rs picks how a
/// thread ended): WORD, his word now, picked; one press sends it ([`sign_off`]). Take back once he has a word on it;
/// ✕ closes it.
fn sign_menu(pi: usize, post: &str, word: Option<&str>, k: &Kit) -> El {
    let words: Vec<&str> = docket_machine::WORTHS.iter().map(|w| worth_words(w)).collect();
    let picked = word.and_then(|w| docket_machine::WORTHS.iter().position(|x| *x == w));
    let p = post.to_string();
    let pick = rune_kit::SegmentedControl::new(&words, picked).label("Your word of worth").fit().on_pick(move |s: &mut App, i, _| {
        if let Some(w) = docket_machine::WORTHS.get(i) {
            sign_off(s, pi, &p, w);
        }
    });
    let mut r = row().gap(8.0).center_y().child(text("Signed off").font(k.ty.small).color(k.c.text_muted)).child(pick.build(k).key(format!("insight-sign-{post}")));
    if word.is_some() {
        let p = post.to_string();
        r = r.child(Button::new("Take back").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| sign_off(s, pi, &p, "none")).build(k).key(format!("insight-unsign-{post}")));
    }
    let close = k.interactive(row().key(format!("insight-ask-close-{post}")).child(text("✕").font(k.ty.small).color(k.c.text_muted))).help("Close").on_press(|s: &mut App| s.insights.ask_more = None);
    r.child(close)
}

/// N THINGs in words: "1 agent", "3 agents".
fn many(n: usize, thing: &str) -> String {
    if n == 1 { format!("1 {thing}") } else { format!("{n} {thing}s") }
}

/// Insight (2) of ROWS as counted when they came, at AT, with what node OPEN lists: the two tiles, the heading, and
/// the topics or a topic's lines.
fn count_per_topic(rows: &[Thread], at: i64, open: Option<&str>) -> TopicsPage {
    let n = per_topic(rows, at);
    let open = Listing::of(open);
    let tile = |l: Listing, label: &str, t: &Topics| {
        let agents = t.agents.map_or_else(|| "no topic".to_string(), |a| format!("median {}", many(a, "agent")));
        let took = t.took.map(|s| format!(", {} open → done", crate::when::took(s))).unwrap_or_default();
        Tile { word: l.word(), count: t.list.len(), label: label.into(), under: format!("{agents}{took}"), open: l == open, red: false }
    };
    let tiles = vec![tile(Listing::Threads, "Threads", &n.threads), tile(Listing::Items, "Items", &n.items)];
    let topic = |t: &Topic| Listed::Topic { line: t.line.clone(), agents: t.agents.iter().map(|a| a.to_string()).collect(), lines: t.lines.len(), secs: t.secs };
    let (head, listed): (String, Vec<Listed>) = match &open {
        Listing::Threads => (format!("Threads · {}, the most agents first", many(n.threads.list.len(), "topic")), n.threads.list.iter().map(topic).collect()),
        Listing::Items => (format!("Items · {}, the most agents first", many(n.items.list.len(), "topic")), n.items.list.iter().map(topic).collect()),
        Listing::Topic(id) => match n.threads.list.iter().chain(&n.items.list).find(|t| t.line.id == *id) {
            Some(t) => {
                let agents = if t.agents.is_empty() { "no agent".to_string() } else { format!("{}: {}", many(t.agents.len(), "agent"), t.agents.join(", ")) };
                (format!("{} {} · {agents} · {}", t.line.id, t.line.label, many(t.lines.len(), "line")), t.lines.iter().map(|x| Listed::Line((*x).clone(), took(x, at))).collect())
            }
            None => (format!("{id} is no line of the outline now"), Vec::new()),
        },
    };
    TopicsPage { tiles, head, listed: Arc::new(listed) }
}

/// Insight (2)'s page P, its rows held since AT, at lens L: under TOP its tiles and the heading, then what is listed,
/// built in view only.
fn per_topic_body(top: El, p: &TopicsPage, at: i64, l: &Lens, k: &Kit) -> El {
    let tiles = row().w_full().gap(8.0).equal_columns(4).children(p.tiles.iter().map(|t| tile(t, l, k)));
    let top = top.child(tiles).child(text(p.head.clone()).font(k.ty.small_medium).color(k.c.text_base).key("insight-listed"));
    let (kit, lens) = (k.clone(), l.clone());
    let key = |x: &Listed| match x {
        Listed::Topic { line, .. } => format!("topic-{}", line.id),
        Listed::Line(x, _) => format!("listed-{}", x.id),
    };
    in_view(top, p.listed.clone(), key, move |_, x| match x {
        Listed::Topic { line, agents, lines, secs } => topic_row(line, agents, *lines, *secs, &lens, &kit),
        Listed::Line(x, secs) => thread_row(x, *secs, true, &kit),
    }, (at, &l.at))
}

/// Topic LINE: its label, id, kind, state and how many LINES it holds; its AGENTS, how many and by name; SECS, the time
/// it took or has been open. A press lists its lines (lens L at `topic:ID`), a place of its own (Back).
fn topic_row(line: &Thread, agents: &[String], lines: usize, secs: Option<i64>, l: &Lens, k: &Kit) -> El {
    let lens = Lens { at: Some(Listing::Topic(line.id.clone()).word()), ..l.clone() };
    let r = row().w_full().gap(10.0).py(4.0).center_y().border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
        .child(col().grow(1.0).shrink().min_w(0.0)
            .child(text(crate::refs::cut(&line.label, 80)).font(k.ty.small).color(k.c.text_base))
            .child(text(format!("{} · {} · {} · {}", line.id, line.kind, line.state, many(lines, "line"))).font(k.ty.small).color(k.c.text_faint)))
        .child(col().w(240.0).min_w(240.0)
            .child(text(many(agents.len(), "agent")).font(k.ty.small_medium).color(k.c.text_base))
            .child(text(crate::refs::cut(&agents.join(", "), 48)).font(k.ty.small).color(k.c.text_faint)))
        .child(text(took_words(line, secs)).font(k.ty.small_medium).color(k.c.text_base).w(96.0).min_w(96.0))
        .child(text("Lines ›").font(k.ty.small).color(k.c.text_accent));
    k.interactive(r.key(format!("insight-topic-{}", line.id))).help("Its lines, each with its agents").on_press(move |s: &mut App| s.show_sel(crate::Sel::Lens(lens.clone())))
}

/// SECS, the time X took when it ended, else has been open, in words; nothing when a time did not read.
fn took_words(x: &Thread, secs: Option<i64>) -> String {
    secs.map(|s| if x.ended.is_some() { format!("took {}", crate::when::took(s)) } else { format!("open {}", crate::when::took(s)) }).unwrap_or_default()
}

/// Who or what opened a thread, from its row's `by` (docket-machine `opened_by`), after its state (proto-window N-1446
/// p646): an agent by its name ("by dev"); a binding, written as the Docket's picture words what starts it
/// (`picture::start_words`), after what it is: "by a clock, every 1 h", "by a program's handler, when cut-build is
/// build", "by an agent's event, agent post".
fn by_words(by: &str) -> String {
    use docket_machine::syntax::Every;
    match docket_machine::syntax::parse_bindings("", by).ok().and_then(|bs| bs.into_iter().next()?.ok()) {
        Some(b) => {
            let what = match b.every {
                Every::Clock(_) => "a clock",
                Every::When { .. } => "a program's handler",
                Every::Each | Every::Count(_) => "an agent's event",
            };
            format!("by {what}, {}", crate::picture::start_words(&b))
        }
        None => format!("by {by}"),
    }
}

/// The heading over the LISTED threads and items of WHOSE number OPEN: "3 threads", "2 threads, 1 item" (proto-lead
/// N-1446 p671), "1 item".
fn listed_head(whose: Whose, open: Number, listed: &[(Thread, Option<i64>)]) -> String {
    let items = listed.iter().filter(|(x, _)| x.kind == "item").count();
    let threads = match (listed.len() - items, items) {
        (n, 0) => many(n, "thread"),
        (0, i) => many(i, "item"),
        (n, i) => format!("{}, {}", many(n, "thread"), many(i, "item")),
    };
    let agents = if whose == Whose::Agents { "Opened by agents and programs · " } else { "" };
    match open {
        Number::Opened => format!("{agents}Opened · {threads}, the newest first"),
        Number::Ended => format!("{agents}Ended · {threads}, the latest end first"),
        Number::Open => format!("{agents}Open now · {threads}, the longest open first"),
        Number::Days => format!("{agents}Open for days · {threads}"),
    }
}

/// Tile T: its label, its count and the line under it, marked when open; a press shows lens L at its word (the
/// number's node), a place of its own (Back).
fn tile(t: &Tile, l: &Lens, k: &Kit) -> El {
    let value = TextStyle { size: 22.0, line_height: 28.0, ..k.ty.ui_medium };
    let lens = Lens { at: Some(t.word.clone()), ..l.clone() };
    let tile = col().w_full().min_w(0.0).gap(2.0).padding(12.0).radius(k.radius.md).border(1.0, if t.open { k.c.border_focus } else { k.c.border_weak }).fill(k.c.layer_01)
        .child(text(t.label.clone()).font(k.ty.small).color(k.c.text_muted))
        .child(text(t.count.to_string()).font(value).color(k.c.text_base))
        .child(text(t.under.clone()).font(k.ty.small).color(if t.red { k.c.danger.fg } else { k.c.text_faint }).wrap());
    k.interactive(tile.key(format!("insight-{}", t.word))).on_press(move |s: &mut App| s.show_sel(crate::Sel::Lens(lens.clone())))
}

/// Thread X: its label and id, its state and who or what opened it when not he ([`by_words`]), when it opened and
/// ended, and SECS, the time it took or has been open; with AGENTS, insight (2)'s line, its own agents: "given to …"
/// on every line and "started by …" on an item's, so a topic's names read off its rows (proto-lead N-1446 p678). A press zooms to its lines at the Events depth: an ended one's from its last post, where its end
/// follows; any other's from its opening.
fn thread_row(x: &Thread, secs: Option<i64>, agents: bool, k: &Kit) -> El {
    let when = match &x.ended {
        Some(e) => format!("{} → {}", crate::when::at(&x.opened), crate::when::at(e)),
        None => format!("opened {}", crate::when::at(&x.opened)),
    };
    let node = match (&x.ended, x.last) {
        (Some(_), Some(n)) => format!("{}.p{n}", x.id),
        _ => x.id.clone(),
    };
    let label = if x.archived { format!("{} (archived)", x.label) } else { x.label.clone() };
    let by = x.by.as_deref().map(|b| format!(" · {}", by_words(b))).unwrap_or_default();
    let names = |w: &str, v: &[String]| format!(" · {w} {}", if v.is_empty() { "nobody".to_string() } else { v.join(", ") });
    let own = match (agents, x.kind == "item") {
        (false, _) => String::new(),
        (true, false) => names("given to", &x.agents),
        (true, true) => names("given to", &x.agents) + &names("started by", &x.started),
    };
    let r = row().w_full().gap(10.0).py(4.0).center_y().border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
        .child(col().grow(1.0).shrink().min_w(0.0)
            .child(text(crate::refs::cut(&label, 80)).font(k.ty.small).color(k.c.text_base))
            .child(text(crate::refs::cut(&format!("{} · {} · {}{by}{own}", x.id, x.kind, x.state), 140)).font(k.ty.small).color(k.c.text_faint)))
        .child(text(when).font(k.ty.small).color(k.c.text_muted))
        .child(text(took_words(x, secs)).font(k.ty.small_medium).color(k.c.text_base).w(96.0).min_w(96.0))
        .child(text("Events ›").font(k.ty.small).color(k.c.text_accent));
    k.interactive(r.key(format!("insight-row-{}", x.id))).help("Its lines").on_press(move |s: &mut App| crate::lens::zoom(s, Depth::Events, &node))
}

/// A model as the page names it: its name as recorded, or that the turn's line names none.
fn model_name(m: &str) -> String {
    if m.is_empty() { "no model recorded".into() } else { m.to_string() }
}

/// An hour of the day as the record keys it (`2026-11-20T18`, UTC), on his clock.
fn hour_name(h: &str) -> String {
    crate::when::at(&format!("{h}:00:00Z"))
}

/// Work over the turns known in T, as a percent; none when none is known.
fn share(t: &Tally) -> String {
    t.share().map(|r| format!("{:.0}% work", r * 100.0)).unwrap_or_else(|| "none known".into())
}

/// Insight (1) of TURNS as counted, with the count at node OPEN listed: its tiles, its tables by agent, model and hour,
/// the heading and the turns listed.
fn count_work_talk(turns: &[Turn], open: Option<&str>) -> WorkTalkPage {
    let n = work_talk(turns);
    let open = Count::of(open);
    let listed: Vec<Turn> = open.turns(&n).into_iter().cloned().collect();
    // the first he reads on the day this coordd goes in, when every turn is not known (proto-window N-1446 p545, 2)
    let known = match n.all.share() {
        Some(_) => format!("{} of the turns known", share(&n.all)),
        None => "no turn known yet".to_string(),
    };
    let tiles = [
        (Did::Work, n.all.work.len(), "Work turns".to_string(), known),
        (Did::Talk, n.all.talk.len(), "Talk turns".to_string(), "only coord's tools, or none".to_string()),
        (Did::NotKnown, n.all.not_known.len(), "Not known".to_string(), "turns that ended before this release recorded tools".to_string()),
    ]
    .into_iter()
    .map(|(did, count, label, under)| {
        let c = Count::Did(did);
        Tile { word: c.word(), count, label, under, open: c == open, red: false }
    })
    .collect();
    let table = |rows: &[Tally], count: fn(String) -> Count, name: fn(&str) -> String| -> Vec<TallyRow> {
        rows.iter().map(|t| {
            let c = count(t.label.clone());
            let not_known = if t.not_known.is_empty() { String::new() } else { format!(" · {} not known", t.not_known.len()) };
            TallyRow { open: c == open, c, name: name(&t.label), line: format!("{} work · {} talk{not_known}", t.work.len(), t.talk.len()), share: share(t) }
        }).collect()
    };
    let tables = [("By agent", table(&n.agents, Count::Agent, str::to_string)), ("By model", table(&n.models, Count::Model, model_name)), ("By hour", table(&n.hours, Count::Hour, hour_name))];
    WorkTalkPage { tiles, tables, head: open.head(listed.len()), listed: Arc::new(listed) }
}

/// Insight (1)'s page P, its rows held since AT, at lens L: under TOP its tiles, its tables and the heading, then the
/// turns listed, built in view only.
fn work_talk_body(top: El, p: &WorkTalkPage, at: i64, l: &Lens, k: &Kit) -> El {
    let tiles = row().w_full().gap(8.0).equal_columns(p.tiles.len()).children(p.tiles.iter().map(|t| tile(t, l, k)));
    let tables = row().w_full().gap(12.0).equal_columns(p.tables.len()).children(p.tables.iter().map(|(title, rows)| {
        col().w_full().min_w(0.0).gap(2.0)
            .child(text(*title).font(k.ty.small_medium).color(k.c.text_base))
            .child(col().w_full().max_h(240.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).children(rows.iter().map(|r| tally_row(r, l, k))))
    }));
    let top = top.child(tiles).child(tables).child(text(p.head.clone()).font(k.ty.small_medium).color(k.c.text_base).key("insight-listed"));
    let kit = k.clone();
    in_view(top, p.listed.clone(), |x| format!("listed-{}", x.line), move |_, x| turn_row(x, &kit), (at, &l.at))
}

/// Row R of a table of insight (1): its name as the page names it, its tally (work, talk, not known) and share, marked
/// when open; a press lists its turns, a place of its own (Back).
fn tally_row(r: &TallyRow, l: &Lens, k: &Kit) -> El {
    let lens = Lens { at: Some(r.c.word()), ..l.clone() };
    let row_ = row().w_full().gap(8.0).py(3.0).px(6.0).center_y().radius(k.radius.sm).fill(k.c.layer_01).border(1.0, if r.open { k.c.border_focus } else { k.c.layer_01 })
        .child(text(crate::refs::cut(&r.name, 40)).font(k.ty.small).color(k.c.text_base).grow(1.0).shrink().min_w(0.0))
        .child(text(r.line.clone()).font(k.ty.small).color(k.c.text_muted))
        .child(text(r.share.clone()).font(k.ty.small).color(k.c.text_faint));
    k.interactive(row_.key(format!("insight-{}", r.c.word()))).on_press(move |s: &mut App| s.show_sel(crate::Sel::Lens(lens.clone())))
}

/// Turn X of the turns listed: its agent, when it ended, what it did and on which model, its tools by how often it
/// called them. A press zooms to the Events depth at the post it was given, where the turn shows; one given no post has
/// nowhere to zoom: none named on its line, or a delivery that is not a post (a question's `Q-15.c`, a ruling's
/// `R-…c`, a post taken back), which lens.rs `parts` reads as no post (proto-window N-1446 p545, 1).
fn turn_row(x: &Turn, k: &Kit) -> El {
    let did = match x.did() {
        Did::Work => "work",
        Did::Talk => "talk",
        Did::NotKnown => "not known",
    };
    let mut tools: Vec<(&String, &u32)> = x.tools.iter().collect();
    tools.sort_by_key(|(name, n)| (std::cmp::Reverse(**n), (*name).clone()));
    let tools = tools.iter().map(|(name, n)| format!("{name} {n}")).collect::<Vec<_>>().join(" · ");
    let model = x.model.as_deref().map_or_else(|| model_name(""), str::to_string);
    let r = row().w_full().gap(10.0).py(4.0).center_y().border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_weak)
        .child(col().grow(1.0).shrink().min_w(0.0)
            .child(text(format!("{} · {did} · {model}", x.agent)).font(k.ty.small).color(k.c.text_base))
            .child(text(crate::refs::cut(&tools, 120)).font(k.ty.small).color(k.c.text_faint)))
        .child(text(crate::when::at(&x.at)).font(k.ty.small).color(k.c.text_muted));
    let key = format!("insight-turn-{}", x.line);
    match x.delivery.clone().filter(|d| crate::lens::parts(d).1.is_some()) {
        Some(node) => k.interactive(r.child(text("Events ›").font(k.ty.small).color(k.c.text_accent)).key(key)).help("The turn, at its post").on_press(move |s: &mut App| crate::lens::zoom(s, Depth::Events, &node)),
        None => r.key(key),
    }
}

/// The insight lenses the structure pane offers, in his order (proto-lead p388): (3) first, then (1), then (2).
pub fn lenses() -> Vec<Lens> {
    vec![Lens::of(Filter::OpenToDone), Lens::of(Filter::WorkTalk), Lens::of(Filter::AgentsPerTopic)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use docket_machine::insight::OpenToDone;
    use serde_json::json;

    /// A project whose coordd answers insights, with no thread in its snapshot: the insight's rows are its own.
    fn project() -> crate::Proj {
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        p.down = None;
        p.protocol = coord_types::INSIGHT_PROTOCOL;
        // a thread, as the snapshot carries it: the structure, Insights in it, is drawn only in a
        // project that has a thread (sidebar.rs `structure`; paper-research N-1446 p3085)
        let v: coord_types::TicketView = serde_json::from_value(json!({"id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {}, "title": "the parser", "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 1,
            "opened_at": "2026-11-19T08:00:00Z", "given": [], "posts": []})).unwrap();
        p.live.tickets.insert(1, v);
        p
    }

    /// A clock's binding as written, the `by` of a thread it opened (docket-machine `opened_by`; proto-lead N-1446 p581).
    const CLOCK: &str = "every 1h open thread(\"lead\", [\"lead\"], [\"lead\"], \"the hourly check\", Named, none)";

    /// Made-up rows as coordd answers them, their times from now. His: the parser opened 3 days ago and done after 3 h,
    /// the lexer open 50 h, the printer open 2 h, the docs opened 5 h ago and dropped after 1 h. Opened by agents and
    /// programs: dev's build open 30 h, lead's review opened 6 h ago and ended after 1 h, the hourly check a clock
    /// opened 1 h ago, open, and lead's two items, the release notes open 20 h and tidy the logs opened 8 h ago and done
    /// after 2 h. Insight (2)'s: the printer and the release notes stand under the lexer, the review under the parser;
    /// given there: fixer and lead on the parser, fixer on the lexer, dev on the printer and the build, lead and dev on
    /// the review, lead on the hourly check, nobody on the docs; dev and lead started the release notes, fixer tidy the
    /// logs.
    fn rows() -> String {
        let at = |h: i64| docket_machine::at_secs(chrono::Utc::now().timestamp() - h * 3600);
        let t = |id: &str, label: &str, kind: &str, by: Option<&str>, (opened, ended): (i64, Option<i64>), state: &str, last: u32, parent: Option<&str>, agents: &[&str], started: &[&str]| {
            json!({"id": id, "label": label, "kind": kind, "opened": at(opened), "by": by, "ended": ended.map(at), "state": state, "last": last, "parent": parent, "agents": agents, "started": started}).to_string()
        };
        [
            t("N-1", "the parser", "thread", None, (72, Some(69)), "done", 4, None, &["fixer", "lead"], &[]),
            t("N-2", "the lexer", "thread", None, (50, None), "waits on fixer", 2, None, &["fixer"], &[]),
            t("N-3", "the printer", "thread", None, (2, None), "at work", 1, Some("N-2"), &["dev"], &[]),
            t("N-4", "the docs", "thread", None, (5, Some(4)), "dropped", 6, None, &[], &[]),
            t("N-5", "the build", "thread", Some("dev"), (30, None), "quiet", 3, None, &["dev"], &[]),
            t("N-6", "the review", "thread", Some("lead"), (6, Some(5)), "done", 2, Some("N-1"), &["dev", "lead"], &[]),
            t("N-7", "the hourly check", "thread", Some(CLOCK), (1, None), "at work", 1, None, &["lead"], &[]),
            t("N-8", "the release notes", "item", Some("lead"), (20, None), "at work", 1, Some("N-2"), &[], &["dev", "lead"]),
            t("N-9", "tidy the logs", "item", Some("lead"), (8, Some(6)), "done", 2, None, &[], &["fixer"]),
        ]
        .join("\n")
    }

    /// Made-up turns as coordd answers them (docket-machine's insight test's five): lead's line from before (not known);
    /// fixer's that ran, read and replied (work); lead's that replied and ran coord alone in a shell (talk); fixer's
    /// that called nothing (talk); fixer's Codex turn that replied and edited (work). Each given post N-1.pLINE, but the
    /// first, given a question's answer (`Q-15.c`), no post.
    fn turns() -> String {
        let t = |agent: &str, at: &str, tools: serde_json::Value, model: Option<&str>, line: u64| {
            let delivery = if line == 1 { "Q-15.c".to_string() } else { format!("N-1.p{line}") };
            json!({"agent": agent, "at": at, "delivery": delivery, "tools": tools, "model": model, "line": line}).to_string()
        };
        [
            t("lead", "2026-11-20T08:10:00Z", json!({}), None, 1),
            t("fixer", "2026-11-20T09:05:00Z", json!({"Bash": 4, "Read": 2, "mcp__coord__reply": 1}), Some("claude-opus-5-5"), 2),
            t("lead", "2026-11-20T09:40:00Z", json!({"mcp__coord__reply": 1, "Bash(coord)": 2}), Some("claude-sonnet-5-5"), 3),
            t("fixer", "2026-11-20T10:15:00Z", json!({}), Some("claude-opus-5-5"), 4),
            t("fixer", "2026-11-20T10:50:00Z", json!({"coord__reply": 1, "Edit": 3}), Some("gpt-6.2-sol"), 5),
        ]
        .join("\n")
    }

    /// His made-up asks as coordd answers them (`docket_machine::insight::Ask` JSON lines), in the order he asked them,
    /// their times hours before now, all on N-1104 (your one group chat) but three:
    /// - pin the lexer's thread (74 h): reached dev, nobody took it;
    /// - the parser drops the last line (N-2, 50 h): fixer took it, nothing since;
    /// - why is the build slow (30 h): lead answered it in words;
    /// - make the printer ping (N-3, 26 h): dev's commit 0f40917b, green, not in his window yet;
    /// - the docs page is blank on Windows (20 h): in his window 6 h ago, not signed off unless E_SIGNED (a minute
    ///   ago); 14 requests by dev, 3 more by lead for a post naming it;
    /// - a dark theme for the review page (12 h): in his window 4 h ago, signed off worth it 2 h ago, one post of his
    ///   after it landed; 6 requests;
    /// - tidy the logs folder (10 h): reached dev, signed off not worth it an hour later;
    /// - a second look at the docs (N-4, 8 h): fixer answered it in words, signed off fine.
    fn his_asks(e_signed: Option<&str>) -> String {
        use docket_machine::graph::{AskNow, AskTimes, Cost, Reached, Signed, Spent, Tokens};
        let now = chrono::Utc::now().timestamp();
        let at = |h: f64| docket_machine::at_secs(now - (h * 3600.0) as i64);
        let r = |h: f64, id: &str, via: Option<&str>| Some(Reached { at: at(h), id: id.into(), via: via.map(str::to_string) });
        let spent = |agent: &str, what: &str, requests: usize, input: u64| Spent { agent: agent.into(), what: what.into(), requests, carried: requests, tokens: Tokens { input, output: input / 4, cache_read: input * 8, cache_write: input / 2, reasoning: input / 12 } };
        let blank = |post: &str, h: f64| AskTimes { post: post.into(), asked: Some(at(h)), reached: r(h - 0.01, "agent:dev", None), taken: None, first_commit: None, merged: None, first_green: None, in_window: None, asks: vec!["agent:dev".into()], closed: None, signed: None };
        let signed = |h: f64, w: &str| Some(Signed { at: at(h), worth: w.into() });
        let mut v: Vec<(AskTimes, &str, &str, Option<Cost>, Vec<String>)> = Vec::new();
        v.push((blank("N-1104.p12", 74.0), "pin the lexer's thread in the sidebar", "Your one group chat", None, Vec::new()));
        let mut b = blank("N-2.p3", 50.0);
        b.taken = r(49.0, "agent:fixer", Some("N-2.p4"));
        v.push((b, "the parser drops the last line", "the lexer", None, Vec::new()));
        let mut c = blank("N-1104.p20", 30.0);
        (c.taken, c.closed) = (r(29.9, "agent:lead", Some("N-1104.p21")), r(29.0, "agent:lead", Some("N-1104.p21")));
        v.push((c, "why is the build slow today", "Your one group chat", Some(Cost { post: "N-1104.p20".into(), handed: vec![spent("lead", "claude-sonnet-5-5 high", 2, 9_000)], ..Default::default() }), Vec::new()));
        let mut d = blank("N-3.p2", 26.0);
        (d.taken, d.first_commit, d.first_green) = (r(25.9, "agent:dev", Some("N-3.p3")), r(20.0, "commit:0f40917b2c", None), r(19.0, "job:3-1", None));
        v.push((d, "make the printer ping here when it lands", "the printer", Some(Cost { post: "N-3.p2".into(), handed: vec![spent("dev", "claude-opus-5-5 xhigh", 9, 40_000)], ..Default::default() }), Vec::new()));
        let mut e = blank("N-1104.p31", 20.0);
        (e.taken, e.first_commit, e.merged, e.first_green, e.in_window) = (r(19.9, "agent:dev", Some("N-1104.p32")), r(18.0, "commit:9a3c1e22d0", None), r(17.0, "commit:9a3c1e22d0", None), r(16.9, "job:4-2", None), r(6.0, "build:20261122-120000-9a3c1e22", None));
        e.signed = e_signed.and_then(|w| signed(1.0 / 60.0, w));
        let cost = Cost { post: "N-1104.p31".into(), handed: vec![spent("dev", "claude-opus-5-5 xhigh", 14, 120_000)], named: vec![spent("lead", "claude-sonnet-5-5 high", 3, 11_000)], shared: 0 };
        v.push((e, "the docs page is blank on Windows", "Your one group chat", Some(cost), Vec::new()));
        let mut f = blank("N-1104.p35", 12.0);
        (f.taken, f.first_commit, f.merged, f.in_window, f.signed) = (r(11.9, "agent:lead", Some("N-1104.p36")), r(10.0, "commit:5e1d7c03aa", None), r(9.0, "commit:5e1d7c03aa", None), r(4.0, "build:20261122-140000-5e1d7c03", None), signed(2.0, "worth_it"));
        v.push((f, "a dark theme for the review page", "Your one group chat", Some(Cost { post: "N-1104.p35".into(), handed: vec![spent("lead", "claude-opus-5-5 high", 6, 52_000)], ..Default::default() }), vec!["N-1104.p37".into()]));
        let mut g = blank("N-1104.p38", 10.0);
        g.signed = signed(9.0, "not_worth_it");
        v.push((g, "tidy the logs folder", "Your one group chat", None, Vec::new()));
        let mut h = blank("N-4.p6", 8.0);
        (h.taken, h.closed, h.signed) = (r(7.9, "agent:fixer", Some("N-4.p7")), r(7.0, "agent:fixer", Some("N-4.p7")), signed(6.0, "fine"));
        v.push((h, "a second look at the docs", "the docs", None, Vec::new()));
        v.into_iter()
            .map(|(times, words, on, cost, after)| {
                let (point, last) = times.points().into_iter().rev().find_map(|(p, _, r)| Some((p, r?.clone()))).map_or((docket_machine::graph::Point::Asked, None), |(p, r)| (p, Some(r)));
                let who = last.as_ref().map(|r| if r.id.starts_with("agent:") { r.id.clone() } else { "agent:dev".into() });
                serde_json::to_string(&Ask { ask: AskNow { point, last, who, times }, words: words.into(), on: on.into(), cost, after }).unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The text drawn at KEY, and its colour.
    fn drawn(rt: &rune::runtime::Runtime<App>, key: &str) -> Option<(String, Option<rune::Color>)> {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).map(|e| (e.text.unwrap_or_default(), e.text_color))
    }

    /// The colour of the text T drawn, the first.
    fn colour(rt: &rune::runtime::Runtime<App>, t: &str) -> Option<rune::Color> {
        rt.elements().into_iter().find(|e| e.text.as_deref() == Some(t)).and_then(|e| e.text_color)
    }

    fn frames(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..4 {
            rt.frame(0.016);
        }
    }

    /// Two seconds of frames, a colour's change eased to its end, before a colour is read.
    fn settled(rt: &mut rune::runtime::Runtime<App>) {
        for _ in 0..40 {
            rt.frame(0.05);
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

    /// His presses (proto-reader's A5.ins row): the structure pane → Insights → Open → done, which opens at his Open
    /// for days; a number → the threads it counts; a thread → its lines at the Events depth, an ended one's from its
    /// last post, from Ended and from Opened alike (consult-3 p451, 2); the agents' Open for days → theirs. The numbers
    /// drawn are docket-machine's `open_to_done` of coordd's rows.
    #[test]
    fn an_insight_opens_from_the_structure_and_a_number_lists_its_threads_down_to_their_events() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("tree-insights").is_some() && rt.find("tree-insight-open-to-done").is_none(), "Insights folded until he unfolds it");
        press(&mut rt, "tree-fold-insights");
        press(&mut rt, "tree-insight-open-to-done");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens::of(Filter::OpenToDone)), "its lens, at no number");
        rt.update(|s, _| came(s, 0, Insight::OpenToDone, true, rows()));
        rt.invalidate();
        frames(&mut rt);
        assert!(rt.find("insight-counts").is_some(), "what it counts, said");
        assert!(rt.find("insight-row-N-2").is_some() && rt.find("insight-row-N-3").is_none() && rt.find("insight-row-N-5").is_none(), "his Open for days first: the lexer, not the printer, not dev's build");
        press(&mut rt, "insight-ended");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("ended".into()), ..Lens::of(Filter::OpenToDone) }), "the number kept as the lens's node");
        assert!(rt.find("insight-row-N-1").is_some() && rt.find("insight-row-N-4").is_some() && rt.find("insight-row-N-2").is_none(), "the ended ones");
        press(&mut rt, "insight-row-N-4");
        let docs = crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-4.p6".into()) });
        assert_eq!(rt.state.sel, docs, "its lines from its last post");
        rt.update(|s, _| s.show_sel(crate::Sel::Lens(Lens::of(Filter::OpenToDone))));
        rt.invalidate();
        frames(&mut rt);
        press(&mut rt, "insight-opened");
        assert!(rt.find("insight-row-N-4").is_some() && rt.find("insight-row-N-5").is_none(), "his opened");
        press(&mut rt, "insight-row-N-4");
        assert_eq!(rt.state.sel, docs, "from Opened too, its lines from its last post");
        rt.update(|s, _| s.show_sel(crate::Sel::Lens(Lens::of(Filter::OpenToDone))));
        rt.invalidate();
        frames(&mut rt);
        press(&mut rt, "insight-agents-days");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("agents-days".into()), ..Lens::of(Filter::OpenToDone) }));
        assert!(rt.find("insight-row-N-5").is_some() && rt.find("insight-row-N-2").is_none() && rt.find("insight-row-N-7").is_none(), "the agents' open for days: dev's build, not the clock's hour-old check");
        press(&mut rt, "insight-agents-open");
        assert!(rt.find("insight-row-N-7").is_some() && rt.find("insight-row-N-5").is_some() && rt.find("insight-row-N-8").is_some(), "open now in that row: the clock's thread, dev's build, lead's item");
        let Some((Ok(held), at)) = rt.state.insights.threads.get(&0) else { panic!("the rows held") };
        assert_eq!(count_open_to_done(held, *at, Some("agents-open")).head, "Opened by agents and programs · Open now · 2 threads, 1 item, the longest open first", "an item among them named");
        assert_eq!(count_open_to_done(held, *at, Some("days")).head, "Open for days · 1 thread");
        let OpenToDone { his: n, agents: a } = open_to_done(held, *at);
        assert_eq!((n.opened.len(), n.ended.len(), n.open.len(), n.days.len(), n.median), (4, 2, 2, 1, Some(3600)));
        assert_eq!((a.opened.len(), a.ended.len(), a.open.len(), a.days.len(), a.median), (5, 2, 3, 1, Some(3600)));
    }

    /// His presses for insight (2): the structure pane → Insights → Agents per topic, which opens at Threads, the most
    /// agents first; a topic → its lines, each with its own agents, whose names are the topic's (an item's starters
    /// among them: proto-lead N-1446 p671); a line → its lines at the Events depth, where its
    /// turns show; Items → the items, each with who started it, an item its own one line. The numbers drawn are
    /// docket-machine's `per_topic` of coordd's rows.
    #[test]
    fn agents_per_topic_opens_from_the_structure_and_a_topic_lists_its_lines_down_to_their_events() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        press(&mut rt, "tree-fold-insights");
        press(&mut rt, "tree-insight-agents-per-topic");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens::of(Filter::AgentsPerTopic)), "its lens, at no topic");
        rt.update(|s, _| came(s, 0, Insight::OpenToDone, true, rows()));
        rt.invalidate();
        frames(&mut rt);
        assert!(rt.find("insight-counts").is_some(), "what it counts, said");
        let order = ["N-2", "N-1", "N-6", "N-7", "N-5", "N-3", "N-4"].map(|id| rt.find(&format!("insight-topic-{id}")).map(|(_, y)| y));
        assert!(order.iter().all(Option::is_some) && order.windows(2).all(|w| w[0] < w[1]), "Threads first, the most agents first: {order:?}");
        assert!(rt.find("insight-topic-N-8").is_none(), "no item among the threads");
        press(&mut rt, "insight-topic-N-2");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("topic:N-2".into()), ..Lens::of(Filter::AgentsPerTopic) }), "the topic kept as the lens's node");
        let lines = |rt: &rune::runtime::Runtime<App>| (1..=9).filter(|n| rt.find(&format!("insight-row-N-{n}")).is_some()).collect::<Vec<_>>();
        assert_eq!(lines(&rt), vec![2, 3, 8], "the lexer, the printer and the item under it");
        press(&mut rt, "insight-row-N-3");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-3".into()) }), "an open line's lines from its opening");
        rt.update(|s, _| s.show_sel(crate::Sel::Lens(Lens::of(Filter::AgentsPerTopic))));
        rt.invalidate();
        frames(&mut rt);
        press(&mut rt, "insight-items");
        assert!(rt.find("insight-topic-N-8").is_some() && rt.find("insight-topic-N-9").is_some() && rt.find("insight-topic-N-2").is_none(), "the items");
        press(&mut rt, "insight-topic-N-9");
        assert_eq!(lines(&rt), vec![9], "an item is its own line");
        press(&mut rt, "insight-row-N-9");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-9.p2".into()) }), "an ended item's lines from its last post");
        let Some((Ok(held), at)) = rt.state.insights.threads.get(&0) else { panic!("the rows held") };
        let n = per_topic(held, *at);
        let counts = |t: &Topics| t.list.iter().map(|x| (x.line.id.clone(), x.agents.len())).collect::<Vec<_>>();
        let ids = |v: &[(&str, usize)]| v.iter().map(|(id, n)| (id.to_string(), *n)).collect::<Vec<_>>();
        assert_eq!(counts(&n.threads), ids(&[("N-2", 3), ("N-1", 3), ("N-6", 2), ("N-7", 1), ("N-5", 1), ("N-3", 1), ("N-4", 0)]), "the lexer's: fixer, dev on the printer, dev and lead who started the release notes");
        assert_eq!(count_per_topic(held, *at, Some("topic:N-2")).head, "N-2 the lexer · 3 agents: dev, fixer, lead · 3 lines", "the names on its lines' rows");
        assert_eq!((n.threads.agents, n.threads.took), (Some(1), Some(3600)), "the middle of 0, 1, 1, 1, 2, 3, 3; the ended ones' took 1 h, 1 h, 3 h");
        assert_eq!(counts(&n.items), ids(&[("N-8", 2), ("N-9", 1)]));
        assert_eq!((n.items.agents, n.items.took), (Some(1), Some(7200)), "the smaller middle of 1 and 2; tidy the logs took 2 h");
        assert_eq!(by_words(CLOCK), "by a clock, every 1 h", "a clock's period as every duration is written (I-1469 p50)");
        assert_eq!(by_words("dev"), "by dev");
    }

    /// His presses for insight (1): the structure pane → Insights → Work and talk, which opens at Work; Talk → the turns
    /// that talked; an agent's row → its turns; a turn → the Events depth at the post it was given. The numbers drawn
    /// are docket-machine's `work_talk` of coordd's rows.
    #[test]
    fn work_and_talk_opens_from_the_structure_and_a_count_lists_its_turns_down_to_their_events() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        press(&mut rt, "tree-fold-insights");
        press(&mut rt, "tree-insight-work-talk");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens::of(Filter::WorkTalk)), "its lens, at no count");
        // the test's coordd has no link: the ask is made, and not sent
        assert_eq!(rt.state.notice.as_deref(), Some("not sent: the box is not connected"), "its rows asked of coordd");
        rt.update(|s, _| came(s, 0, Insight::WorkTalk, true, turns()));
        rt.invalidate();
        frames(&mut rt);
        let drawn = |rt: &rune::runtime::Runtime<App>| (1..=5).filter(|n| rt.find(&format!("insight-turn-{n}")).is_some()).collect::<Vec<_>>();
        assert!(rt.find("insight-counts").is_some(), "what a work turn is, said");
        assert_eq!(drawn(&rt), vec![2, 5], "Work first: fixer's two");
        press(&mut rt, "insight-talk");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("talk".into()), ..Lens::of(Filter::WorkTalk) }), "the count kept as the lens's node");
        assert_eq!(drawn(&rt), vec![3, 4], "the turns that talked");
        press(&mut rt, "insight-agent:lead");
        assert_eq!(drawn(&rt), vec![1, 3], "lead's, the one from before among them");
        press(&mut rt, "insight-turn-1");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("agent:lead".into()), ..Lens::of(Filter::WorkTalk) }), "a question's delivery is no post: nowhere to zoom");
        press(&mut rt, "insight-model:gpt-6.2-sol");
        assert_eq!(drawn(&rt), vec![5], "the Codex turn");
        press(&mut rt, "insight-hour:2026-11-20T09");
        assert_eq!(drawn(&rt), vec![2, 3], "the hour's");
        press(&mut rt, "insight-turn-3");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1.p3".into()) }), "the turn at the post it was given");
        let Some((Ok(held), _)) = rt.state.insights.turns.get(&0) else { panic!("the rows held") };
        let n = work_talk(held);
        assert_eq!((n.all.work.len(), n.all.talk.len(), n.all.not_known.len(), n.all.share()), (2, 2, 1, Some(0.5)));
    }

    /// A count of thousands builds only the rows in view (proto-window N-1446 p545; proto-lead p547): research's record
    /// has 11,737 turns, all Not known on the day this coordd goes in, and about 1,450 threads. The first rows of the
    /// list are drawn; the last are not built until he scrolls to them.
    #[test]
    fn a_long_count_builds_only_the_rows_in_view() {
        let turns = (1..=3000u64).map(|n| json!({"agent": "lead", "at": "2026-11-20T08:10:00Z", "delivery": format!("N-1.p{n}"), "tools": {}, "model": null, "line": n}).to_string()).collect::<Vec<_>>().join("\n");
        let at = docket_machine::at_secs(chrono::Utc::now().timestamp() - 3600);
        let threads = (1..=1500).map(|n| json!({"id": format!("N-{n}"), "label": "a thread", "kind": "thread", "opened": at, "by": null, "ended": null, "state": "at work", "last": 1}).to_string()).collect::<Vec<_>>().join("\n");
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        rt.update(|s, _| {
            came(s, 0, Insight::WorkTalk, true, turns);
            came(s, 0, Insight::OpenToDone, true, threads);
            s.show_sel(crate::Sel::Lens(Lens { at: Some("not-known".into()), ..Lens::of(Filter::WorkTalk) }));
        });
        rt.invalidate();
        frames(&mut rt);
        let built = (1..=3000).filter(|n| rt.find(&format!("insight-turn-{n}")).is_some()).collect::<Vec<u64>>();
        assert!(built.contains(&3000) && !built.contains(&1) && built.len() < 200, "{} of 3000 built, the newest first: {:?}", built.len(), built.iter().take(3).collect::<Vec<_>>());
        rt.update(|s, _| s.show_sel(crate::Sel::Lens(Lens { at: Some("open".into()), ..Lens::of(Filter::OpenToDone) })));
        rt.invalidate();
        frames(&mut rt);
        let built = (1..=1500).filter(|n| rt.find(&format!("insight-row-N-{n}")).is_some()).count();
        assert!(built > 0 && built < 200, "{built} of 1500 built");
    }

    /// A page is counted once when its rows come, not at each build (proto-lead N-1446 p582): 11,737 turns, as
    /// research's record has, drawn over several builds, are counted once; another count open is counted once more,
    /// and its builds count nothing; the rows coming again are counted again.
    #[test]
    fn a_page_is_counted_when_its_rows_come_and_not_at_each_build() {
        let turns = (1..=11_737u64).map(|n| json!({"agent": "lead", "at": "2026-11-20T08:10:00Z", "delivery": format!("N-1.p{n}"), "tools": {}, "model": null, "line": n}).to_string()).collect::<Vec<_>>().join("\n");
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        let again = turns.clone();
        rt.update(|s, _| {
            came(s, 0, Insight::WorkTalk, true, turns);
            s.show_sel(crate::Sel::Lens(Lens { at: Some("not-known".into()), ..Lens::of(Filter::WorkTalk) }));
        });
        let built = |rt: &mut rune::runtime::Runtime<App>, times: usize| {
            for _ in 0..times {
                rt.invalidate();
                frames(rt);
            }
            rt.state.insights.counted.get()
        };
        assert_eq!(built(&mut rt, 3), 1, "three builds, counted once");
        assert!(rt.find("insight-turn-11737").is_some(), "the newest drawn");
        press(&mut rt, "insight-talk");
        assert_eq!(built(&mut rt, 3), 2, "another count open: counted once more");
        rt.update(|s, _| came(s, 0, Insight::WorkTalk, true, again));
        assert_eq!(built(&mut rt, 2), 3, "the rows came again");
    }

    /// A pane beside the page is a reader of its own (proto-window N-1446 p626): the lens at `talk` with a pane beside
    /// at `work` (YAAIHH_LENS_BESIDE's panes), drawn over three builds, counts each number once, not each at each
    /// build as one page kept per project did.
    #[test]
    fn a_pane_beside_at_another_number_counts_once_too() {
        let turns = (1..=200u64).map(|n| json!({"agent": "lead", "at": "2026-11-20T08:10:00Z", "delivery": format!("N-1.p{n}"), "tools": {}, "model": null, "line": n}).to_string()).collect::<Vec<_>>().join("\n");
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![project()];
        s.cur = 0;
        s.sel = crate::Sel::Tickets;
        let mut rt = crate::view_cost::window(s);
        let at = |n: &str| crate::Sel::Lens(Lens { at: Some(n.into()), ..Lens::of(Filter::WorkTalk) });
        rt.update(|s, _| {
            came(s, 0, Insight::WorkTalk, true, turns);
            s.show_sel(at("talk"));
            s.panes = vec![(0, s.sel_kept()), (0, at("work"))];
            s.focus = 0;
        });
        for _ in 0..3 {
            rt.invalidate();
            frames(&mut rt);
        }
        assert_eq!(rt.state.insights.counted.get(), 2, "two numbers, each counted once over three builds");
    }

    /// His asks on Open → done (paper-research N-1446 p1786): from a coordd that gives them, the lens opens at Nobody
    /// moves, which lists the ask nobody took and the one taken with nothing since, the oldest first, and no thread;
    /// each tile counts what docket-machine's `asks` of coordd's rows counts, In your window saying in red that one
    /// waits for his word; that tile lists both in his window, the one he has not signed off "waits for you" in red,
    /// the other his word; an ask's row zooms to his post's lines at the Events depth. A node read back is its number,
    /// a newer window's `asks-` word Nobody moves.
    #[test]
    fn his_asks_open_at_nobody_moves_and_each_tile_lists_what_it_counts() {
        for n in AskNumber::ALL {
            assert_eq!(AskNumber::of(Some(n.word())), Some(n), "{} read back", n.word());
        }
        assert_eq!((AskNumber::of(Some("asks-a newer word")), AskNumber::of(Some("days")), AskNumber::of(None)), (Some(AskNumber::Stuck), None, None));
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::ASKS_PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        came(&mut s, 0, Insight::OpenToDone, true, rows());
        came(&mut s, 0, Insight::Asks, true, his_asks(None));
        s.show_sel(crate::Sel::Lens(Lens::of(Filter::OpenToDone)));
        assert!(s.projects[0].pending.is_empty(), "both rows held: nothing asked again");
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("insight-asks-counts").is_some(), "what an ask is, said");
        assert!(rt.find("insight-ask-N-1104.p12").is_some() && rt.find("insight-ask-N-2.p3").is_some(), "nobody moves: the one never taken, the one taken with nothing since");
        assert!(rt.find("insight-ask-N-3.p2").is_none() && rt.find("insight-ask-N-1104.p38").is_none(), "not the one at work, not the one he signed off");
        assert!(rt.find("insight-row-N-2").is_none(), "no thread listed");
        assert_eq!(drawn(&rt, "insight-listed").map(|d| d.0), Some("Nobody moves · 2 asks, the longest waiting first".to_string()));
        let Some((Ok(held), at)) = rt.state.insights.asks.get(&0) else { panic!("the asks held") };
        let page = count_asks(held, *at, Some("asks-stuck"));
        let counts: Vec<(&str, usize, bool)> = page.tiles.iter().map(|t| (t.word.as_str(), t.count, t.red)).collect();
        assert_eq!(counts, [("asks-asked", 8, false), ("asks-stuck", 2, false), ("asks-words", 2, false), ("asks-window", 2, true), ("asks-signed", 3, false)], "the ledger's asks, counted, none filtered");
        assert_eq!(page.tiles[3].under, "1 wait for your word");
        assert_eq!(page.tiles[4].under, "1 worth it · 1 fine · 1 not worth it");
        let n = asks(held);
        assert_eq!(page.tiles.iter().map(|t| t.count).collect::<Vec<_>>(), [n.asked.len(), n.stuck.len(), n.words.len(), n.window.len(), n.signed.len()], "the tiles are docket-machine's counts");
        press(&mut rt, "insight-asks-window");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { at: Some("asks-window".into()), ..Lens::of(Filter::OpenToDone) }), "the number kept as the lens's node");
        settled(&mut rt);
        let red = colour(&rt, "1 wait for your word");
        let yours = drawn(&rt, "insight-ask-you-N-1104.p31").expect("the one he has not signed off");
        let other = drawn(&rt, "insight-ask-you-N-1104.p35").expect("the one he signed off");
        assert_eq!((yours.0.as_str(), other.0.as_str()), ("waits for you", "Worth it"), "his word on the other");
        assert!(red.is_some() && yours.1 == red && other.1 != red, "the tile's red, and only on the one that waits for him: {red:?} {:?} {:?}", yours.1, other.1);
        assert!(rt.find("insight-ask-N-1104.p12").is_none());
        press(&mut rt, "insight-ask-N-1104.p31");
        assert_eq!(rt.state.sel, crate::Sel::Lens(Lens { filter: Filter::Threads, depth: Depth::Events, at: Some("N-1104.p31".into()) }), "his post's lines");
    }

    /// His word on an ask is one press (paper-research N-1446 p1786): its `⋯` opens his three words in the row; Worth
    /// it sends one `signoff` of that post and word and closes it, the row saying his word, dimmed, while it is on its
    /// way; once coordd has taken it his asks are asked again, and the rows that come say it, Signed off counting it and
    /// In your window no longer red. Take back sends `none`. A refusal leaves the row as it was and says why. A coordd
    /// before ASKS_PROTOCOL is asked for Open → done's threads only.
    #[cfg(unix)]
    #[test]
    fn his_word_on_an_ask_is_one_press_and_its_row_says_it_once_coordd_has_it() {
        let out = std::env::temp_dir().join(format!("yaaihh-asks-signed-{}", std::process::id()));
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::ASKS_PROTOCOL;
        s.projects = vec![p];
        s.cur = 0;
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        s.show_sel(crate::Sel::Lens(Lens { at: Some("asks-window".into()), ..Lens::of(Filter::OpenToDone) }));
        came(&mut s, 0, Insight::OpenToDone, true, rows());
        came(&mut s, 0, Insight::Asks, true, his_asks(None));
        let mut rt = crate::view_cost::window(s);
        frames(&mut rt);
        assert!(rt.find("insight-sign-N-1104.p31").is_none(), "his words behind the ⋯");
        press(&mut rt, "insight-ask-more-N-1104.p31");
        assert!(rt.find("insight-sign-N-1104.p31/segment0").is_some() && rt.find("insight-unsign-N-1104.p31").is_none(), "three words, nothing to take back yet");
        press(&mut rt, "insight-sign-N-1104.p31/segment0");
        assert_eq!(rt.state.insights.ask_more, None, "the ⋯ closed");
        assert_eq!(rt.state.insights.signing.get(&(0, "N-1104.p31".to_string())).map(String::as_str), Some("worth_it"));
        settled(&mut rt);
        let dim = drawn(&rt, "insight-ask-you-N-1104.p31").expect("his word while on its way");
        assert_eq!(dim.0, "Worth it");
        rt.update(|s, _| signed(s, 0, "N-1104.p31".into(), true, "Recorded.".into()));
        rt.update(|s, _| came(s, 0, Insight::Asks, true, his_asks(Some("worth_it"))));
        rt.invalidate();
        settled(&mut rt);
        assert!(rt.state.insights.signing.is_empty());
        let now = drawn(&rt, "insight-ask-you-N-1104.p31").expect("the record's word");
        let other = drawn(&rt, "insight-ask-you-N-1104.p35").expect("his other word");
        assert_eq!((now.0.as_str(), now.1), ("Worth it", other.1), "the record's word now, as his other is drawn");
        assert_ne!(dim.1, now.1, "dimmed while it was on its way");
        let Some((Ok(held), at)) = rt.state.insights.asks.get(&0) else { panic!("the asks held") };
        let page = count_asks(held, *at, None);
        assert_eq!((page.tiles[3].red, page.tiles[3].under.as_str(), page.tiles[4].count), (false, "built and installed there", 4), "none waits for his word");
        press(&mut rt, "insight-ask-more-N-1104.p31");
        press(&mut rt, "insight-unsign-N-1104.p31");
        rt.update(|s, _| signed(s, 0, "N-1104.p31".into(), false, "coordd: N-1104.p31 is no ask of his".into()));
        assert_eq!(rt.state.notice.as_deref(), Some("coordd: N-1104.p31 is no ask of his"), "a refusal says why");
        assert!(rt.state.insights.signing.is_empty(), "and the row says what it said");
        // an older coordd: Open → done's threads alone
        rt.update(|s, _| {
            s.projects[0].protocol = coord_types::ASKS_PROTOCOL - 1;
            // no answer came for the asks of this test: none is on its way
            s.projects[0].pending.clear();
            s.insights.forget(0, Insight::OpenToDone);
            s.insights.forget(0, Insight::Asks);
            s.show_sel(crate::Sel::Lens(Lens::of(Filter::OpenToDone)));
        });
        rt.state.projects[0].link = None;
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok()).map(|v| v["req"].clone()).collect();
        let _ = std::fs::remove_file(&out);
        assert_eq!(
            sent,
            [
                json!({"op": "insight", "which": "open-to-done"}),
                json!({"op": "insight", "which": "asks"}),
                json!({"op": "signoff", "post": "N-1104.p31", "worth": "worth_it"}),
                json!({"op": "insight", "which": "asks"}),
                json!({"op": "signoff", "post": "N-1104.p31", "worth": "none"}),
                json!({"op": "insight", "which": "open-to-done"}),
            ],
            "one press, one signoff; the asks asked again once taken; none of an older coordd"
        );
    }

    /// An insight lens is kept as a page is (ui.json, a pane, a pin), with the number it lists; a coordd before
    /// INSIGHT_PROTOCOL is not asked, and its page says why.
    #[test]
    fn an_insight_lens_is_kept_with_its_number_and_an_older_coordd_is_not_asked() {
        let l = crate::Sel::Lens(Lens { at: Some("days".into()), ..Lens::of(Filter::OpenToDone) });
        assert_eq!(serde_json::from_str::<crate::Sel>(&serde_json::to_string(&l).unwrap()).unwrap(), l);
        let days = (Whose::His, Number::Days);
        assert_eq!((Number::of(Some("days")), Number::of(None), Number::of(Some("a newer word")), Number::of(Some("agents-a newer word"))), (days, days, days, (Whose::Agents, Number::Days)));
        for whose in [Whose::His, Whose::Agents] {
            for n in Number::ALL {
                assert_eq!(Number::of(Some(&n.node(whose))), (whose, n), "{} read back", n.node(whose));
            }
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::INSIGHT_PROTOCOL - 1;
        s.projects = vec![p];
        s.cur = 0;
        // the test's own folder has no projects.toml: its notice is not this test's
        s.notice = None;
        s.show_sel(l.clone());
        assert_eq!(s.sel, l, "shown at its number");
        assert!(s.projects[0].pending.is_empty() && s.notice.is_none(), "nothing asked of an older coordd");
        assert_eq!(s.sel_kept(), l, "a pane keeps it with its number");
    }

    /// Work and talk's lens is kept with the count it lists; no count, or a word this window does not know, is Work; a
    /// coordd before INSIGHT_PROTOCOL is not asked.
    #[test]
    fn a_work_talk_lens_is_kept_with_its_count_and_an_older_coordd_is_not_asked() {
        let l = crate::Sel::Lens(Lens { at: Some("agent:fixer".into()), ..Lens::of(Filter::WorkTalk) });
        assert_eq!(serde_json::from_str::<crate::Sel>(&serde_json::to_string(&l).unwrap()).unwrap(), l);
        let work = Count::Did(Did::Work);
        assert_eq!((Count::of(None), Count::of(Some("a newer word")), Count::of(Some("talk"))), (work.clone(), work, Count::Did(Did::Talk)));
        for c in [Count::Did(Did::NotKnown), Count::Agent("fixer".into()), Count::Model("gpt-6.2-sol".into()), Count::Model(String::new()), Count::Hour("2026-11-20T09".into())] {
            assert_eq!(Count::of(Some(&c.word())), c, "{} read back", c.word());
        }
        let (_held, mut s) = crate::view_cost::test_app();
        let mut p = project();
        p.protocol = coord_types::INSIGHT_PROTOCOL - 1;
        s.projects = vec![p];
        s.cur = 0;
        s.notice = None;
        s.show_sel(l.clone());
        assert_eq!(s.sel, l, "shown at its count");
        assert!(s.projects[0].pending.is_empty() && s.notice.is_none(), "nothing asked of an older coordd");
        assert_eq!(s.sel_kept(), l, "a pane keeps it with its count");
    }

    /// Stills on made-up rows (STILL_OUT=DIR): the structure pane with Insights unfolded and Open → done at Open for
    /// days, at Ended, and at Open now in the row of those opened by agents and programs (a clock's thread and an item
    /// among them, each named by what opened it); Work and talk at Work, and at fixer's row; Agents per topic at Threads,
    /// at Items, and at the lexer's topic (its thread rows and the item under it, each with its agents); Open → done with
    /// his asks (paper-research N-1446 p1786): at Nobody moves, as it opens; at In your window with the ⋯ of the docs
    /// page's ask open; at Signed off.
    #[test]
    fn insight_stills() {
        let Ok(out) = std::env::var("STILL_OUT") else { return };
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        fn app(filter: Filter, at: Option<&str>) -> App {
            made(filter, at, false)
        }
        // with his asks, as a coordd that gives them answers
        fn made(filter: Filter, at: Option<&str>, asks: bool) -> App {
            let (_held, mut s) = crate::view_cost::test_app();
            // the test's own folder has no projects.toml: none of that in a still
            s.notice = None;
            let mut p = project();
            p.protocol = 0;
            // the open ones of the rows, as the snapshot holds them, for the structure pane
            for (n, label, hours) in [(2, "the lexer", 50), (3, "the printer", 2)] {
                let at = docket_machine::at_secs(chrono::Utc::now().timestamp() - hours * 3600);
                let v = json!({"id": format!("N-{n}"), "n": n, "kind": "thread", "opener": "lead", "ctx": {}, "form": {}, "title": label, "status": {"is": "resting"},
                    "children": [], "program": "", "bound": 1, "spent": 1, "opened_at": at, "given": [], "posts": [{"n": 1, "at": at, "by": "owner", "text": label}], "waits_on": []});
                p.live.tickets.insert(n, serde_json::from_value(v).unwrap());
            }
            s.projects = vec![p];
            s.cur = 0;
            s.streams.open.insert("tree|0|insights".into());
            came(&mut s, 0, Insight::OpenToDone, true, rows());
            came(&mut s, 0, Insight::WorkTalk, true, turns());
            if asks {
                came(&mut s, 0, Insight::Asks, true, his_asks(None));
            }
            s.show_sel(crate::Sel::Lens(Lens { at: at.map(str::to_string), ..Lens::of(filter) }));
            s
        }
        fn asks_open() -> App {
            made(Filter::OpenToDone, None, true)
        }
        fn asks_more() -> App {
            let mut s = made(Filter::OpenToDone, Some("asks-window"), true);
            s.insights.ask_more = Some((0, "N-1104.p31".into()));
            s
        }
        fn asks_signed() -> App {
            made(Filter::OpenToDone, Some("asks-signed"), true)
        }
        fn days() -> App {
            app(Filter::OpenToDone, None)
        }
        fn ended() -> App {
            app(Filter::OpenToDone, Some("ended"))
        }
        fn agents() -> App {
            app(Filter::OpenToDone, Some("agents-open"))
        }
        fn work() -> App {
            app(Filter::WorkTalk, None)
        }
        fn fixer() -> App {
            app(Filter::WorkTalk, Some("agent:fixer"))
        }
        fn topics() -> App {
            app(Filter::AgentsPerTopic, None)
        }
        fn items() -> App {
            app(Filter::AgentsPerTopic, Some("items"))
        }
        fn topic() -> App {
            app(Filter::AgentsPerTopic, Some("topic:N-2"))
        }
        let all = [("insight-days", days as fn() -> App), ("insight-ended", ended), ("insight-agents", agents), ("insight-work-talk", work), ("insight-work-talk-fixer", fixer), ("insight-topics", topics), ("insight-topics-items", items), ("insight-topic", topic), ("insight-asks", asks_open), ("insight-asks-more", asks_more), ("insight-asks-signed", asks_signed)];
        // STILLS=a,b: those alone
        let only: Vec<String> = std::env::var("STILLS").map(|v| v.split(',').map(str::to_string).collect()).unwrap_or_default();
        let all: Vec<_> = all.into_iter().filter(|(name, _)| only.is_empty() || only.iter().any(|o| o == name)).collect();
        for (name, app) in all {
            let script = out.join(format!("{name}.json"));
            std::fs::write(&script, "[]").unwrap();
            let snap = out.join(format!("{name}.png")).display().to_string();
            let script = script.display().to_string();
            let a: [&str; 8] = ["--snap", &snap, "--size", "1600x900", "--at", "1", "--script", &script];
            let spec = rune::AppSpec { families: ("Inter", "ui-monospace"), ..rune::AppSpec::new("yaaihh", (1600.0, 900.0), app, crate::view) };
            rune::App::new(spec).args(a.map(String::from).to_vec()).try_run().unwrap();
        }
    }
}
