//! yaaihh: the owner's window onto the projects (vault designs/yaaihh-2026-11-07.md). A project is one coordd, in a box
//! or on the host; each is read through its coordd's stream (`coordc bridge`, link.rs) and drawn from the project
//! file's views.
//!
//!   yaaihh                     the window, with the projects of ~/.config/yaaihh/projects.toml (YAAIHH_CONFIG: another)
//!   yaaihh --background        the tray icon only (login autostart); closing the window keeps yaaihh running, a
//!                              second start opens the running one's window (tray.rs)
//!   yaaihh --snap F --scene S  a still of the first project's real state (scenes: overview, items, items-board,
//!                              item-detail, runner, runner-detail, commits, decisions, mail, questions, recognised, agent,
//!                              agents, agents-all, split, new-project, …: SCENES below; the agent scene shows
//!                              YAAIHH_AGENT, else the last agent added)
//!   YAAIHH_VIEW_DATA=ID=FILE   with --snap: view ID shows FILE's data instead of its source's (a source not yet live)
//!   YAAIHH_TERM_TEST=COORD [YAAIHH_TERM_AGENT=NAME] yaaihh --snap F --scene term
//!                              a still of agent NAME's (t1's) terminal on its agent page, through `coordc bridge` run
//!                              here on the coord folder COORD (its coordd at COORD/coordd.sock, NAME's holder at
//!                              COORD/pty/NAME.sock), as an unboxed project's pipe is
//!
//! projects.toml (projects.rs):
//!   agent_base = "/path/to/agent-base"     (default $AGENT_BASE; with yaaihh on Windows, its path in WSL)
//!   wsl = "Ubuntu"                         (the WSL distribution the coordds run in: their commands go through
//!                                            wsl.exe; on Windows always, this naming one other than the default)
//!   search = ["~/src"]                       (where a relative file path in a chat is looked for; default nowhere)
//!   [[project]]
//!   name = "My box"
//!   box  = "my-box"
//!   [[project]]
//!   name  = "Scratch"
//!   id    = "scratch"
//!   boxed = false

/// the Accounts page of the native path: the owner's own accounts by tier (registered by main.rs)
pub mod accounts;
mod acts;
mod agents;
mod also;
mod archive;
mod ask;
mod places;
mod chat;
mod circle;
/// the Harnesses page: versions and the model × provider grid per harness (registered by main.rs)
pub mod harness;
mod docket_ls;
mod domain;
mod drawer;
mod edit;
mod ending;
mod elsewhere;
pub mod ext;
mod files;
mod pace;
mod footprint;
mod pdf_view;
mod find;
mod inspect;
mod jobs;
mod link;
pub mod engine_v2;
pub mod machine;
pub mod policy_v2;
pub mod v2;
mod keystamp;
mod kinds;
mod mentions;
mod log;
mod message_box;
mod changed;
mod live;
mod look;
mod panes;
mod paths;
mod policy;
pub mod settings_look;
mod passage;
mod history;
mod code;
mod cold;
mod global;
mod home;
mod home_cards;
mod compute;
mod devices;
mod disk_health;
mod hosts;
mod layers;
mod store;
mod insight;
mod read_place;
mod programs;
mod read_ask;
mod projects;
mod prompt_edit;
// Claude Code's own system prompt as parts: the parts editor's split (accounts-lead N-1446 p3065); pub until the pane reads it
pub mod prompt_parts;
pub mod new_agent_fields;
pub mod prompt_diff;
pub mod prompt_view;
pub mod schedule;
mod outline;
mod outgoing;
mod parts;
mod permission;
mod settings;
mod marks;
/// A thread's margin: the marks standing at its posts, drawn as brackets and dots down its left (0.3 W4).
mod margin;
mod spell;
mod split;
mod suggest;
/// Speech into the message box (N-554): the take, its sound and its recogniser; the box takes what it heard.
pub mod speech;
mod speech_box;
mod desk_marks;
mod sidebar;
mod slow;
mod stamp;
mod stars;
pub mod spend;
mod state;
mod tabs;
mod ran;
mod picture;
mod popup;
mod raw;
mod refs;
mod restart;
mod sent_window;
mod templates;
mod term;
mod runs;
mod seen;
mod threads;
mod texts;
mod ticket;
mod tray;
mod turn;
mod version;
#[cfg(test)]
mod view_cost;
#[cfg(test)]
mod one_stop_tests;
#[cfg(test)]
mod idle_tests;
#[cfg(test)]
mod new_agent_paths;
#[cfg(test)]
mod prompt_view_paths;
mod streams;
mod subthread;
mod viewer;
mod thumbs;
mod views;
mod post;
/// Lenses: a filter over the one pile, its layer and its node, pinned and dropped like a page (N-1104 p599)
mod lens;
/// The Map: one canvas over the record, zoomed continuously, detail by depth (I-1478; N-1104 p715).
mod map;
/// With you: the list of threads narrowed to one agent, beside its chat (N-800, N-873.p14)
mod withyou;

/// Open a web link in his browser, as the window's own links open (`views::open`): on Windows his Windows browser.
/// The gateway's sign-in pages open by it in the Windows window (piece G3, accounts-lead N-831.p18).
pub fn open_link(target: String) {
    views::open(target)
}
mod waits;
mod whereabouts;
/// One way to write a time, and a duration ([`when::span`], which the gateway's Accounts cards write theirs with).
pub mod when;
mod words;
mod work;
mod vocab;
mod zoom;

use coord_types::{Request, Transcript};
use link::{Incoming, Link};
use live::{Conv, Live};
use projects::{NewProject, ProjectConfig};
use rune::{AppSpec, Chrome, Cx, El, KeyEvent, TextEdit, Tx, text};
use rune_kit::shell::{self, PaneState, SidebarToggle, SplitView, Toolbar};
use rune_kit::{Button, ButtonVariant, SortDir, TableState};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Under this many points the window is narrow (App::narrow).
const NARROW_W: f32 = 1000.0;

/// The protocol from which a bridge serves the server's reads (docket-core5's Subject and Harness, N-412 p129):
/// docket-core gives each request's number in the release that brings it (p137, 79e1fa2: Lsp is 2), so this is the
/// release's to set; 3 if it is the next.
pub(crate) const SERVER_READS: u32 = 3;

/// What the main pane shows.
#[derive(Clone, PartialEq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Sel {
    /// A view of the project file, by id; empty: the project's home.
    View(String),
    Mail,
    /// The agents page (agents.rs).
    Agents,
    /// The kinds of ticket the project runs (kinds.rs); the Clerk page's place, so a saved Clerk opens it.
    #[serde(alias = "Clerk")]
    Kinds,
    /// Docket's tickets (tickets.rs); its Waiting on you is the owner's inbox, so a saved Questions page opens it.
    #[serde(alias = "Questions")]
    Tickets,
    /// The project's folders and a file of them (files.rs).
    Files,
    Agent(String),
    /// Agent NAME's session as stored, beside its chat (raw.rs).
    Raw(String),
    /// Extension NAME's page PAGE (ext.rs).
    Ext(String, String),
    /// A lens on the project's one pile (lens.rs): a filter (Threads, With you, a2o, a2a, a search) at a layer and a node.
    Lens(lens::Lens),
    /// The code view of a repository folder on coordd's machine (code.rs).
    Code(String),
}

impl Sel {
    /// Whether this is the Threads page: its own, or a lens drawn as it (lens.rs), its list beside the thread
    /// `tickets.shown`.
    pub fn shows_threads(&self) -> bool {
        match self {
            Sel::Tickets => true,
            Sel::Lens(l) => l.shows_threads(),
            _ => false,
        }
    }

    /// The agent whose With you this page draws, its thread beside the list `withyou.inside`: an agent's page, or a
    /// With you lens (a search within it too, lens.rs).
    pub fn with_you(&self) -> Option<&str> {
        match self {
            Sel::Agent(a) => Some(a),
            Sel::Lens(l) => l.with_you(),
            _ => None,
        }
    }
}

/// What a request was for, so its reply lands in the right place.
pub(crate) enum Pending {
    Page { agent: String, older: bool },
    /// The old line FROM (a transcript's path) beside his system-prompt edit in AGENT's chat, from the cut on
    /// (`Proj::toggle_fold`; path fix 6 point 3).
    Fold { agent: String, from: String },
    Sent,
    /// A note from a chat's composer: what was typed, and the comments a window before Add comment sent at once kept
    /// with it (outgoing.rs `Then::Chat`).
    Mailed { key: (usize, String), typed: String, comments: Vec<ask::Comment> },
    /// A terminal request (term.rs): a refusal is shown in its view.
    Term { agent: String },
    /// A raw pane's lines of AGENT's session (raw.rs): the newer ones, or OLDER; PAGE: asked as coordd's `RawPage`
    /// (the lines themselves), else as `RawSource` (where the log is, read here).
    Raw { agent: String, older: bool, page: bool },
    /// A request extension EXT made ([`Proj::ask`]), with the data it asked with: the reply goes to it.
    Ext { ext: &'static str, data: Box<dyn std::any::Any> },
    /// A chat's image (`chat::image_key`).
    Image { key: String },
    /// The commits before the oldest held (domain.rs "Show older").
    Older,
    /// His own stars on threads (`Request::Find { starred }`), for the Starred row after a window start (stars.rs).
    OwnStars,
    /// Every brain pass of the project (`Request::Tickets` of kind `brain`), for its agents' chats ([`asks_on_pipe_up`]).
    BrainPasses,
    /// A text by its place, for the Tickets page (tickets.rs), by its key there.
    TicketText { key: String },
    /// The kinds, for the Tickets page's open form.
    TicketKinds,
    /// An open, answer or post from the Tickets page: what to clear once taken.
    TicketDone { clear: crate::threads::Cleared },
    /// What the look on the Kinds page asked: its file, or the text-change its Save or way back opened (look.rs).
    Look(crate::look::Asked),
    /// Home's Devices: coordd's reader's devices (devices.rs).
    Devices,
    /// Home's Layers now: coordd's layers (layers.rs).
    Layers,
    /// Home's Store: the store's rows (store.rs).
    Store,
    /// Home's Store: a page of ROW's entries (store.rs).
    StorePage { row: String },
    /// Home's Store: the delete of one finding of the walk, PATH (store.rs `free`).
    StoreFree { path: String },
    /// Home's cold storage: its answer, after a read or one of the owner's acts (cold.rs).
    Cold,
    /// System prompt: project PROJECT's standing texts, each with its file (prompt_view.rs).
    Standing { project: String },
    /// `Request::Given`: agent NAME's given file, for System prompt (prompt_view.rs).
    Given { project: String, name: String },
    /// `Request::Schedule`: project PROJECT's held messages, asked from the box AT (schedule.rs).
    Schedule { project: String, at: Option<(usize, String)>, words: Option<String> },
    /// The Settings page: read, or one set (SET) (policy.rs).
    Settings { set: bool },
    /// The Settings page: the models its provider rows pick from (policy.rs `models_came`).
    SettingsModels,
    /// Overview: project PROJECT's repositories as git says them (home.rs `ask_repo_state`).
    RepoState { project: String },
    /// The search of these words, typed into a list's field (find.rs `typed`).
    Find { q: String },
    /// The record's lines about NODE (`N-1104`, `N-1104.p587`), for a lens's Events and Record layers (lens.rs): its
    /// first page, or the page after line AFTER.
    Record { node: String, after: Option<u64> },
    /// A run of a handler (`Request::Ticket`, its id), for the steps the Map places in the post it ran for (map/deep.rs).
    Run { id: String },
    /// The rows of an insight (`Request::Insight`: (3) open → done, (1) work and talk), for its lens (insight.rs).
    Insight(coord_types::Insight),
    /// His sign-off of his ask POST with his word of worth (`Request::SignOff`, from ASKS_PROTOCOL), pressed in Open →
    /// done's asks row: once taken, the asks are asked again (insight.rs `signed`).
    SignOff { post: String },
    /// The part of the record's graph an ask names (`Request::Graph`), for the graph page (map/lanes.rs).
    Graph(docket_machine::graph_slice::Ask),
    /// An agent's folder (`Request::Graph` with `Ask::folder`), for its rows under its row in the sidebar (sidebar.rs).
    Folder { agent: String },
    /// A system prompt block's text in an agent's folder (`Request::Raw` of its text node ID), for its popup
    /// (sidebar.rs `apart_poll`).
    FolderText { id: String },
    /// What agent AGENT's graph gives apart under OF (sidebar.rs `Folders::requests`, `apart_poll`): the requests of a
    /// session (its node; `Ask::folder` with `Ask::session`) for its popup; its system prompt (`PROMPT`; `Ask::prompt`)
    /// and a version's requests (its text node; `Ask::version`) for System prompt's pane (fix 4).
    Requests { agent: String, of: String },
    /// The part of the graph a template page's ask names (`Request::Graph`), for view VIEW's page (templates.rs).
    Template { view: String },
    /// The labels of the references the app holds (refs.rs).
    Labels,
    /// A commit's subject asked of coordd (ran.rs, on Windows).
    Subject { hash: String },
    /// A file request of the Files page (files.rs, on Windows).
    Files(files::Asked),
    /// A piece of a picture for the viewer (viewer.rs, on Windows).
    Picture { path: std::path::PathBuf, from: u64 },
    /// A path clicked in a chat, found by coordd (paths.rs, on Windows): its `:line[:col]`, and whether it is a picture.
    Located { at: String, image: bool },
    /// A path a message named, asked of coordd before it is drawn as a link (paths.rs `ask_found`).
    Found { key: (String, String, String) },
    /// A change from AGENT's session settings (settings.rs): its model, effort, account, restart or permissions;
    /// coordd's word on when it takes effect.
    Effort { agent: String },
    /// A point set on AGENT's circle (circle.rs): nothing is said of it (the view's point is the news, N-804.p16), but
    /// a refusal, which shows under the session's settings.
    Compact { agent: String },
    /// The owner's New agent, Fork or Rename asked of coordd (protocol 1, projects.rs): its answer, or why it failed,
    /// goes where the dialog waits for it, as roster's last line did through the shell.
    Roster(std::sync::mpsc::Sender<Result<String, String>>),
    /// What the kind and call editor asked the coordd on Windows (edit.rs).
    Edit(edit::Asked),
    /// ACTOR's acts at THING, for the owner's menu (acts.rs).
    Acts { thing: Option<String>, actor: String },
    /// The split asked for box KEY's TEXT (split.rs): its proposal, if the box still holds TEXT.
    Split { key: (usize, String), text: String },
    /// Point I of box KEY's split send (split.rs): taken or refused.
    SplitPoint { key: (usize, String), i: usize },
    /// The box KEY's words cleaned on a pause, its ask N (changed.rs).
    Cleaned { key: (usize, String), n: u64 },
    /// The Agents page's Esc all (agents.rs): the notice says to how many agents it went.
    EscAll,
    /// His writing said for KEY (`App::writing`): what it says next waits for this answer (`App::writing_changed`).
    Writing { key: (usize, String) },
}

pub struct Proj {
    pub name: String,
    /// The box's name, or the host coordd's.
    pub id: String,
    /// The device its server runs on, when not this PC's (projects.toml's `device`, places.rs `Device`): its pipe and
    /// its runs go there by ssh.
    pub device: Option<places::Device>,
    cmd: Vec<String>,
    link: Option<Link>,
    pub live: Live,
    /// Why the pipe is down, while it is.
    pub down: Option<String>,
    /// The protocol its bridge said in its hello (protocol 1, N-412 ask 13); 0 until it says one, and for a bridge from
    /// before protocol 1, which says none.
    pub protocol: u32,
    /// The release its bridge named in its hello (its coordc's folder under releases/), for `older`.
    pub release: Option<String>,
    /// The bridge sent a line this build cannot read (`Incoming::Unread`): the engine is newer than the window.
    pub unread: bool,
    /// Why its bridge and this app cannot speak, from the last hello (N-437): kept over the pipe's end that follows,
    /// and shown without Start (starting its coordd again mends nothing); cleared by a hello that fits.
    pub refused: Option<String>,
    /// In a box (`box.sh`), else on the host (`host-coordd`): how it is brought up (projects.rs `bring_up`).
    pub boxed: bool,
    /// Its bring-up running (at yaaihh's start, or the owner's Start): what it ended with comes here.
    pub starting: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// A fork of one of its agents running (the agent page's Fork): the new agent's name or why it failed.
    pub forking: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    /// The owner's end of one of its spin-offs running (the agent page's Join back, Close or Delete; his walk item 19):
    /// which, and coordd's answer or why it failed.
    pub spin_ending: Option<crate::projects::SpinEnding>,
    /// Why the last bring-up failed.
    pub start_error: Option<String>,
    pub convs: HashMap<String, Conv>,
    pending: HashMap<u64, Pending>,
    /// The `commits` view's data, read once per change, and after it the older ones the page asked for.
    pub commits: Vec<Commit>,
    /// The older commits asked for (`Request::Older`), oldest last; FROM: where the commits held begin (the next
    /// ask's before); PENDING: an ask is out.
    pub older: Vec<Commit>,
    pub older_from: Option<String>,
    pub older_pending: bool,
    /// What the pipe brought and the window has not taken yet (`poll` takes it in batches).
    inbox: Vec<Incoming>,
    /// The labels coordd gave its references (`coord label`, refs.rs).
    pub labels: refs::Held,
}

pub struct App {
    pub projects: Vec<Proj>,
    /// The agent-base checkout (box.sh, host-coordd) and the config file (projects.rs).
    pub base: String,
    pub config_path: String,
    /// Wakes the window from other threads (the pipes, a project's start); none headless.
    pub wake: Option<rune::Waker>,
    /// The agents page: its tab (0 Active, 1 All agents), All agents' table and the sort it shows.
    pub agents_tab: usize,
    /// The Active tab's tile under the pointer: only it shows its Chat/Terminal switch (N-472, actions where the
    /// pointer is).
    pub agents_hover: Option<String>,
    /// The Active tab's tile whose last jobs are open, by its agent's name: a click on its job line, or on Jobs in its
    /// head, opens and closes them (jobs.rs, N-1334).
    pub agents_jobs: Option<String>,
    /// The Agents page's tiles on screen, by (project, agent): only their sessions are read (paper-research
    /// N-472.p5 point 2: not every active agent's at once, each a page read under coordd's lock).
    pub agents_seen: HashSet<(usize, String)>,
    /// The context circle's red mark while he drags it (circle.rs).
    pub circle: circle::Drag,
    pub all_table: TableState,
    pub all_sort: Option<(String, SortDir)>,
    /// The model price table the agents' cost is read from: an extension's ([`ext::Extension::prices`]), read again
    /// while the agents page shows, at most once a minute; else the bundled catalog.
    pub prices: Result<prices::PriceTable, String>,
    prices_at: Instant,
    pub new_project: NewProject,
    /// The tabs and their windows, and the edge menus (tabs.rs).
    pub tabs: tabs::Tabs,
    /// The agent page's Rename dialog (a click on the agent's name in the toolbar).
    pub rename: projects::Rename,
    /// The Agents page's New agent dialog (projects.rs).
    pub new_agent: projects::NewAgent,
    /// His edit of an agent's system prompt on its System prompt pane, and the engine's answer to his last (prompt_edit.rs).
    pub prompt_edit: prompt_edit::PromptEdit,
    /// The standing texts each project's coordd said, each with its file, which System prompt marks (prompt_view.rs).
    pub standing: prompt_view::Standing,
    /// His messages held to go later, as each coordd lists them (schedule.rs).
    pub schedule: schedule::Schedules,
    /// The project's Home's Add a repository dialog (home.rs).
    pub add_repo: home::AddRepo,
    /// The code view's asks and the repository folders it has open (code.rs).
    pub code: code::Code,
    /// Home's Devices: what coordd's reader read, and the window's own machine (devices.rs).
    pub devices: devices::Devices,
    /// Home's Store: the store's rows and the one opened (store.rs).
    pub store: store::Store,
    /// Home's cold storage: targets, rules, the dry run and what it holds (cold.rs).
    pub cold: cold::Cold,
    /// Home's Policy: the Safe · Yolo defaults (home.rs).
    pub settings: policy::Settings,
    pub repo_state: home::RepoStates,
    /// Home and Overview's machine state for the v2 project, on its way back from the reader (machine.rs).
    pub machine: machine::Reader,
    /// Home's Layers now: coordd's layers and those opened (layers.rs).
    pub layers: layers::Layers,
    /// The extensions' states (ext.rs).
    pub ext: ext::States,
    pub cur: usize,
    sidebar: PaneState,
    pub sel: Sel,
    /// The mail being written to each agent, by project and agent (chat.rs: each chat pane has its own).
    pub composers: HashMap<(usize, String), TextEdit>,
    /// The one message box's target chosen in a place (message_box.rs), by project and place; none: the place's default.
    pub box_targets: HashMap<(usize, String), message_box::Target>,
    /// What coordd answered to the last send from a place's box, shown under it.
    pub box_said: HashMap<(usize, String), Result<String, String>>,
    /// The box whose "⋯" menu or "?" line is open, over the chat (message_box.rs; consult-2, N-815.p23).
    pub box_open: Option<(message_box::Target, message_box::Open)>,
    /// His messages being cleaned before they go (words.rs).
    pub words: words::State,
    /// His messages on their way, from the press until the engine's copy is in the snapshot (outgoing.rs).
    pub outgoing: outgoing::State,
    /// speech in the composers: the take running, the line under a box, the model's fetch (speech_box.rs)
    pub speech_box: speech_box::SpeechBox,
    /// The box's words cleaned on a pause, and the changed words (changed.rs).
    pub changed: changed::State,
    /// The ghost at the caret of the box he types in: completions, fixes, the cleaning's held edits (suggest.rs).
    pub suggest: suggest::State,
    /// The chats coordd was told the owner is writing in (`Request::Writing`): while he is, nothing from agents is
    /// given to that agent (paper-research N-101 p1099, 2; p1287). A thread's boxes by `thread:ID`
    /// (`message_box::writing_key`): while he writes there, its posts' hold waits (proto-lead N-1446 p325).
    pub writing: HashSet<(usize, String)>,
    /// Each agent chat's view: the owner's stream, the agents', or the work behind a reply (streams.rs).
    pub streams: streams::Streams,
    /// The drawers open beside agents' chats (drawer.rs).
    pub drawers: drawer::Drawers,
    /// The harness's programs as the Docket draws them, each stack made once (programs/).
    pub programs: programs::State,
    /// With you drawn as threads, while its switch is on (withyou.rs)
    pub withyou: withyou::WithYou,
    /// Each agent chat's rows, kept until they move (chat.rs `items`).
    pub chat_rows: chat::Rows,
    /// What the owner's mails from a composer were made of (the typed text, the comments), by project and mail id,
    /// so a queued one that is edited goes back into the composer as it was written.
    pub drafts: HashMap<(usize, String), (String, Vec<ask::Comment>)>,
    /// The split's other panes (the focused one is `cur` and `sel`), each a project and what it shows: at most three,
    /// so four in all (view; owner 7 Nov night, "side by side, split into 4, like VS Code").
    pub panes: Vec<(usize, Sel)>,
    /// The focused pane's place in `panes` (panes.rs).
    pub focus: usize,
    /// The pinned chats (sidebar.rs).
    pub pins: sidebar::Pins,
    /// The moment he put everything up to then into the Archive, kept in ui.json; the card that does it; the tickets
    /// each project has in it, built again when they change (archive.rs).
    pub archive: Option<archive::Mark>,
    pub archive_card: archive::Card,
    pub archive_cache: std::cell::RefCell<archive::Cache>,
    /// The composer's spelling mode (spell.rs).
    pub spell: spell::Spell,
    /// The answers opened whole under the passage he commented on, by the answer's id (passage.rs).
    pub passage_open: std::collections::HashSet<String>,
    /// His posts whose line under them is opened to each agent's chain, by `T.pN`; `T.pN+` shows every agent (whereabouts.rs).
    pub where_open: HashSet<String>,
    /// The question whose box stands open under it, answered in place: (project, thread id, post, the ask's index)
    /// (threads.rs `drawn`; the owner, N-1104 post 405). Esc closes it; what he wrote stays in its box's place.
    pub answering: Option<(usize, String, u32, usize)>,
    /// Spelling marks over other programs' boxes, on Windows, behind its switch (desk_marks.rs).
    pub desk: desk_marks::DeskMarks,
    /// His messages split by point, behind its switch (split.rs).
    pub split: split::Splits,
    /// An agent being dragged from the sidebar onto the split, and where it would land if released now (the view
    /// works that out from the layout, panes.rs `drop_zone`; a cell, since the view only reads the state).
    pub dragging: Option<panes::AgentDrag>,
    pub drop_at: std::cell::Cell<Option<(usize, panes::Zone)>>,
    /// What the structure pane last revealed (sidebar.rs `reveal`): its project, the thread by its number or the Map's
    /// node by its name (sidebar.rs `Revealed`), the frames left to bring its row into view.
    pub revealed: Option<(usize, sidebar::Revealed, u8)>,
    /// The record's lines asked of coordd for the lenses at the Events and Record layers (lens.rs), by project and node.
    pub records: lens::Records,
    /// The insights' rows asked of coordd for their lenses (insight.rs), by project.
    pub insights: insight::Insights,
    /// The window is narrow (under NARROW_W points, a laptop at scale 2): the sidebar shows its icons and Threads
    /// shows the list or the open ticket, one at a time. Set by the view from the window's laid-out width.
    pub narrow: std::cell::Cell<bool>,
    pub show_closed: bool,
    pub open_mail: Option<String>,
    /// Entries whose long text is shown whole.
    pub expanded: HashSet<String>,
    /// The chats' images by `chat::image_key`, asked of coordd as their entries arrive (chat.rs `ask_images`).
    pub images: HashMap<String, chat::Img>,
    /// The images decoded lately, held so while they show and a little after (chat.rs `image`).
    pub decoded: chat::Decoded,
    /// The chat image shown over the whole window (`chat::image_key`), opened by clicking it; a click or Escape closes it.
    pub viewing: Option<String>,
    /// The viewer's zoom, place and the images opened in it (viewer.rs).
    pub shown: viewer::Shown,
    /// What the Map's deep levels hold: the runs asked, why one did not come, the step whose prompt is being edited
    /// (map/deep.rs).
    pub deep: map::deep::Deep,
    /// The big things opened apart over the window, the newest on top (popup.rs).
    pub popups: Vec<popup::Popup>,
    /// What a model was sent, opened in windows of its own, and the turns whose requests are read to open theirs
    /// (sent_window.rs).
    pub sent: sent_window::Sent,
    /// The pointer is over an element that takes Ctrl+wheel itself (the usage chart's zoom, the Map), so the window does
    /// not zoom.
    pub wheel_taken: bool,
    /// The Map's camera, selection and graph (map/canvas.rs).
    pub map: map::canvas::View,
    /// The graph page's ask, its answer and its camera (map/lanes.rs).
    pub lanes: map::lanes::Lanes,
    pub cloud: map::cloud::Cloud,
    /// The agents' folders asked of coordd and what came of them (sidebar.rs, the file view).
    pub folders: sidebar::Folders,
    /// The template pages' asks and their answers (templates.rs).
    pub templates: templates::Templates,
    /// The window is the active one (true until the system says otherwise): an extension asks what its page shows
    /// again only while it is (the proxy's Usage page, N-476.p47).
    pub window_focused: bool,
    /// The boxes the last view drew, by project and place (message_box.rs `build`; a cell, since the view only reads
    /// the state): his writing is on only in a box drawn (`writing_changed`), as rune keeps an unmounted box's focus.
    pub boxes_drawn: std::cell::RefCell<HashSet<(usize, String)>>,
    /// Decodes them off the frame; set up with the window's waker (a headless scene makes its own).
    pub image_cache: Option<rune::ImageCache>,
    /// The last thing that went wrong (a refused send, say).
    pub notice: Option<String>,
    /// A commit whose place in its agent's session is being looked for (earlier pages are read until it is found).
    pub seek: Option<Commit>,
    /// The entry a commit was found at, marked in the conversation.
    pub found: Option<String>,
    /// The agents' terminals attached (term.rs).
    pub term: term::Terms,
    /// The raw panes' logs (raw.rs).
    pub raw: raw::Raws,
    /// Comments on selected text and answers being written (ask.rs).
    pub notes: ask::Notes,
    /// The kinds whose text is open on the Kinds page.
    pub kinds_open: HashSet<String>,
    /// The domain views' detail panel, filters and layouts (domain.rs).
    pub domain: domain::State,
    /// The Tickets page (tickets.rs).
    pub tickets: crate::threads::Page,
    /// The Files page (files.rs).
    pub files: files::Files,
    /// Where he was reading in each thread and chat, kept across a swap (read_place.rs).
    pub read_places: read_place::Places,
    /// The places he has been, for Back, forward and History (history.rs).
    pub history: history::History,
    /// The window's state as last written to ui.json (state.rs).
    /// A release of the exe beside this one (restart.rs).
    pub restart: restart::Restart,
    keeper: state::Keeper,
    /// The whole window's zoom (zoom.rs).
    pub zoom: zoom::Zoom,
    /// The sidebar closed to its icons (sidebar.rs `rail`), and the width it opens to again.
    pub rail: bool,
    pub rail_from: f32,
    /// The bar's version shows the release's long name (stamp.rs).
    pub version_long: bool,
    /// The top bar's `⋯` menu open.
    pub bar_menu: bool,
    /// The Vocabulary dialog (vocab.rs).
    pub vocab: vocab::Vocab,
    /// His look: dark (true) or light, as he set it in the bar's menu; None follows the system's (0.2 line 28).
    pub dark: Option<bool>,
    /// The teams folded on the Agents page, by (project id, part path) (parts.rs; agents.rs `team_rows`).
    pub closed_parts: HashSet<(String, String)>,
    /// The pin under the pointer beneath Agents, by its row's key: only it shows its Unpin, so at rest the names have
    /// the row's width (sidebar.rs `unpin`).
    pub pin_hover: Option<String>,
    /// A project's context menu, open at a secondary press on its row in the sidebar (projects.rs `menu`).
    pub project_menu: Option<(usize, rune::PointerPress)>,
    /// The menu at a ticket or an agent, from the interpreter's answer (acts.rs).
    pub acts: acts::Acts,
    /// The Project page's tab last shown (panes.rs `project_tabs`): the sidebar's Project opens it again.
    pub project_tab: Option<Sel>,
    /// Each extension's page last shown, by its name: its Project tab opens there.
    pub ext_last: HashMap<String, Sel>,
    /// The pace of the pipes' wakes: for the agents' messages, and for the terminals' output (pace.rs).
    stream: pace::Pacer,
    pty: pace::Pacer,
    /// The window's waker behind each, for the pipes.
    pub stream_wake: Option<rune::Waker>,
    pub pty_wake: Option<rune::Waker>,
    /// The next tick of each period asked for with `wake_every`, on rune's clock (seconds).
    ticks: HashMap<Duration, f64>,
    /// The time (seconds) asked for with `wake_at`, and when rune's clock is to wake for it, till it comes.
    tick_at: Option<(i64, f64)>,
}

/// How soon a page he left with a draft ends his writing (`App::writing_checked`): the poll that would see it runs
/// before the page is drawn, so while he is writing the window wakes once in this to ask again.
const WRITING_EVERY: Duration = Duration::from_millis(500);

/// An agent's commit, from the project's `commits` view (its Agent, Session and Item trailers): the one commit type,
/// the record graph's too (paper-research N-1446 p1370: extended, not a second).
pub use docket_machine::graph_git::Commit;

pub(crate) fn init() -> App {
    let path = projects::config_path();
    let (cfg, mut notice) = match std::fs::read_to_string(&path) {
        Ok(t) => match toml::from_str::<projects::Config>(&t) {
            Ok(c) => (c, None),
            Err(e) => (projects::Config::default(), Some(format!("{path}: {e}"))),
        },
        Err(e) => (projects::Config::default(), Some(format!("{path}: {e}"))),
    };
    // a project whose device is not in the config is left out, and the notice says so
    let (placed, refused) = cfg.placed();
    if notice.is_none() && !refused.is_empty() {
        notice = Some(format!("{path}: {}", refused.join("; ")));
    }
    let base = cfg.agent_base.or_else(|| std::env::var("AGENT_BASE").ok()).unwrap_or_default();
    let _ = paths::SEARCH.set(cfg.search.clone());
    let _ = places::WSL.set(cfg.wsl.clone());
    let projects = placed
        .into_iter()
        .map(|(p, device)| Proj { device: device.clone(), ..Proj::new(&p, projects::pipe(&base, &p, device.as_ref())) })
        .collect();
    let (all_table, all_sort) = agents::table_state();
    let mut s = App {
        projects,
        base,
        config_path: path,
        pins: sidebar::Pins::load(),
        archive: None,
        archive_card: Default::default(),
        archive_cache: Default::default(),
        spell: spell::Spell::load(),
        passage_open: Default::default(),
        where_open: Default::default(),
        answering: None,
        desk: if rune::is_headless() { Default::default() } else { desk_marks::DeskMarks::load() },
        split: if rune::is_headless() { Default::default() } else { split::Splits::load() },
        wake: None,
        agents_tab: 0,
        agents_hover: None,
        agents_jobs: None,
        agents_seen: HashSet::new(),
        circle: circle::Drag::default(),
        all_table,
        all_sort,
        images: HashMap::new(),
        decoded: Default::default(),
        viewing: None,
        popups: Vec::new(),
        sent: Default::default(),
        shown: Default::default(),
        deep: Default::default(),
        wheel_taken: false,
        map: Default::default(),
        lanes: Default::default(),
        cloud: Default::default(),
        folders: Default::default(),
        templates: Default::default(),
        window_focused: true,
        boxes_drawn: Default::default(),
        image_cache: None,
        prices: prices::PriceTable::bundled(),
        prices_at: Instant::now(),
        new_project: NewProject::default(),
        rename: projects::Rename::default(),
        prompt_edit: prompt_edit::PromptEdit::default(),
        standing: prompt_view::Standing::default(),
        schedule: schedule::Schedules::default(),
        new_agent: projects::NewAgent::default(),
        add_repo: home::AddRepo::default(),
        code: code::Code::default(),
        devices: devices::Devices::default(),
        store: store::Store::default(),
        cold: cold::Cold::default(),
        settings: policy::Settings::default(),
        repo_state: home::RepoStates::default(),
        machine: machine::Reader::default(),
        layers: layers::Layers::default(),
        tabs: tabs::Tabs::default(),
        ext: ext::States::default(),
        cur: 0,
        sidebar: PaneState::new(220.0).range(crate::sidebar::RAIL_W, 520.0),
        sel: Sel::View(String::new()),
        composers: HashMap::new(),
        box_targets: HashMap::new(),
        box_said: HashMap::new(),
        box_open: None,
        words: Default::default(),
        outgoing: Default::default(),
        speech_box: Default::default(),
        changed: Default::default(),
        suggest: Default::default(),
        writing: HashSet::new(),
        streams: Default::default(),
        drawers: Default::default(),
        programs: Default::default(),
        withyou: Default::default(),
        chat_rows: Default::default(),
        drafts: HashMap::new(),
        panes: Vec::new(),
        dragging: None,
        drop_at: std::cell::Cell::new(None),
        revealed: None,
        records: Default::default(),
        insights: Default::default(),
        focus: 0,
        show_closed: false,
        open_mail: None,
        expanded: HashSet::new(),
        notice,
        seek: None,
        found: None,
        term: term::Terms::default(),
        raw: raw::Raws::default(),
        notes: ask::Notes::load(),
        kinds_open: HashSet::new(),
        domain: domain::State::default(),
        tickets: crate::threads::Page::default(),
        files: files::Files::for_window(),
        read_places: read_place::Places::default(),
        history: history::History::default(),
        keeper: state::Keeper::default(),
        restart: restart::Restart::default(),
        zoom: zoom::Zoom::default(),
        bar_menu: false,
        vocab: vocab::Vocab::default(),
        dark: None,
        closed_parts: HashSet::new(),
        pin_hover: None,
        project_menu: None,
        acts: Default::default(),
        narrow: Default::default(),
        rail: false,
        rail_from: 220.0,
        version_long: false,
        project_tab: None,
        ext_last: HashMap::new(),
        stream: pace::Pacer::new(pace::STREAM_EVERY),
        pty: pace::Pacer::new(pace::PTY_EVERY),
        stream_wake: None,
        pty_wake: None,
        ticks: HashMap::new(),
        tick_at: None,
    };
    for e in ext::all() {
        (e.init)(&mut s);
    }
    s.read_prices();
    if !rune::is_headless() {
        state::restore(&mut s);
    }
    restart::init(&mut s);
    s
}

fn setup(s: &mut App, wake: rune::Waker) {
    let headless = rune::is_headless();
    if headless && !live() {
        return; // the scenes read the box once, synchronously
    }
    s.image_cache = Some(rune::ImageCache::new(wake.clone()));
    s.wake = Some(wake.clone());
    if !headless {
        desk_marks::setup(s);
    }
    let _ = viewer::WAKE.set(wake.clone());
    marks::set_waker(wake.clone());
    if s.spell.on {
        marks::warm();
    }
    let (stream_wake, pty_wake) = (s.stream.waker(wake.clone()), s.pty.waker(wake.clone()));
    (s.stream_wake, s.pty_wake) = (Some(stream_wake.clone()), Some(pty_wake.clone()));
    // a take's words wake the window to take them (speech.rs)
    speech::set_waker(wake.clone());
    // a live headless run starts no coordd and keeps nothing up: its coordd is the runner's
    if !headless {
        // from Windows: the WSL side kept up once the window closes (projects.rs `hold`)
        projects::hold(&s.base);
        // each project brought up (after a reboot nothing else starts a box's steps, its coordd or its agents, 9 Nov)
        for i in 0..s.projects.len() {
            projects::bring_up(s, i);
        }
    }
    for p in &mut s.projects {
        p.link = Some(Link::start(p.cmd.clone(), stream_wake.clone(), pty_wake.clone()));
    }
    for e in ext::all() {
        (e.setup)(s, wake.clone());
    }
}

impl App {
    /// Ask for a poll every PERIOD while this is called from `poll`. One timer is asked for per tick, when the tick
    /// is set, not on each poll: rune keeps every distinct deadline, so `wake_after(PERIOD)` on each poll made each
    /// frame ask for one more frame PERIOD later, and the window kept drawing at whatever rate it drew PERIOD ago
    /// (60 frames a second with nothing changing, frames.jsonl 11 Nov). The frame log's timers name the caller's line,
    /// not this one. The tick is kept on rune's clock (`Tx::clock`, the poll's), which rune counts the wake from: kept
    /// on the wall's, read later in the poll, the wake could come before it and no tick be asked again (proto-window,
    /// N-1468 p15).
    #[track_caller]
    pub fn wake_every(&mut self, period: Duration, tx: &mut Tx) {
        self.wake_every_at(period, tx.clock(), tx);
    }

    /// [`App::wake_every`] at rune's CLOCK.
    #[track_caller]
    fn wake_every_at(&mut self, period: Duration, clock: f64, tx: &mut Tx) {
        if self.ticks.get(&period).is_none_or(|next| *next <= clock) {
            self.ticks.insert(period, clock + period.as_secs_f64());
            tx.wake_after(period);
        }
    }

    /// `wake_every` for a time of its own, not a period: a poll at AT (seconds), the soonest time asked, once for each
    /// time, by the same `wake_after`: words that change at that time with nothing sent then, as a held agent's
    /// "its turn again at 05:11" (`agents::next_reset`, proto-lead N-1397 p167). True when the time asked before has
    /// come (NOW, seconds), so the poll builds the view again: a wake is a frame, not a new view (`Tx::wake_after`).
    /// rune counts the wait from its CLOCK, the poll's, read before NOW: a wake can come before AT by the poll's own
    /// time. One that came (CLOCK past its deadline) with AT still ahead is asked again (consult-3 N-1468 p11).
    pub fn wake_at(&mut self, at: Option<i64>, now: i64, clock: f64, tx: &mut Tx) -> bool {
        let came = self.tick_at.is_some_and(|(t, _)| t <= now);
        if came {
            self.tick_at = None;
        }
        let early = self.tick_at.is_some_and(|(_, due)| clock >= due);
        if let Some(at) = at.filter(|t| *t > now && (early || self.tick_at.map(|(t, _)| t) != Some(*t))) {
            let wait = (at - now) as u64;
            self.tick_at = Some((at, clock + wait as f64));
            tx.wake_after(Duration::from_secs(wait));
        }
        came
    }
}

/// The terminals' output, at its own pace (`pace::PTY_EVERY`) rather than with the agents' batch: a key's echo waited
/// up to `pace::STREAM_EVERY` for it (N-412, the owner's Windows window at 13:31: echo read to taken, p90 78 ms of 88). Only
/// the terminals on screen are attached. True when one changed.
fn take_terminals(s: &mut App) -> bool {
    let mut shown = false;
    for i in 0..s.projects.len() {
        if !s.projects[i].inbox.iter().any(|m| matches!(m, Incoming::Pty(_))) {
            continue;
        }
        let (pty, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut s.projects[i].inbox).into_iter().partition(|m| matches!(m, Incoming::Pty(_)));
        s.projects[i].inbox = rest;
        for m in pty {
            shown |= s.take(i, m);
        }
    }
    shown
}

/// What is asked once of each project whose pipe is up (over the live link, nothing waits for the reply), and again
/// once it is back after it went down. A failed ask is not kept as an empty list.
/// - His stars on threads: the Starred row stands for a thread that closed long ago.
/// - Its agents' brain passes, every ticket of kind `brain`, from a coordd at `BRAIN_PROTOCOL` where an agent has a
///   brain: each stands in its agent's chat at the time it spoke (agents.rs `passes`; his N-1104 p625), and one closed
///   more than a day ago is no longer in the snapshot. They are kept with the tickets a fetch has (find.rs `fetched`).
fn asks_on_pipe_up(s: &mut App) {
    for p in &mut s.projects {
        if p.down.is_none() && !s.tickets.find.owned_asked.contains(&p.id) {
            if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&Request::Find { q: String::new(), starred: true, wide: false })) {
                p.pending.insert(id, Pending::OwnStars);
                s.tickets.find.owned_asked.insert(p.id.clone());
            }
        }
        let brains = p.protocol >= coord_types::BRAIN_PROTOCOL && p.live.agents.iter().any(|a| a.brain.is_some());
        if p.down.is_none() && brains && !s.tickets.find.passes_asked.contains(&p.id) {
            if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&Request::Tickets { filter: serde_json::json!({"kind": "brain"}) })) {
                p.pending.insert(id, Pending::BrainPasses);
                s.tickets.find.passes_asked.insert(p.id.clone());
            }
        }
    }
}

pub(crate) fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    let began = Instant::now();
    let mut changed = false;
    // cleared before the pipes are read: a message that arrives after the read wakes the window again
    s.stream.reading();
    s.pty.reading();
    paths::ask_located(s);
    paths::ask_found(s);
    changed |= viewer::drain(s);
    // the pictures posts name, read or asked, and decoded once they came (thumbs.rs)
    changed |= thumbs::poll(s);
    changed |= map::canvas::poll(s, clock, tx);
    // his writing ended by a draft left behind (`App::writing_checked`); while it is on, asked again within WRITING_EVERY
    if s.writing_checked() {
        tx.wake_after(WRITING_EVERY);
    }
    asks_on_pipe_up(s);
    for p in &mut s.projects {
        if let Some(l) = &p.link {
            p.inbox.extend(l.drain());
        }
    }
    // each pipe's batch at its pace; within it, the timer's poll takes it and whatever arrives until then
    let pty = |m: &Incoming| matches!(m, Incoming::Pty(_));
    if s.projects.iter().any(|p| p.inbox.iter().any(pty)) {
        match s.pty.wait() {
            Some(d) => tx.wake_after(d),
            None => {
                s.pty.took();
                if take_terminals(s) {
                    tx.instant();
                    changed = true;
                }
            }
        }
    }
    if s.projects.iter().any(|p| p.inbox.iter().any(|m| !pty(m))) {
        match s.stream.wait() {
            Some(d) => tx.wake_after(d),
            None => {
                s.stream.took();
                // an agent's counters land where they are; what a chat or a page shows of the rest (a new entry, a
                // reply) moves in as before
                let (mut shown, mut lands) = (false, true);
                for i in 0..s.projects.len() {
                    let (held, batch): (Vec<_>, Vec<_>) = std::mem::take(&mut s.projects[i].inbox).into_iter().partition(pty);
                    s.projects[i].inbox = held;
                    for m in batch {
                        let quiet = matches!(&m, Incoming::Event(coord_types::Event { body: coord_types::Body::Agent(_), .. }));
                        if s.take(i, m) {
                            shown = true;
                            lands &= quiet;
                        }
                    }
                }
                if shown && lands {
                    tx.instant();
                }
                changed |= shown;
            }
        }
    }
    changed |= s.new_project_done();
    changed |= projects::brought_up(s);
    changed |= projects::new_agent_ended(s);
    changed |= home::add_repo_ended(s);
    changed |= code::ended(s);
    changed |= code::heard(s);
    changed |= code::keep(s);
    changed |= prompt_edit::update(s);
    // Home's Devices, asked while Home shows (devices.rs)
    changed |= devices::poll(s, tx);
    changed |= tabs::poll(s);
    // his messages being cleaned: the box's judge, the ends; each second for the "being cleaned" line and the look's time
    changed |= words::step(s);
    changed |= outgoing::step(s);
    // his new chat found once its thread is in, and pinned where the old one's pin stands (archive.rs)
    changed |= archive::step(s);
    if words::any(s) {
        s.wake_every(Duration::from_secs(1), tx);
    }
    // the box's words cleaned on a pause, its changes put in, woken for the pause's end once per tick (changed.rs)
    changed::poll(s, tx);
    // whether he types or moved in each box, and the cleaning's edits held on his line put in once he stops (suggest.rs)
    changed |= suggest::poll(s, tx);
    changed |= desk_marks::poll(s);
    changed |= split::poll(s, tx);
    changed |= speech_box::poll(s, tx);
    // the agents page: prices as the usage page last saved them, the tiles' chats, and the "ago" times
    // an agent's page while its working line shows: its "in a turn 40 s", "waiting on its job · … · 6 m" or its
    // job's "compiling 41 of 230 · 2 m" moves every ten seconds (streams.rs, jobs.rs)
    if let Sel::Agent(n) = &s.sel
        && s.projects.get(s.cur).is_some_and(|p| p.live.agents.iter().any(|a| &a.name == n && (streams::now(a).is_some() || jobs::shown(a).is_some())))
    {
        s.wake_every(Duration::from_secs(10), tx);
    }
    // only the chats on screen are read: no session of an agent he has not opened (N-1933.p1)
    s.read_shown();
    // a held agent's row at its reset: "its turn again at 05:11" says the reset has come then, with nothing from
    // coordd at that time (agents.rs `held`, `next_reset`)
    let now_s = chrono::Utc::now().timestamp();
    changed |= s.wake_at(agents::next_reset(s, now_s), now_s, clock, tx);
    if s.sel == Sel::Agents {
        if s.prices_at.elapsed() > Duration::from_secs(60) {
            changed |= s.read_prices();
        }
        if s.agents_tab == 0 {
            s.follow_tiles();
        }
        s.wake_every(Duration::from_secs(10), tx);
    }
    let mut answered = false;
    for e in ext::all() {
        answered |= (e.poll)(s, tx);
    }
    changed |= answered;
    // Windows: the price table is the proxy's answer, held in memory; read again as soon as an answer came, so costs
    // come when the table does, not up to a minute later (N-763.p1)
    if cfg!(windows) && answered {
        s.read_prices();
    }
    changed |= term::follow_page(s);
    changed |= crate::texts::poll(s);
    seen::mark(s);
    changed |= refs::poll(s, tx);
    if let Sel::Ext(n, page) = &s.sel
        && !page.is_empty()
        && s.ext_last.get(n) != Some(&s.sel)
    {
        s.ext_last.insert(n.clone(), s.sel.clone());
    }
    ran::ask_subjects(s);
    changed |= files::poll(s, tx);
    changed |= drawer::poll(s);
    // a page, thread or panel opened since the last frame makes a place (history.rs)
    changed |= history::poll(s);
    read_place::poll(s, tx);
    changed |= raw::poll(s, tx);
    changed |= sent_window::poll(s);
    // words typed into a list's search field, searched when he pauses (find.rs)
    changed |= find::typed(s, tx);
    changed |= find::seek(s, tx);
    changed |= sidebar::reveal(s, tx);
    // the folders of the agents whose rows are unfolded, asked when none is held or the agent moved (sidebar.rs)
    changed |= sidebar::folders_poll(s);
    // what the folders' popups show and coordd has not given yet: a block's text, a session's requests
    changed |= sidebar::apart_poll(s);
    changed |= prompt_view::poll(s);
    changed |= schedule::poll(s);
    // the template page shown: its part of the graph, asked when he opens it or its ask changes (templates.rs)
    changed |= templates::poll(s);
    // an edit of the program, once he pauses: what it would reach, asked of coordd (edit.rs, p294)
    changed |= edit::reach_poll(s, tx);
    // the prompt being edited as it would be sent, once he pressed for it and pauses (edit.rs, H2)
    changed |= edit::dry_poll(s, tx);
    // the look's Save or way back, once its text-change ends (look.rs)
    changed |= look::poll(s);
    if let Some(key) = s.seek_step() {
        tx.scroll_into_view(key.clone());
        s.found = Some(key);
        changed = true;
    }
    s.zoom.show(tx);
    changed |= restart::poll(s, tx);
    state::keep(s, tx);
    if began.elapsed() > Duration::from_millis(100) {
        log::line(&format!("slow update: {} ms", began.elapsed().as_millis()));
    }
    changed
}

impl App {
    pub fn proj(&self) -> Option<&Proj> {
        self.projects.get(self.cur)
    }

    /// Project P's place among the window's: the focused one's, or that of a pane drawn beside it (panes.rs `page_in`).
    pub fn place_of(&self, p: &Proj) -> usize {
        self.projects.iter().position(|q| std::ptr::eq(q, p)).unwrap_or(self.cur)
    }

    /// The price table again from the extension that keeps one (the first), else the bundled catalog; whether one was
    /// read.
    fn read_prices(&mut self) -> bool {
        self.prices_at = Instant::now();
        let Some(f) = ext::all().iter().find_map(|e| e.prices) else { return false };
        self.prices = f(self);
        true
    }

    /// Whether the window shows AGENT of project I's counters (memory, CPU, sizes, tokens, last activity): its page, the
    /// agents page, or an extension's page (which may read any agent's).
    fn shows_counters(&self, i: usize, agent: &str) -> bool {
        std::iter::once((self.cur, &self.sel)).chain(self.panes.iter().enumerate().filter(|(j, _)| *j != self.focus).map(|(_, (p, sel))| (*p, sel))).any(|(p, sel)| match sel {
            Sel::Agents => p == i,
            Sel::Agent(name) => p == i && name == agent,
            Sel::Ext(..) => true,
            _ => false,
        })
    }

    /// Take M from project I's pipe; whether the window may show something else for it. An agent's event that moved
    /// only its counters, while no page shows them, changes nothing in view: the window is not built for it.
    fn take(&mut self, i: usize, m: Incoming) -> bool {
        let p = &mut self.projects[i];
        // the agent as it was, with the event's counters: equal to the event's agent when only counters moved
        let counters_only = match &m {
            Incoming::Event(coord_types::Event { body: coord_types::Body::Agent(a), .. }) if p.down.is_none() => p.live.agents.iter().find(|x| x.name == a.name).is_some_and(|old| {
                let mut old = old.clone();
                (old.mem_mb, old.cpu_s, old.last_active) = (a.mem_mb, a.cpu_s, a.last_active.clone());
                (old.transcript_bytes, old.folder_bytes) = (a.transcript_bytes, a.folder_bytes);
                old.usage = a.usage.clone();
                old == *a
            }),
            _ => false,
        };
        let quiet = counters_only && match &m {
            Incoming::Event(coord_types::Event { body: coord_types::Body::Agent(a), .. }) => !self.shows_counters(i, &a.name),
            _ => false,
        };
        // the chats on screen read on at once; the others when they show again
        let shown = if matches!(&m, Incoming::Event(_)) { self.chats_shown(i) } else { Vec::new() };
        let p = &mut self.projects[i];
        match m {
            Incoming::Event(ev) => {
                p.down = None;
                // a new stream: a whole snapshot, or one resumed after a reconnect (protocol 1), whose terminals and
                // chats are read on as after a snapshot, though the state held stands
                let snapshot = matches!(ev.body, coord_types::Body::Snapshot(_) | coord_types::Body::Resumed { .. });
                // an agent's chat is read on only when its transcript grew (its latest activity moved), not for every
                // change of its view (tokens, memory, state): those came twice a second while agents worked, each a
                // page asked of coordd and the window rebuilt (9 Nov, m151 fix 4)
                let active_before = match &ev.body {
                    coord_types::Body::Agent(a) => p.live.agents.iter().find(|x| x.name == a.name).map(|x| x.last_active.clone()),
                    _ => None,
                };
                let agent = p.apply(ev);
                if snapshot {
                    // a new stream: what was read of each session may be behind. A chat on screen reads on from where
                    // it ends; one not on screen asks its last page (`merge_last`) when it shows again (`read_shown`)
                    let names: Vec<String> = p.convs.keys().cloned().collect();
                    for n in &names {
                        if shown.contains(n) {
                            p.read_newer(n);
                        } else if let Some(c) = p.convs.get_mut(n) {
                            if !c.newer_pending {
                                c.catch_up = true;
                            }
                            c.behind.get_or_insert_with(Instant::now);
                        }
                    }
                }
                // a chat on screen reads on as its session grows; a hidden one reads its last page when it shows again
                let grew = |a: &String| p.live.agents.iter().find(|x| &x.name == a).map(|x| x.last_active.clone()) != active_before;
                if let Some(a) = agent.filter(|a| p.convs.contains_key(a) && grew(a)) {
                    if shown.contains(&a) {
                        p.read_newer(&a);
                    } else if let Some(c) = p.convs.get_mut(&a) {
                        if !c.newer_pending {
                            c.catch_up = true;
                        }
                        c.behind.get_or_insert_with(Instant::now);
                    }
                }
                if snapshot {
                    term::relink(self, i);
                    // what ui.json gave back of its agents' pages, against what the project holds now
                    withyou::check_restored(self, i);
                    // the images of the chats on screen that a pipe's end lost (`Incoming::Down`), asked again
                    for n in &shown {
                        self.ask_images(i, n);
                    }
                }
                self.pins.follow(&self.projects[i].id, &self.projects[i].live.agents);
            }
            Incoming::Pty(ev) => term::take(self, i, ev),
            Incoming::Hello(h) => {
                p.protocol = h.protocol;
                p.release = h.release.clone();
                // the bridge ends after a refusal; this says why, where the pipe's state is shown
                p.refused = h.refused.clone().or_else(|| coord_types::Hello::mismatch(&h)).map(|why| {
                    crate::log::line(&format!("{}: the bridge's hello: {why}", p.name));
                    format!("coordd's bridge and this app cannot speak ({why}): {}", update_which(&h))
                });
                if p.refused.is_some() {
                    p.down = p.refused.clone();
                }
            }
            Incoming::Reply(id, r) => match p.pending.remove(&id) {
                Some(Pending::Roster(tx)) => {
                    let _ = tx.send(if r.ok { Ok(r.out) } else { Err(r.out) });
                }
                Some(Pending::Page { agent, older }) => {
                    let c = p.convs.entry(agent.clone()).or_default();
                    if older { c.older_pending = false } else { c.newer_pending = false }
                    match serde_json::from_str::<Transcript>(&r.out) {
                        Ok(page) if r.ok => {
                            let wanted = chat::image_requests(&agent, &page, &self.images);
                            if !older && std::mem::take(&mut c.catch_up) {
                                c.merge_last(page);
                            } else if !c.merge(page, older) {
                                p.read_newer(&agent);
                            }
                            for (key, req) in wanted {
                                if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&req)) {
                                    p.pending.insert(id, Pending::Image { key: key.clone() });
                                    self.images.insert(key, chat::Img::Asked);
                                }
                            }
                        }
                        _ => {
                            c.catch_up = false;
                            c.refused = Some(r.out);
                        }
                    }
                }
                // the old line beside his edit (path fix 6 point 3), its images asked as an open chat's are
                Some(Pending::Fold { agent, from }) => {
                    let mut wanted = Vec::new();
                    if let Some(f) = p.convs.get_mut(&agent).and_then(|c| c.folds.get_mut(&from)) {
                        f.conv.newer_pending = false;
                        match serde_json::from_str::<Transcript>(&r.out) {
                            Ok(page) if r.ok => {
                                wanted = chat::image_requests(&agent, &page, &self.images);
                                f.conv.merge(page, false);
                            }
                            _ => f.conv.refused = Some(r.out),
                        }
                    }
                    for (key, req) in wanted {
                        if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&req)) {
                            p.pending.insert(id, Pending::Image { key: key.clone() });
                            self.images.insert(key, chat::Img::Asked);
                        }
                    }
                }
                Some(Pending::Sent) if !r.ok => self.notice = Some(r.out),
                Some(Pending::EscAll) => self.notice = Some(crate::agents::esc_all_said(r.ok, &r.out, chrono::Local::now())),
                Some(Pending::Mailed { key, typed, comments }) => {
                    // his writing to that agent, which waited for this answer (`writing_changed`); said first, so a
                    // notice below is not cleared by its request
                    self.writing_changed(i, &key.1);
                    // his message's bubble says it: sent, or not sent and why (outgoing.rs), not also the notice
                    let said = outgoing::answered(self, &key, r.ok, &r.out);
                    match r.out.split(';').next().filter(|_| r.ok) {
                        Some(id) => {
                            self.drafts.insert((i, id.trim().to_string()), (typed, comments));
                        }
                        None if said => {}
                        None => self.notice = Some(r.out),
                    }
                }
                Some(Pending::Term { agent }) if !r.ok => term::refused(&mut self.term, i, &agent, r.out),
                Some(Pending::Image { key }) => {
                    let img = chat::take_image(r.ok, &r.out);
                    self.images.insert(key, img);
                }
                Some(Pending::OwnStars) if r.ok => {
                    self.tickets.find.owned.insert(p.id.clone(), stars::own_of(&r.out));
                }
                Some(Pending::OwnStars) => {}
                // JSON lines of `TicketView`; a line that does not read is left out, as a ticket a fetch did not have
                Some(Pending::BrainPasses) if r.ok => {
                    let held = self.tickets.find.fetched.entry(p.id.clone()).or_default();
                    held.extend(r.out.lines().filter_map(|l| serde_json::from_str::<coord_types::TicketView>(l).ok()).map(|v| (v.n, v)));
                }
                Some(Pending::BrainPasses) => {}
                Some(Pending::Older) => {
                    p.older_pending = false;
                    let page = if r.ok { serde_json::from_str::<serde_json::Value>(&r.out).map_err(|e| format!("older commits: {e}")) } else { Err(r.out) };
                    match page {
                        Ok(v) => {
                            p.older.extend(v.get("commits").and_then(|c| serde_json::from_value::<Vec<Commit>>(c.clone()).ok()).unwrap_or_default());
                            p.older_from = v.get("from").and_then(serde_json::Value::as_str).map(String::from);
                            p.join_older();
                        }
                        Err(e) => self.notice = Some(e),
                    }
                }
                Some(Pending::Ext { ext, data }) => {
                    if let Some(e) = ext::get(ext) {
                        (e.reply)(self, i, data, if r.ok { Ok(r.out) } else { Err(r.out) });
                    }
                }
                Some(Pending::Raw { agent, older, page }) => raw::replied(self, i, agent, older, page, r.ok, r.out),
                Some(Pending::Labels) => p.labels.took(r.ok, &r.out),
                Some(Pending::Acts { thing, actor }) => acts::took(self, i, thing, actor, r.ok, r.out),
                Some(Pending::Edit(what)) => edit::came(self, i, what, r.ok, r.out),
                Some(Pending::Look(what)) => look::came(self, i, what, r.ok, r.out),
                Some(Pending::Devices) => devices::came(self, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Layers) => layers::came(self, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Store) => store::came(self, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::StorePage { row }) => store::paged(self, &row, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Settings { set }) => policy::came(self, set, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::SettingsModels) => policy::models_came(self, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::RepoState { project }) => home::repo_state_came(self, project, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::StoreFree { path }) => store::freed(self, &path, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Cold) => cold::came(self, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Standing { project }) => prompt_view::came(self, project, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Given { project, name }) => prompt_view::came_given(self, project, name, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Schedule { project, at, words }) => schedule::came(self, project, at, words, if r.ok { Ok(r.out) } else { Err(r.out) }),
                Some(Pending::Subject { hash }) => crate::ran::subject_came(&p.id, hash, r.ok, &r.out),
                Some(Pending::Files(what)) => files::replied(self, what, r.ok, r.out),
                Some(Pending::Picture { path, from }) => viewer::replied(self, path, from, r.ok, r.out),
                Some(Pending::Located { at, image }) => paths::located(self, at, image, r.ok, r.out),
                Some(Pending::Found { key }) => paths::found_came(key, r.ok, &r.out),
                Some(Pending::Effort { agent }) => {
                    // every request of the settings clears the line first: what is there is an earlier reply of the
                    // same Apply (model, then effort), and both are said
                    let said = self.streams.said.entry((i, agent)).or_default();
                    if !said.is_empty() {
                        said.push('\n');
                    }
                    said.push_str(&r.out);
                }
                Some(Pending::Compact { agent }) => {
                    if !r.ok {
                        self.streams.said.insert((i, agent), r.out);
                    }
                }
                Some(Pending::Find { q }) => find::came(self, q, r.ok, r.out),
                Some(Pending::Record { node, after }) => lens::came(self, i, node, after, r.ok, r.out),
                Some(Pending::Run { id }) => map::deep::came(self, i, id, r.ok, r.out),
                Some(Pending::Insight(which)) => insight::came(self, i, which, r.ok, r.out),
                Some(Pending::SignOff { post }) => insight::signed(self, i, post, r.ok, r.out),
                Some(Pending::Graph(ask)) => map::cloud::came(self, i, ask, r.ok, r.out),
                Some(Pending::Folder { agent }) => sidebar::folder_came(self, i, agent, r.ok, r.out),
                Some(Pending::FolderText { id }) => sidebar::folder_text_came(self, i, id, r.ok, r.out),
                Some(Pending::Requests { agent, of }) => sidebar::requests_came(self, i, agent, of, r.ok, r.out),
                Some(Pending::Template { view }) => templates::came(self, i, view, r.ok, r.out),
                Some(Pending::Split { key, text }) => split::replied(self, key, text, r.ok, r.out),
                Some(Pending::SplitPoint { key, i }) => split::point_replied(self, key, i, r.ok, r.out),
                Some(Pending::Cleaned { key, n }) => changed::replied(self, key, n, r.ok, r.out),
                // the kinds of a project no longer shown, come after he changed project: not taken as the one's shown,
                // whose own are asked (texts.rs `poll`, paper-research N-1446 p1859)
                Some(Pending::TicketKinds) if self.tickets.kinds_of.as_deref() != Some(p.id.as_str()) => {}
                Some(what @ (Pending::TicketText { .. } | Pending::TicketKinds | Pending::TicketDone { .. })) => crate::threads::replied(self, what, r.ok, r.out),
                // what that box says next, which waited for this answer
                Some(Pending::Writing { key }) => self.writing_changed(i, &key.1),
                _ => {}
            },
            Incoming::Unread => p.unread = true,
            Incoming::Down(why) => {
                // after a refusal the bridge ends: the refusal stays the reason, not the pipe's end
                p.down = Some(p.refused.clone().unwrap_or_else(|| why.clone()));
                p.protocol = 0;
                // what the Map read of it once per pipe: read again once it is back
                map::deep::ended(&mut self.deep, i);
                // a New agent, Fork or Rename asked of coordd: what became of it is not known here. The page's own asks
                // (the kinds, a text's words) are asked again once the pipe is back: build 36's window asked the kinds on
                // the pipe the install's restart of coordd ended, and its Docket said "Reading the kinds…" from then on
                // (N-1446 p198)
                let mut kinds_lost = false;
                let mut pictures = Vec::new();
                for (_, w) in p.pending.drain() {
                    match w {
                        Pending::Roster(tx) => {
                            let _ = tx.send(Err(format!("the pipe to coordd ended before its answer ({why}); look in the agents list for what was done")));
                        }
                        Pending::TicketKinds => kinds_lost = true,
                        Pending::TicketText { key } => {
                            self.tickets.texts.remove(&key);
                        }
                        // a chat's image: asked again once the pipe is back, its chat on screen, or when the chat opens
                        // (`ask_images`); a picture he opened says the pipe ended (viewer.rs `lost`); his box's split,
                        // asked again for the same words (split.rs `lost`) (proto-reader, N-1446 p251)
                        Pending::Image { key } => {
                            if matches!(self.images.get(&key), Some(chat::Img::Asked)) {
                                self.images.remove(&key);
                            }
                        }
                        Pending::Picture { path, .. } => pictures.push(path),
                        Pending::Split { key, text } => split::lost(&mut self.split, &key, &text),
                        // a run the Map asked for its steps: asked again once the pipe is back and its post is near
                        Pending::Run { id } => map::deep::lost(&mut self.deep, i, &id),
                        // his press on an ask: not known to be taken, so its row says what the record said before
                        Pending::SignOff { post } => {
                            self.insights.signing.remove(&(i, post));
                        }
                        // a raw pane's read, or a turn's on the Map: no longer on its way, asked again once it is back
                        Pending::Raw { agent, older, .. } => raw::lost(&mut self.raw, i, &agent, older),
                        // an agent's folder: asked again once the pipe is back, while its row is unfolded
                        Pending::Folder { agent } => sidebar::folder_lost(&mut self.folders, i, &agent),
                        // a block's text, a session's requests or a system prompt a popup or pane shows: asked again
                        // once the pipe is back
                        w @ (Pending::FolderText { .. } | Pending::Requests { .. }) => sidebar::apart_lost(&mut self.folders, i, &w),
                        // a template page's ask: asked again once the pipe is back, while its page is shown
                        Pending::Template { view } => templates::lost(&mut self.templates, i, &view),
                        // System prompt's standing texts and given file: asked again once the pipe is back
                        w @ (Pending::Standing { .. } | Pending::Given { .. }) => prompt_view::lost(&mut self.standing, &w),
                        w @ Pending::Schedule { .. } => schedule::lost(&mut self.schedule, &w),
                        _ => {}
                    }
                }
                // the kinds as the coordd it comes back to reads them: a restart (an install) may bring others; those
                // held are drawn until they come
                if kinds_lost || i == self.cur {
                    self.tickets.kinds_asked = false;
                }
                p.labels.down();
                // a reply to his stars' list that never came: the list is asked again when the pipe is back, and so are
                // the brain's passes, which a coordd restarted since may have more of
                self.tickets.find.owned_asked.remove(&p.id);
                self.tickets.find.passes_asked.remove(&p.id);
                p.convs.values_mut().for_each(|c| {
                    (c.older_pending, c.newer_pending) = (false, false);
                    // an old line on its way beside his edit: folded, read again when he opens it
                    for f in c.folds.values_mut().filter(|f| f.conv.newer_pending) {
                        (f.open, f.conv.newer_pending) = (false, false);
                    }
                });
                term::down(&mut self.term, i);
                // a split's points on their way: what became of them is not known here (split.rs)
                split::down(&mut self.split, &mut self.box_said, i);
                // his messages with the engine and no answer: not sent, Send again beside each (outgoing.rs)
                outgoing::down(self, i);
                for path in pictures {
                    viewer::lost(self, &path);
                }
                // coordd writes his writing off when the bridge that said it ends (core.rs `threads_writing_ended`,
                // `owner_writing`): forgotten here too, and said again as he types
                self.writing.retain(|(pi, _)| *pi != i);
            }
        }
        !quiet
    }

    /// Show agent NAME, reading its session the first time.
    pub fn select_agent(&mut self, name: &str) {
        self.sel = Sel::Agent(name.into());
        let cur = self.cur;
        if let Some(p) = self.projects.get_mut(cur) {
            p.open_conv(name);
        }
    }

    /// Ask for the images of what is held of NAME's session in project I that are neither held nor asked: those a pipe's
    /// end lost.
    pub fn ask_images(&mut self, i: usize, name: &str) {
        let Some(p) = self.projects.get_mut(i) else { return };
        let Some(c) = p.convs.get(name) else { return };
        let wanted: Vec<(String, Request)> = chat::conv_image_requests(name, c).into_iter().filter(|(key, _)| !self.images.contains_key(key)).collect();
        for (key, req) in wanted {
            if let Some(id) = p.link.as_mut().and_then(|l| l.ask(&req)) {
                p.pending.insert(id, Pending::Image { key: key.clone() });
                self.images.insert(key, chat::Img::Asked);
            }
        }
    }

    /// A chat on screen that fell behind while hidden reads on at once, however it came on screen (a press, Back, a pane,
    /// a tile): never held behind the hidden ones (consult-3, N-412 p2113).
    fn read_shown(&mut self) {
        for i in 0..self.projects.len() {
            for n in self.chats_shown(i) {
                if self.projects[i].convs.get(&n).is_some_and(|c| c.behind.is_some()) {
                    self.projects[i].read_on(&n);
                }
            }
        }
    }

    /// The agents of project I whose chat is on screen: the page's or a pane's agent, or a tile seen on the Agents page.
    fn chats_shown(&self, i: usize) -> Vec<String> {
        let mut out = Vec::new();
        for (p, sel) in std::iter::once((self.cur, &self.sel)).chain(self.panes.iter().map(|(p, sel)| (*p, sel))) {
            match sel {
                Sel::Agent(n) if p == i => out.push(n.clone()),
                Sel::Agents if p == i => out.extend(self.agents_seen.iter().filter(|(q, _)| *q == i).map(|(_, n)| n.clone())),
                _ => {}
            }
        }
        out
    }

    /// Show COMMIT's agent at the commit: the entry that names its hash (a `git commit` and its result do), else the
    /// first entry after the commit's time.
    pub fn jump(&mut self, c: Commit) {
        let Some(p) = self.proj() else { return };
        let Some(a) = p.live.agents.iter().find(|a| a.name == c.agent) else {
            self.notice = Some(format!("{} is not an agent of this box", c.agent));
            return;
        };
        if !c.session.is_empty() && !a.transcript.as_deref().is_some_and(|t| t.contains(&c.session)) {
            self.notice = Some(format!("{} was made in an earlier session of {} (session {})", &c.hash[..9.min(c.hash.len())], c.agent, c.session));
        }
        self.found = None;
        self.select_agent(&c.agent.clone());
        self.seek = Some(c);
    }

    /// One step of a seek: the key once the commit's entry is held; else read an earlier page if there is one.
    fn seek_step(&mut self) -> Option<String> {
        let c = self.seek.clone()?;
        let p = self.projects.get_mut(self.cur)?;
        let conv = p.convs.get(&c.agent)?;
        if !conv.loaded {
            return None;
        }
        let short = &c.hash[..10.min(c.hash.len())];
        let by_hash = conv.entries.iter().position(|e| e.text.contains(short) || e.text.contains(&c.hash[..7.min(c.hash.len())]));
        let at = match by_hash {
            Some(i) => Some(i),
            None if conv.start > 0 => {
                if !conv.older_pending {
                    p.read_older(&c.agent);
                }
                return None;
            }
            None => conv.entries.iter().position(|e| e.at >= c.at).or(conv.entries.len().checked_sub(1)),
        };
        self.seek = None;
        let i = at?;
        // a call is folded into its run: open the run and the call (work.rs)
        let open = work::reveal(conv, i, |t| ask::asked(p, t).is_some());
        let key = conv.keys[i].clone();
        self.expanded.extend(open);
        Some(key)
    }

    /// The project's agent commits (its `commits` view), newest first.
    pub fn commits(&self) -> &[Commit] {
        self.proj().map_or(&[], |p| &p.commits)
    }

    /// Send the text written to agent NAME of project PROJECT, after the comments held for its chat each with its
    /// quote (ask.rs `Notes::held`), as one message: a follow-up as a post on its thread, else a `say` (intake). WAIT:
    /// never put into a running turn, given at its end (Schedule's "When its turn ends", schedule.rs).
    pub fn send(&mut self, project: usize, name: &str, wait: bool) {
        let key = (project, name.to_string());
        let Some((mut req, written)) = self.composed(project, name) else { return };
        if wait {
            schedule::set_wait(&mut req);
        }
        // it goes as the box holds it, at once: its words were cleaned in place while he typed (changed.rs), nothing
        // is held to clean them after Send (0.2 B9)
        self.words.unclean.remove(&key);
        // his message stands above the box from the press on (outgoing.rs), its comments' quotes in it
        let comments = self.notes.held.remove(&key).unwrap_or_default();
        let shown = if comments.is_empty() { written.clone() } else { ask::compose(&comments, &written).0 };
        let then = outgoing::Then::Chat { typed: written.clone(), comments: comments.clone() };
        let (out, refused) = outgoing::press(self, key.clone(), &req, &shown, then, None);
        if let Some(o) = self.outgoing.list.iter_mut().find(|o| o.id == out) {
            o.held = comments;
        }
        if let Some(why) = refused {
            // not sent: his message stands as `not sent · why`, Send again beside it, and goes by itself once the pipe
            // is back; said once, there (N-1396 p51)
            outgoing::refused(self, out, why.trim_start_matches("not sent: "));
        }
        self.composers.remove(&key);
        changed::sent(self, &key);
        self.writing_changed(project, name);
        // his words on disk at the press, not half a second later (N-1396 p51)
        crate::state::write_now(self);
    }

    /// The request Send makes of what is written to NAME and the comments held for its chat, with the words typed;
    /// none when neither is there or a tag names nobody (the notice says so).
    pub(crate) fn composed(&mut self, project: usize, name: &str) -> Option<(Request, String)> {
        let key = (project, name.to_string());
        let written = self.composers.get(&key).map(|t| t.text.clone()).unwrap_or_default();
        let held = ask::held(self, project, name);
        if written.trim().is_empty() && held.is_empty() {
            return None;
        }
        match self.request_for(project, name, &written, held) {
            Ok(req) => Some((req, written)),
            Err(e) => {
                if !e.is_empty() {
                    self.notice = Some(e);
                }
                None
            }
        }
    }

    /// The request to agent NAME of project PROJECT that WRITTEN after COMMENTS make; Err: why it cannot go ("" for no
    /// project).
    pub(crate) fn request_for(&self, project: usize, name: &str, written: &str, comments: &[ask::Comment]) -> Result<Request, String> {
        // the chat's agent, and each agent tagged @NAME in the text or its comments (owner m118): the names make the
        // message a thread for sure (design-two-systems §3)
        let notes: Vec<&str> = std::iter::once(written).chain(comments.iter().map(|c| c.note.as_str())).collect();
        let p = self.projects.get(project).ok_or_else(String::new)?;
        let names: Vec<String> = ask::tagged(p, &notes)?.into_iter().filter(|n| n != name).collect();
        let (body, _quotes) = ask::compose(comments, written);
        // a reply belongs to the message it was begun on (the owner, N-234.p7): the first comment's, which goes under that
        // post (or on that ticket) and opens no thread; the passages of other messages are quotes in its text
        let follows = comments.first().and_then(|c| ask::follows_of(p, &c.source));
        // one that follows goes as a post on that ticket, whose `@NAME`s coordd reads itself: a group only from
        // GROUPS_PROTOCOL (0.2 B10)
        if follows.is_some() && ask::group_refused(p, &notes) {
            return Err(ask::NO_GROUP_ON_POST.into());
        }
        // the box's thread (withyou.rs: the one he chose, else the one his last message continues), else a new thread
        // with the agent; a comment it answers or another agent tagged places it themselves. `wait` is false: his
        // message to a working agent waits for its turn's end as every post to it does (the owner, N-1104 p716 "only
        // the turn end"; his yes p923; coordd's delivery.kind, a2ab7686), and nothing in the box chooses it; the field
        // stays for a coordd from before (paper-research N-1446 p2112, 1)
        Ok(match (follows.is_none() && names.is_empty()).then(|| withyou::bound(self, project, name)).flatten() {
            Some(t) => Request::Post { kind: None, t, text: body, to: Some(name.to_string()), answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: ask::data(p, comments) },
            None => {
                let thread = follows.is_none() && names.is_empty();
                Request::Say { to: Some(name.to_string()), names, text: body, follows, wait: false, under: None, label: None, thread, typed: None, split_from: None, comments: ask::data(p, comments) }
            }
        })
    }

    /// Where his writing in the box of PLACE of PROJECT is said (`message_box::writing_key`); the comment box's where
    /// the comment on the passage picked goes (`ask::Picked::target`: a thread, an agent's chat), as its Add comment
    /// sends through that place's Send (ask.rs `send`), so his comment holds that place as its own box does (proto-mind
    /// N-1446 p2359: 11 of his 11 comments on 24 Nov held nothing). None for the other boxes, and for the comment box
    /// with no passage picked in PROJECT.
    pub fn writing_at(&self, project: usize, place: &str) -> Option<String> {
        if place != message_box::COMMENT_PLACE {
            return message_box::writing_key(place);
        }
        let p = self.notes.picked.as_ref().filter(|p| p.pi == project)?;
        message_box::writing_key(&p.target)
    }

    /// Whether the owner is writing in the box of PLACE of PROJECT changed: to an agent, or on a thread, in its own box,
    /// under one of its questions or in a comment on a passage that goes there (`writing_at`). He is writing while such
    /// a box is drawn, holds words and has the keyboard, in the window he is in (`window_focused`), so a draft he leaves
    /// behind, by another page, another box or another program, ends it and the hold it makes (paper-research N-1446
    /// p2189, proto-mind p2138): rune keeps an unmounted box's focus, so the view says which are drawn (`boxes_drawn`),
    /// and the poll asks again each frame (`writing_checked`). coordd is told once each way (`Request::Writing`); a pipe
    /// that is down tells nothing and says nothing. The bridge asks each request on a thread of its own, so two on their
    /// way may reach coordd crossed: what it says next waits for coordd's answer to what it said before, and its `off`
    /// for the answer to his message from that box or his comment going there, which it would otherwise pass: on a
    /// thread, ending the hold before his post is in it (consult-3, N-1446 p309, 2); to an agent, giving it what was
    /// kept for him before his message (consult-3 p442). Each answer comes back here (`take`; threads.rs `replied`).
    pub fn writing_changed(&mut self, project: usize, place: &str) {
        let Some(at) = self.writing_at(project, place) else { return };
        let key = (project, at);
        let ours = |k: &str| self.writing_at(project, k).as_deref() == Some(key.1.as_str());
        let waits = self.projects.get(project).is_some_and(|p| {
            p.pending.values().any(|w| match w {
                Pending::Writing { key: said } | Pending::Mailed { key: said, .. } => *said == key,
                Pending::TicketDone { clear: threads::Cleared::Out(pi, k) | threads::Cleared::Comment(pi, k) } => *pi == project && ours(k),
                _ => false,
            })
        });
        if waits {
            return;
        }
        let on = self.window_focused && {
            let drawn = self.boxes_drawn.borrow();
            self.composers.iter().any(|((pi, k), t)| *pi == project && ours(k) && !t.text.trim().is_empty() && t.focused && drawn.contains(&(*pi, k.clone())))
        };
        if on == self.writing.contains(&key) {
            return;
        }
        let req = match message_box::thread_of(&key.1) {
            Some(t) => Request::Writing { agent: String::new(), on, thread: Some(t.to_string()) },
            None => Request::Writing { agent: key.1.clone(), on, thread: None },
        };
        let notice = self.notice.clone();
        if self.request_in(project, req, Pending::Writing { key: key.clone() }) {
            if on {
                self.writing.insert(key);
            } else {
                self.writing.remove(&key);
            }
        } else {
            self.notice = notice;
        }
    }

    /// His writing asked again where it may have changed with no edit (`writing_changed`): each place it is on, which
    /// a box no longer drawn, the keyboard gone elsewhere or the window left ends, and each box with words and the
    /// keyboard, which coming back to it starts again. The poll's, each frame; true while it is on anywhere, so the
    /// poll wakes the window to ask again: a page left is drawn after the poll that would see it. His notice stays.
    pub fn writing_checked(&mut self) -> bool {
        let mut places: Vec<(usize, String)> = self.writing.iter().cloned().collect();
        places.extend(self.composers.iter().filter(|(_, t)| t.focused && !t.text.trim().is_empty()).map(|(k, _)| k.clone()));
        let notice = self.notice.clone();
        for (pi, place) in places {
            self.writing_changed(pi, &place);
        }
        self.notice = notice;
        !self.writing.is_empty()
    }

    /// Send a mail request through the current project's pipe; false (and a notice) when it is down.
    pub fn mail(&mut self, req: Request) -> bool {
        self.mail_in(self.cur, req)
    }

    /// Send a mail request through project PROJECT's pipe; false (and a notice) when it is down.
    pub fn mail_in(&mut self, project: usize, req: Request) -> bool {
        self.request_in(project, req, Pending::Sent)
    }

    /// Send REQ through project PROJECT's pipe, its reply going to WHAT; false (and a notice) when the pipe is down.
    pub(crate) fn request_in(&mut self, project: usize, req: Request, what: Pending) -> bool {
        // the engine's project has no coordd: his words there go to its window driver
        if v2::at(self) == Some(project) {
            return v2::seat::request(self, &req, what);
        }
        let Some(p) = self.projects.get_mut(project) else { return false };
        match p.link.as_mut().and_then(|l| l.ask(&req)) {
            Some(id) => {
                p.pending.insert(id, what);
                self.notice = None;
                true
            }
            None => {
                self.notice = Some("not sent: the box is not connected".into());
                false
            }
        }
    }
}

impl Proj {
    pub fn new(cfg: &ProjectConfig, cmd: Vec<String>) -> Proj {
        Proj {
            name: cfg.name.clone(),
            id: cfg.id.clone(),
            device: None,
            cmd,
            link: None,
            live: Live::default(),
            down: Some("connecting".into()),
            boxed: cfg.boxed,
            starting: None,
            forking: None,
            spin_ending: None,
            protocol: 0,
            release: None,
            unread: false,
            refused: None,
            start_error: None,
            convs: HashMap::new(),
            pending: HashMap::new(),
            commits: Vec::new(),
            older: Vec::new(),
            older_from: None,
            older_pending: false,
            inbox: Vec::new(),
            labels: refs::Held::default(),
        }
    }

    /// Apply EV to the live state, and read the commits again when their view changed.
    fn apply(&mut self, ev: coord_types::Event) -> Option<String> {
        let commits = match &ev.body {
            coord_types::Body::Snapshot(_) => true,
            coord_types::Body::View(v) => v.id == "commits",
            _ => false,
        };
        let agent = self.live.apply(ev);
        if commits {
            let data = self.live.views.get("commits").and_then(|v| v.data.as_ref());
            if self.older.is_empty() {
                self.older_from = data.and_then(|d| d.get("from")?.as_str()).map(String::from);
            }
            self.commits = data.and_then(|d| d.get("commits")).and_then(|c| serde_json::from_value(c.clone()).ok()).unwrap_or_default();
            self.join_older();
        }
        agent
    }

    /// The commits held: the view's, then the older ones asked for (none twice).
    fn join_older(&mut self) {
        let seen: HashSet<String> = self.commits.iter().map(|c| c.hash.clone()).collect();
        let older: Vec<Commit> = self.older.iter().filter(|c| !seen.contains(&c.hash)).cloned().collect();
        self.commits.extend(older);
    }

    /// Ask this project's coordd REQ for extension EXT, with DATA kept for its reply ([`ext::Extension::reply`]);
    /// false when the pipe is down.
    pub fn ask(&mut self, req: &Request, ext: &'static str, data: Box<dyn std::any::Any>) -> bool {
        let Some(id) = self.link.as_mut().and_then(|l| l.ask(req)) else { return false };
        self.pending.insert(id, Pending::Ext { ext, data });
        true
    }

    /// The bridge's relay to this coord's shared language servers, when the bridge speaks protocol 2 (Request::Lsp;
    /// docket-core's bump, N-412 p137): the hovers' way on Windows.
    pub fn lsp(&self) -> Option<crate::link::LspPipe> {
        self.link.as_ref().filter(|_| self.protocol >= 2).map(|l| l.lsp())
    }

    /// Ask coordd REQ for a dialog (the owner's New agent, Fork or Rename): its answer comes on the receiver, as roster's
    /// last line came through the shell. None when its bridge does not speak protocol 1 (one from before it) or the pipe
    /// is down: the dialog then runs roster through the shell as before.
    pub fn ask_roster(&mut self, req: &Request) -> Option<std::sync::mpsc::Receiver<Result<String, String>>> {
        if self.protocol < 1 {
            return None;
        }
        let id = self.link.as_mut().and_then(|l| l.ask(req))?;
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending.insert(id, Pending::Roster(tx));
        Some(rx)
    }

    /// Ask this project's coordd REQ once and wait for the reply (Err: coordd's refusal, or no reply): for a scene,
    /// which reads before its one frame.
    pub fn ask_once(&self, req: &Request) -> Result<String, String> {
        for m in link::once(&self.cmd, std::slice::from_ref(req)) {
            if let Incoming::Reply(_, r) = m {
                return if r.ok { Ok(r.out) } else { Err(r.out) };
            }
        }
        Err(format!("{}: no reply", self.name))
    }

    /// Ask coordd for the week of commits before those held.
    pub fn ask_older(&mut self) {
        let Some(before) = self.older_from.clone().filter(|_| !self.older_pending) else { return };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&Request::Older { id: "commits".into(), before })) {
            self.pending.insert(id, Pending::Older);
            self.older_pending = true;
        }
    }

    /// Open NAME's chat: its session read the first time; one that fell behind while hidden reads its last page now.
    pub fn open_conv(&mut self, name: &str) {
        let (held, behind) = self.convs.get(name).map_or((false, false), |c| (c.loaded || c.newer_pending, c.behind.is_some()));
        if behind {
            self.read_on(name);
        } else if !held {
            self.read_newer(name);
        }
    }

    /// Read on NAME's chat, held and behind, unless its page is being read: no longer behind once asked.
    fn read_on(&mut self, name: &str) {
        if self.convs.get(name).is_some_and(|c| c.newer_pending) {
            return;
        }
        self.read_newer(name);
        if let Some(c) = self.convs.get_mut(name).filter(|c| c.newer_pending) {
            c.behind = None;
        }
    }

    /// Ask for the part of NAME's session after what is held (all of its last part, the first time).
    fn read_newer(&mut self, name: &str) {
        let c = self.convs.entry(name.into()).or_default();
        if c.newer_pending {
            return;
        }
        let req = Request::Transcript { name: name.into(), before: None, after: (c.loaded && !c.catch_up).then_some(c.next), session: None };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&req)) {
            c.newer_pending = true;
            self.pending.insert(id, Pending::Page { agent: name.into(), older: false });
        }
    }

    /// Open or fold the old line FROM (a transcript's path beside NAME's) under his system-prompt edit in NAME's chat,
    /// read from byte CUT on the first time it opens, and again after a read that failed (path fix 6 point 3,
    /// accounts-lead N-1446 p2651): only of a coordd at TRANSCRIPT_SESSION_PROTOCOL, since one below it reads the
    /// current session whatever is named.
    pub fn toggle_fold(&mut self, name: &str, from: &str, cut: u64) {
        if self.protocol < coord_types::TRANSCRIPT_SESSION_PROTOCOL {
            return;
        }
        let Some(c) = self.convs.get_mut(name) else { return };
        let f = c.folds.entry(from.to_string()).or_default();
        f.open = !f.open;
        if !f.open || f.conv.loaded || f.conv.newer_pending {
            return;
        }
        let req = Request::Transcript { name: name.into(), before: None, after: Some(cut), session: Some(from.into()) };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&req)) {
            f.conv.newer_pending = true;
            f.conv.refused = None;
            self.pending.insert(id, Pending::Fold { agent: name.into(), from: from.into() });
        }
    }

    /// Ask for the page before what is held of NAME's session.
    pub fn read_older(&mut self, name: &str) {
        let Some(c) = self.convs.get_mut(name) else { return };
        if c.older_pending || !c.loaded || c.start == 0 {
            return;
        }
        let req = Request::Transcript { name: name.into(), before: Some(c.start), after: None, session: None };
        if let Some(id) = self.link.as_mut().and_then(|l| l.ask(&req)) {
            c.older_pending = true;
            self.pending.insert(id, Pending::Page { agent: name.into(), older: true });
        }
    }
}

/// The bar's line when a bridge sent what this window cannot read.
const UNREAD: &str = "This window is older than the engine and cannot read all it sends: restart it to update";

/// The window's one line when it is older than a server it speaks with (N-412 post 120): OURS is this build's release
/// (`release.sh build` sets YAAIHH_RELEASE, the same folder name as the coordc it ships beside), SERVERS the releases
/// the bridges named. A name begins with its time (`20261118-111000-abc1234`), so the later sorts after. Nothing
/// for a build without a release (a dev build) or when no bridge named one; nothing restarts the window.
fn older<'a>(ours: Option<&str>, servers: impl Iterator<Item = &'a str>) -> Option<String> {
    let ours = ours.filter(|r| !r.is_empty())?;
    let newest = servers.max()?;
    (newest > ours).then(|| format!("This window is {ours}, the server {newest}: restart it to update"))
}

#[cfg(test)]
mod older_tests {
    #[test]
    fn the_window_says_once_it_is_older_than_a_server() {
        let (old, new) = ("20261118-090000-aaaaaaa", "20261118-111000-bbbbbbb");
        assert_eq!(super::older(Some(old), [old, new].into_iter()).as_deref(), Some("This window is 20261118-090000-aaaaaaa, the server 20261118-111000-bbbbbbb: restart it to update"));
        assert_eq!(super::older(Some(new), [old, new].into_iter()), None, "as new as the newest");
        assert_eq!(super::older(None, [new].into_iter()), None, "a dev build");
        assert_eq!(super::older(Some(old), std::iter::empty()), None, "no bridge named one");
    }
}

// The views built, for the tests that count them per key (view_cost.rs `his_long_message`).
#[cfg(test)]
thread_local! {
    pub(crate) static VIEWS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// Counts the window's views: what a view reads once for all it draws (passage.rs `on_drawn`) is kept for one count.
    static VIEW_GEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The count of the view being built, or of the last one built (the rows rune builds for it after it returns).
pub(crate) fn view_gen() -> u64 {
    VIEW_GEN.with(std::cell::Cell::get)
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    VIEW_GEN.with(|g| g.set(g.get() + 1));
    keystamp::built();
    #[cfg(test)]
    VIEWS.with(|n| n.set(n.get() + 1));
    // the boxes this view draws, each said by `message_box::build` as it draws it (`App::writing_changed`)
    s.boxes_drawn.borrow_mut().clear();
    // the window's own motion, not the system's: with Windows' "Show animations" off (SPI_GETCLIENTAREAANIMATION, his
    // setting) Rune held every loop still, so a working agent's spinner, in the sidebar and on its run's line, stood
    // frozen on his Windows window while the WSL window's turned (his "they hang", N-912.p32; N-1048). Its layout lands
    // at once anyway (`instant_changes` below): what moves is the spinners, hover and press fades and the layer's fades,
    // as in the WSL window
    cx.reduced_motion(false);
    // his look, dark or light, from the bar's menu (zoom.rs); until he picks one, the system's (0.2 line 28)
    let dark = s.dark.unwrap_or_else(|| cx.system_dark());
    // the code view follows the system's look too while he has picked none (code.rs `keep`)
    s.code.system_dark.set(Some(cx.system_dark()));
    // the window's look, as the flavour's look.toml has it (look.rs): drawn from this frame on
    let k = look::kit(s, dark);
    // a selection in the kit's colour, the same on every text of the page it crosses (rune draws a text that sets
    // none in its own grey, beside the posts' kit blue)
    cx.selection_color(k.selection());
    // the window's own buttons (minimise, maximise, close) in the same look: Rune's own choice reads the system's
    cx.caption_color(if dark { rune::Color::WHITE } else { rune::Color::BLACK });
    // the page's name is not on the bar: the sidebar's lit entry says where he is, and a bar on every screen holds
    // only what he came for (paper-research N-101 p773); a pane's name stays in the split's labels (panes::label)
    // narrow or wide, from the window's width as laid out (the first pass reads none and lays out wide)
    s.narrow.set(cx.layout_with("kit-root", |r| r[2] < NARROW_W).unwrap_or(false));
    // where the comment box goes, before the page is drawn: the room it leaves in the flow is the one the box goes in
    ask::decide(s, cx);
    // the focused page is the split's focused pane (panes.rs `area` draws the others in theirs); what is drawn after
    // the split is in none
    s.notes.drawing.set((!s.panes.is_empty()).then_some(s.focus));
    let (_title, main) = panes::page(s, &k);
    // the mouse's back and forward buttons over the page (history.rs)
    let main = history::buttons(panes::area(s, main, &k));
    // the Archive shown: its bar across the top of every page, Back to now and Bring it all back on it (archive.rs)
    let main = match archive::bar(s, &k) {
        Some(b) => rune::col().w_full().h_full().min_h(0.0).child(b).child(rune::col().w_full().grow(1.0).min_h(0.0).child(main)),
        None => main,
    };
    s.notes.drawing.set(None);
    let mut bar = Toolbar::new();
    let mut layer = Vec::new();
    // Back and forward first on the bar, the same spot on every page; History behind the mark between them
    bar = bar.leading(history::arrows(s, &k, &mut layer));
    // find in all threads, from every page: the Threads page's own box, over this one (find.rs, N-925)
    bar = bar.leading(find::mark(&k));
    // where he is, by the page's name (paper-research N-1446 p3207, 2), before Docket and Map, which stay where he
    // knows them (p3226: moving them is his to say); an agent's page says its name after them, where it can be renamed
    let page = panes::page_name(&s.sel);
    if !matches!(s.sel, Sel::Agent(_)) {
        // a faint rule after it, so it reads as where he is, not as one more press beside Docket and Map
        bar = bar.leading(rune::row().gap(12.0).center_y().child(text(page.clone()).font(k.ty.body_medium).color(k.c.text_base).truncate().key("bar-title")).child(rune::el().w(1.0).h(16.0).fill(k.c.border_strong)));
    }
    // the project's Docket, its programs drawn (0.2 line 26): the Kinds page, where it stands over the kinds
    bar = bar.leading(crate::picture::docket_mark(&k));
    // the Map from where he is, the thread shown its card (paper-research N-1446 p2116, 4), where coordd serves the graph
    if let Some(m) = map::canvas::bar_mark(s, &k) {
        bar = bar.leading(m);
    }
    find::over(s, &k, &mut layer);
    if let Sel::Agent(name) = &s.sel {
        let name = name.clone();
        // the way back to the Agents page, shown while one Back reaches it, and that Back: ‹ Agents then Back is the
        // page before the Agents page, not this chat again (the walk's 3, paper-research N-1446 p2120)
        if history::back_is(s, |p| p.sel == Sel::Agents) {
            bar = bar.leading(Button::new("‹ Agents").variant(ButtonVariant::Ghost).on_press(|s: &mut App| history::go(s, true, 1)).build(&k).key("agents-back"));
        }
        // the name in the leading slot, which gives way first: a long name, or a notice beside Fork, cuts it with an
        // ellipsis before the buttons; centred across the band it ran under them ("paper-reseaFork", paper-research
        // N-584.p62, 3)
        bar = bar.leading(projects::rename_title(&name, &k));
        // Spin off, or a spin-off's Join back, Close and Delete (his walk item 19): on the bar, one press away
        for b in agents::spin_presses(s, &name, &k) {
            bar = bar.leading(b);
        }
    }
    if let Some(p) = s.proj().filter(|p| p.down.is_some() && p.down.as_deref() != Some("connecting")) {
        // its coordd is not reached: why, and Start (its box's start steps or its host coordd, then its agents)
        let why = p.start_error.clone().or_else(|| p.down.clone()).unwrap_or_default();
        let i = s.cur;
        bar = bar.trailing(text(format!("{}: {}", p.name, sidebar::first_line(&why))).font(k.ty.small).color(k.c.danger.fg).truncate().max_w(420.0));
        // a bridge that refused this app: its coordd runs, and starting it again mends nothing (N-437)
        if p.refused.is_none() {
            bar = bar.trailing(
            Button::new(if p.starting.is_some() { "Starting…" } else { "Start" })
                .variant(ButtonVariant::Neutral)
                .disabled(p.starting.is_some())
                .on_press(move |s: &mut App| projects::bring_up(s, i))
                .build(&k),
            );
        }
    }
    if let Some(b) = restart::line(s, &k) {
        // an exe of another release beside this one: his press restarts onto it (restart.rs)
        bar = bar.trailing(b);
    } else if s.projects.iter().any(|p| p.unread) {
        // the engine sent what this window cannot read: it keeps what it had (N-447 post 19)
        bar = bar.trailing(text(UNREAD).font(k.ty.small).color(k.c.text_muted).truncate().max_w(420.0));
    } else if let Some(note) = older(option_env!("YAAIHH_RELEASE"), s.projects.iter().filter_map(|p| p.release.as_deref())) {
        bar = bar.trailing(text(note).font(k.ty.small).color(k.c.text_muted).truncate().max_w(420.0));
    }
    if let Some(n) = &s.notice {
        // a failure of the window's own: one plain line in support level, its words whole on the line it stands in
        // (window-rules §2: red only when he must act; the down coordd above, with Start, is that)
        let line = accounts::plain(sidebar::first_line(n).as_str());
        bar = bar.trailing(text(line).font(k.ty.small).color(k.c.text_muted).truncate().max_w(420.0).help(accounts::plain(n)));
    }
    // zoom and Split, folded into one menu, last on the bar
    bar = bar.trailing(zoom::menu(s, &k, &mut layer));
    // which version this window is, at the bar's right end on every page (the owner, N-1054 post 1; stamp.rs)
    bar = bar.trailing(stamp::line(s, &k));
    projects::dialog(s, &k, &mut layer);
    projects::rename_dialog(s, &k, &mut layer);
    spell::dialog(s, &k, &mut layer);
    vocab::dialog(s, &k, &mut layer);
    archive::card(s, &k, &mut layer);
    threads::foot_layer(s, &k, &mut layer);
    chat::more_layer(s, &k, &mut layer);
    // the comment box first: the spelling card and the @ suggestions of its box stand over it; a popup's comment box
    // with them after the popups, over the popup it is written in (ask.rs `apart`), inside rune's modal there
    let apart = ask::apart(s);
    if !apart {
        ask::layer(s, &k, &mut layer);
        message_box::card_layer(s, &k, &mut layer);
        message_box::open_layer(s, &k, &mut layer);
    }
    projects::menu(s, &k, &mut layer);
    acts::menu(s, &k, &mut layer);
    acts::anyway(s, &k, &mut layer);
    acts::detail(s, &k, &mut layer);
    projects::new_agent_dialog(s, &k, &mut layer);
    home::add_repo_dialog(s, &k, &mut layer);
    if !apart {
        ask::mentions(s, cx, &k, &mut layer);
    }
    crate::picture::doc_layer(s, &k, &mut layer);
    panes::drop_zone(s, cx, &k, &mut layer);
    // big things opened apart, over the chat they were pressed in (popup.rs)
    popup::layer(s, cx, &k, &mut layer);
    if apart {
        ask::layer(s, &k, &mut layer);
        message_box::card_layer(s, &k, &mut layer);
        message_box::open_layer(s, &k, &mut layer);
        ask::mentions(s, cx, &k, &mut layer);
    }
    // under the bar (rune-kit's toolbar, as last laid out): its arrows stay in reach over a picture (the walk's 7)
    viewer::layer(s, &k, cx.layout_with("toolbar", |r| r[1] + r[3]).unwrap_or(0.0), &mut layer);
    tabs::layer(s, &k, &mut layer);
    circle::layer(s, &k, &mut layer);
    // a narrow window shows the sidebar as its icons, whatever width it was left at
    let mut side = s.sidebar;
    if s.narrow.get() && !s.rail {
        side.set_width(sidebar::RAIL_W);
    }
    let split = SplitView::new(main.key(panes::MAIN))
        .sidebar(&side, |s: &mut App| &mut s.sidebar, sidebar::sidebar(s, &k).cache())
        .toolbar(bar)
        .titlebar_inset(cx.titlebar_inset())
        .frame_inset(8.0)
        .toolbar_in_panel()
        .sidebar_padding(sidebar::SIDE_PAD[0], sidebar::SIDE_PAD[1])
        .sidebar_toggle(SidebarToggle::new().in_header())
        .reduced_motion(cx.is_reduced_motion())
        .build(&k);
    // nothing in the page slides or grows in: every layout change lands at once (rune moves and fades what a commit
    // changes unless told; his frames.jsonl, 11:10-15:26: whole-window redraws in seconds of springs and growth,
    // N-433.p63). Hover and press colours, spinners and scrolling keep their motion; the layer (menus, dialogs,
    // popovers, tooltips, toasts) keeps its fades. The window reads as a page, as a web page does (paper-research N-1446
    // p2987, 3): every text it draws selects, one selection across them all, a post into the next, a heading, a table's
    // cells, a dialog's words; a control's label (rune-kit's controls say so), the bar that moves the window and the
    // Map's canvas, whose drags pan (map/canvas.rs `board`), keep theirs
    rune_kit::root(split.instant_changes(), layer).select_texts(true)
}

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    restart::key(s);
    // the open image's keys first: its Ctrl + and - zoom the image, not the window
    if viewer::key(s, ev) {
        return true;
    }
    if zoom::key(s, ev, tx) {
        return true;
    }
    // under a popup the page takes no keys: Escape is the popup's (rune's modal), the rest its own text's
    if !s.popups.is_empty() {
        return false;
    }
    if history::key(s, ev) {
        return true;
    }
    if find::key(s, ev, tx) {
        return true;
    }
    // the Map's Esc, Enter, + and -, with no field focused
    if tx.focused().is_none() && map::canvas::shown(s) && map::canvas::key(s, &ev.key) {
        return true;
    }
    if shell::is_sidebar_toggle(ev) {
        s.sidebar.toggle();
        return true;
    }
    tx.focused().is_none() && term::page_key(s, ev, tx)
}

/// A scene: the first project's real state, read once.
/// The edge menus' scene: tabs and windows made up, EDGE's menu open.
/// Two tabs: the first with a split of the project's first two agents, an agent working and the Agents page; the
/// second with Questions and Mail. Any project's agents (the README's tutorial films the yaaihh-dev box's demo).
fn tabs_scene(s: &mut App, edge: tabs::Edge) {
    read_once(s, Sel::Agents);
    let agents: Vec<(String, bool)> = s.projects.first().map_or(Vec::new(), |p| p.live.agents.iter().filter(|a| a.kind != "program").map(|a| (a.name.clone(), a.state == "working")).collect());
    let name = |i: usize| agents.get(i).or(agents.first()).map(|a| a.0.clone()).unwrap_or_default();
    let (first, second) = (name(0), name(1));
    let working = agents.iter().find(|a| a.1).map(|a| a.0.clone()).unwrap_or_else(|| second.clone());
    read_once(s, Sel::Agent(first));
    s.split();
    read_once(s, Sel::Agent(second));
    let split = s.layout();
    let one = |sel: Sel| tabs::Layout { cur: 0, sel, panes: Vec::new(), focus: 0 };
    s.tabs.tabs = vec![
        tabs::Tab { windows: vec![split, one(Sel::Agent(working)), one(Sel::Agents)], cur: 0 },
        tabs::Tab { windows: vec![one(Sel::Tickets)], cur: 0 },
    ];
    s.tabs.open = Some(edge);
}

fn tickets_scene(s: &mut App, opening: bool) {
    read_once(s, Sel::Tickets);
    // YAAIHH_LOOK=dark|light: his look from the bar's menu, as ui.json would give it; YAAIHH_BAR_MENU=1: that menu open
    if let Ok(l) = std::env::var("YAAIHH_LOOK") {
        s.dark = Some(l == "dark");
    }
    s.bar_menu = std::env::var("YAAIHH_BAR_MENU").is_ok_and(|x| x == "1");
    // YAAIHH_OUTLINE_OPEN=N-1[,N-2]: those topics of the Threads outline unfolded, as a click on their ▸ leaves them
    for id in std::env::var("YAAIHH_OUTLINE_OPEN").unwrap_or_default().split(',').filter(|x| !x.is_empty()) {
        s.streams.open.insert(format!("outline|0|{id}"));
    }
    // YAAIHH_TICKETS_FILE=F[:F…]: tickets as a view of them was recorded (each file one ticket's view, or a list), over
    // the live ones: a moment of a run that has passed (slow mode's halts)
    // YAAIHH_STREAM=agents|work: the agent chat's view for a still (streams.rs); the owner's stream by default
    if let (Ok(w), Ok(a)) = (std::env::var("YAAIHH_STREAM"), std::env::var("YAAIHH_AGENT")) {
        let which = match w.as_str() {
            "agents" => streams::Which::Agents,
            "work" => streams::Which::Work,
            _ => streams::Which::You,
        };
        s.streams.which.insert((0, a), which);
    }
    if let (Ok(files), Some(p)) = (std::env::var("YAAIHH_TICKETS_FILE"), s.projects.first_mut()) {
        // YAAIHH_TICKETS_ONLY=1: the file's tickets alone (a run whose coordd is gone, drawn while the coord's live
        // coordd still reads the texts)
        if std::env::var("YAAIHH_TICKETS_ONLY").is_ok() {
            p.live.tickets.clear();
        }
        // YAAIHH_AGENTS=A,B: agents standing in for a coord the app does not ask (a record replayed for a still)
        for a in std::env::var("YAAIHH_AGENTS").unwrap_or_default().split(',').filter(|a| !a.is_empty()) {
            let v = serde_json::json!({"id": a, "name": a, "parent": "", "kind": "claude", "model": "", "state": "idle", "since": "",
                "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null});
            match serde_json::from_value::<coord_types::AgentView>(v) {
                Ok(v) if !p.live.agents.iter().any(|x| x.name == v.name) => p.live.agents.push(v),
                Ok(_) => {}
                Err(e) => eprintln!("YAAIHH_AGENTS {a}: {e}"),
            }
        }
        for f in files.split(':') {
            let text = std::fs::read_to_string(f).unwrap_or_default();
            let views: Vec<coord_types::TicketView> = serde_json::from_str(&text).or_else(|_| serde_json::from_str(&text).map(|v| vec![v])).unwrap_or_else(|e| {
                eprintln!("{f}: {e}");
                Vec::new()
            });
            for v in views {
                p.live.tickets.insert(v.n, v);
            }
        }
    }
    // YAAIHH_ARCHIVE=AT[,CHAT]: everything opened before AT in the Archive and his new chat CHAT found, as his press on
    // the card leaves it (archive.rs), for a still of the lists after it
    if let (Ok(v), Some(p)) = (std::env::var("YAAIHH_ARCHIVE"), s.projects.first()) {
        let (at, chat) = v.split_once(',').map_or((v.as_str(), None), |(a, c)| (a, Some(c.to_string())));
        s.archive = Some(archive::Mark { at: at.to_string(), project: p.id.clone(), chat, showing: false });
    }
    // YAAIHH_RELOAD_REPLY=FILE: a reload answered with FILE's text, as the Save's reload takes coordd's answer
    if let Ok(text) = std::env::var("YAAIHH_RELOAD_REPLY").map(|f| std::fs::read_to_string(f).unwrap_or_default()) {
        s.tickets.edits.said = Some(Ok("Saved.".into()));
        edit::replied(s, true, true, &text);
    }
    // YAAIHH_TAKEN=N: the page of a thread whose `take` opened ticket number N, before the step is wired: a thread
    // holding N's first post, the owner's words, with N under it, shown (made up in memory, nothing is written)
    let mut taken = None;
    if let (Some(n), Some(p)) = (std::env::var("YAAIHH_TAKEN").ok().and_then(|n| n.parse::<u64>().ok()), s.projects.first_mut())
        && let Some(c) = p.live.tickets.get(&n).cloned()
    {
        let mut t = c.clone();
        t.n = u64::MAX;
        t.id = "N-0".into();
        t.kind = "thread".into();
        let words = c.form.get("words").and_then(serde_json::Value::as_str).unwrap_or_default().to_string();
        t.title = words.clone();
        t.posts.truncate(1);
        // the owner's words as he typed them, where a change holds them in its form
        if let Some(q) = t.posts.first_mut() {
            q.by = "owner".into();
            q.text = docket_machine::types::Text::New(words);
            q.head = None;
            q.step = None;
        }
        t.children = vec![c.id.clone()];
        t.run = Vec::new();
        t.status = docket_machine::ViewStatus::Resting;
        t.waits_on = Vec::new();
        // opened as the step opens it: after the owner's words
        if let (Some(q), Some(c)) = (t.posts.first(), p.live.tickets.get_mut(&n)) {
            c.opened_at = q.at.clone();
            c.under = Some("N-0".into());
        }
        p.live.tickets.insert(t.n, t);
        taken = Some(u64::MAX);
    }
    let Some(p) = s.projects.first() else { return };
    // YAAIHH_DUMP=N[,N…]: those tickets' views printed to stderr as JSON (what a page is drawn from)
    for n in std::env::var("YAAIHH_DUMP").unwrap_or_default().split(',').filter_map(|n| n.parse::<u64>().ok()) {
        if let Some(v) = p.live.tickets.get(&n) {
            eprintln!("YAAIHH_DUMP {}", serde_json::to_string(v).unwrap_or_default());
        }
    }
    s.tickets.kinds = Some(p.ask_once(&Request::Kinds).and_then(|o| serde_json::from_str(&o).map_err(|e| e.to_string())));
    let shown = taken.or_else(|| std::env::var("YAAIHH_TICKET").ok().and_then(|n| n.parse().ok())).or_else(|| p.live.tickets.values().rev().find(|v| v.parent.is_none()).map(|v| v.n));
    s.tickets.shown = shown;
    // YAAIHH_HALT_EDIT=T:H: halt H of ticket number T with its prompt's edit open, holding YAAIHH_HALT_EDIT_TEXT (else
    // the prompt as it is), as the owner's "edit the prompt" leaves it
    if let Some((t, h)) = std::env::var("YAAIHH_HALT_EDIT").ok().and_then(|e| e.split_once(':').and_then(|(t, h)| Some((t.parse::<u64>().ok()?, h.parse::<u32>().ok()?)))) {
        let prompt = p.live.tickets.get(&t).and_then(|v| v.halts.iter().find_map(|x| match x {
            docket_machine::ViewHalt::Give { n, prompt, .. } if *n == h => Some(prompt.clone()),
            _ => None,
        }));
        let typed = std::env::var("YAAIHH_HALT_EDIT_TEXT").ok().or(prompt).unwrap_or_default();
        s.tickets.slow.editing.insert((t, h), slow::Editing::Prompt);
        s.tickets.slow.text.insert((t, h), TextEdit::new(typed));
    }
    // YAAIHH_RAN_OPEN=K[,K]: the thread's lines K stand open (ran.rs; `N-1.open.P-2`, `P-2.s1`), as clicks leave them
    for k in std::env::var("YAAIHH_RAN_OPEN").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        s.tickets.ran.insert(k.to_string());
    }
    // YAAIHH_PIC_OPEN=K[,K]: the opens K of a program's picture shown open (picture.rs, `pic-…open` keys without
    // `pic-`); YAAIHH_PIC_HIDE: the picture beside the thread hidden
    for k in std::env::var("YAAIHH_PIC_OPEN").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        s.tickets.pic_open.insert(k.to_string());
    }
    // YAAIHH_STAGE=KEY: stage KEY of the ticket shown in its panel (picture/stages.rs), as a click on it leaves it
    if let Ok(key) = std::env::var("YAAIHH_STAGE") {
        s.tickets.pic_open.insert(crate::picture::shown_key(&key));
    }
    let mut sub_scene = None;
    // YAAIHH_SUB=P[:TEXT]: post P of the thread shown under the pointer, its Subthread chosen for the box, TEXT typed
    // (subthread.rs); YAAIHH_ENDING=WHY: the shown thread's end box open, WHY typed (ending.rs)
    if let (Some(n), Ok(spec)) = (shown, std::env::var("YAAIHH_SUB")) {
        let (post, typed) = spec.split_once(':').unwrap_or((spec.as_str(), ""));
        let id = p.live.tickets.get(&n).map(|v| v.id.clone());
        if let (Ok(post), Some(id)) = (post.parse::<u32>(), id) {
            sub_scene = Some((message_box::Target::Thread { project: s.cur, t: id, post: Some(post), way: message_box::Way::Subthread }, (n, post), typed.to_string()));
        }
    }
    if let (Some(n), Ok(why)) = (shown, std::env::var("YAAIHH_ENDING")) {
        s.tickets.more.insert(n);
        s.tickets.ending.insert(n, ending::Ending { how: 0 });
        s.composers.insert((s.cur, ending::place(n)), TextEdit::new(why));
    }
    // YAAIHH_HOW: the thread's "how it runs" open (design §16, 4)
    if let (Some(n), true) = (shown, std::env::var("YAAIHH_HOW").is_ok()) {
        s.tickets.how.insert(n);
    }
    // the open form as "Open a thread of a kind…" (behind the list's ⋯) leaves it (YAAIHH_KIND=K: kind K picked instead), its fields typed
    // from YAAIHH_OPEN_FIELDS=NAME=TEXT[;NAME=TEXT…] (NAME empty: the first post)
    if opening {
        let ks = s.tickets.kinds.as_ref().and_then(|k| k.as_ref().ok());
        let first = ks.and_then(|ks| crate::kinds::owners_kinds(ks).iter().find(|f| f.kind == "change").or(crate::kinds::owners_kinds(ks).first()).map(|f| f.kind.clone()));
        s.tickets.opening = Some(std::env::var("YAAIHH_KIND").ok().or(first).unwrap_or_else(|| "change".into()));
        s.tickets.shown = None;
        for f in std::env::var("YAAIHH_OPEN_FIELDS").unwrap_or_default().split(';').filter(|f| !f.is_empty()) {
            if let Some((n, t)) = f.split_once('=') {
                s.tickets.fields.insert(n.to_string(), TextEdit::new(t));
            }
        }
        return;
    }
    // every delivery unfolded, and each text read
    if let Some(v) = shown.and_then(|n| p.live.tickets.get(&n)) {
        s.tickets.unfolded.extend(v.given.iter().map(|g| g.delivery.clone()));
    }
    if let Some((t, at, typed)) = sub_scene {
        s.tickets.hover_post = Some(at);
        s.composers.insert((s.cur, t.place()), TextEdit::new(&typed));
        message_box::choose(s, t);
    }
    let locs = crate::texts::scene_texts(&s.tickets, shown.and_then(|n| s.projects.first()?.live.tickets.get(&n)), s.projects.first());
    for (key, loc) in locs {
        let r = s.projects.first().map(|p| p.ask_once(&Request::Text { loc })).unwrap_or_else(|| Err("no project".into()));
        s.tickets.texts.insert(key, Some(r));
    }
    // YAAIHH_STEP=T:KEY:NOW:WORKER:CALL[:INLINE]: ticket number T's step at picture key KEY clicked (NOW: 1 if the
    // ticket waits on it), its panel open (edit.rs); YAAIHH_EDIT_TEXT and YAAIHH_EDIT_WHY as on the Kinds page;
    // YAAIHH_ONCE_TREE=1: the panel's wider reach picked
    if let Ok(st) = std::env::var("YAAIHH_STEP") {
        let f: Vec<&str> = st.splitn(6, ':').collect();
        if f.len() >= 5 {
            s.tickets.edits.step = Some(edit::Step { t: f[0].parse().unwrap_or(0), key: f[1].to_string(), awaited: f[2] == "1", worker: f[3].to_string(), call: f[4].to_string(), inline: f.get(5).map(|x| x.to_string()) });
        }
        s.tickets.edits.only_thread = std::env::var("YAAIHH_ONCE_TREE").is_ok_and(|x| x == "1");
        edit::scene_text(s);
    }
    // YAAIHH_RAN_UNFOLD=D[,D]: the prompts of deliveries D opened apart, as a press on their line opens them (popup.rs)
    for d in std::env::var("YAAIHH_RAN_UNFOLD").unwrap_or_default().split(',').filter(|d| !d.is_empty()) {
        let found = s.projects.first().and_then(|p| crate::ran::prompt_of(p, p.live.tickets.get(&shown?)?, d));
        if let Some((title, place)) = found {
            popup::open(s, title, popup::What::Sent { place });
        }
    }
    // YAAIHH_READINGS=FILE,…: the shown ticket's "How its answers were read" asked for, each reading of FILE (e.g.
    // `tickets/forming/2.s1.1.json`) opened apart, the last on top, as presses on their lines open them (popup.rs)
    if let (Ok(files), Some(n)) = (std::env::var("YAAIHH_READINGS"), shown) {
        if let Some(id) = s.projects.first().and_then(|p| p.live.tickets.get(&n)).map(|v| v.id.clone()) {
            let r = s.projects.first().map(|p| p.ask_once(&Request::Forming { t: id })).unwrap_or_else(|| Err("no project".into())).and_then(|o| serde_json::from_str(&o).map_err(|e| e.to_string()));
            s.tickets.forming.insert(n, r);
            for f in files.split(',').filter(|f| !f.is_empty()) {
                let title = s.projects.first().and_then(|p| Some(ticket::reading_title(p, p.live.tickets.get(&n)?, s.tickets.forming.get(&n)?.as_ref().ok()?.iter().find(|r| r.file == f)?)));
                if let Some(title) = title {
                    popup::open(s, title, popup::What::Reading { t: n, file: f.to_string() });
                }
            }
        }
    }
    // YAAIHH_MENTION=ID: a click on the mention of ID (`N-412.p30`, `@NAME`) in a post, as mentions.rs takes it
    if let Ok(id) = std::env::var("YAAIHH_MENTION") {
        mentions::open(s, &format!("yaaihh-ref:{id}"));
    }
    // YAAIHH_OUT=TEXT[|TEXT…]: his messages on their way from the shown thread's foot box (outgoing.rs), the first not
    // sent (its pipe ended before the engine's answer), the others sending…
    if let (Ok(out), Some(id)) = (std::env::var("YAAIHH_OUT"), shown.and_then(|n| s.projects.first()?.live.tickets.get(&n)).map(|v| v.id.clone())) {
        let key = (0, message_box::thread_place(&id));
        for (i, text) in out.split('|').filter(|x| !x.trim().is_empty()).enumerate() {
            let req = Request::Post { kind: None, t: id.clone(), text: text.into(), to: None, answers: None, wait: false, also: Vec::new(), typed: None, split_from: None, on: Vec::new(), comments: Vec::new() };
            let o = outgoing::new(s, key.clone(), &req, text, outgoing::Then::Box);
            outgoing::flying(s, o, &req, None);
            if i == 0 {
                outgoing::down(s, 0);
            }
        }
    }
    crate::threads::retitle_scene(s, 0);
    doc_hover_scene(s);
}

/// YAAIHH_DOC_HOVER=KEY CONTAINER NAME (the tickets and kinds scenes): the pointer on the picture's name at KEY
/// (`doc-…`), a CONTAINER's (`call`, `kind`), its declaration asked of docket-ls and its card shown.
fn doc_hover_scene(s: &mut App) {
    let Some(w) = std::env::var("YAAIHH_DOC_HOVER").ok().map(|h| h.split_whitespace().map(String::from).collect::<Vec<_>>()).filter(|w| w.len() == 3) else { return };
    if let Some(pid) = s.projects.first().map(|p| p.id.clone()) {
        crate::picture::doc_hover(s, &pid, &w[1], &w[2], &w[0], true);
    }
}

/// The scene `tickets-answer`: YAAIHH_TICKET's step answered as the owner's button does, YAAIHH_ANSWER being
/// `RESULT: the text typed`; then the ticket shown as it is after. It answers for real (dev copy only).
fn tickets_answer_scene(s: &mut App) {
    tickets_scene(s, false);
    let (Some(n), Ok(a)) = (s.tickets.shown, std::env::var("YAAIHH_ANSWER")) else { return };
    let (c, typed) = a.split_once(':').map(|(c, t)| (c.trim().to_string(), t.trim().to_string())).unwrap_or((a.trim().to_string(), String::new()));
    match crate::ask::scene_answer(s, n, &c, &typed) {
        Ok(r) => eprintln!("answered: {r}"),
        Err(e) => eprintln!("not answered: {e}"),
    }
    if let Some(p) = s.projects.first_mut() {
        p.live = Live::default();
    }
    s.tickets.texts.clear();
    s.tickets.unfolded.clear();
    tickets_scene(s, false);
}

/// The scene `ticket-once-save`: the ticket scene with YAAIHH_STEP's panel and the edit typed (YAAIHH_EDIT_TEXT,
/// YAAIHH_EDIT_WHY), then its Save pressed: the request the button sends, sent for real (dev copy only), and coordd's
/// answer in the panel.
fn ticket_once_save_scene(s: &mut App) {
    tickets_scene(s, false);
    let Some(st) = s.tickets.edits.step.clone() else { return };
    let Some(id) = s.projects.first().and_then(|p| p.live.tickets.get(&st.t)).map(|v| v.id.clone()) else { return };
    let tree = s.tickets.edits.only_thread;
    let r = edit::once_request(s, &id, &st.call, tree).and_then(|req| s.projects.first().map(|p| p.ask_once(&req)).unwrap_or_else(|| Err("no project".into())));
    match &r {
        Ok(t) => eprintln!("taken: {t}"),
        Err(e) => eprintln!("refused: {e}"),
    }
    let (ok, out) = match r {
        Ok(t) => (true, t),
        Err(e) => (false, e),
    };
    edit::replied(s, false, ok, &out);
    // the ticket read again, as the app's next event brings it: the panel shows the text saved
    let said = s.tickets.edits.said.take();
    if let Some(p) = s.projects.first_mut() {
        p.live = Live::default();
    }
    s.tickets.texts.clear();
    tickets_scene(s, false);
    s.tickets.edits.once = rune::TextEdit::new("");
    s.tickets.edits.why = rune::TextEdit::new("");
    s.tickets.edits.said = said;
}

/// The scene `ticket-halt-save`: the ticket scene with YAAIHH_HALT_EDIT's prompt edit typed, then its "save the edit"
/// pressed: the request the button sends, sent for real (dev copy only); then the ticket read again and shown.
fn ticket_halt_save_scene(s: &mut App) {
    tickets_scene(s, false);
    let Some((t, h)) = std::env::var("YAAIHH_HALT_EDIT").ok().and_then(|e| e.split_once(':').and_then(|(t, h)| Some((t.parse::<u64>().ok()?, h.parse::<u32>().ok()?)))) else { return };
    let Some(id) = s.projects.first().and_then(|p| p.live.tickets.get(&t)).map(|v| v.id.clone()) else { return };
    let r = slow::edit_request(s, &id, t, h, slow::Editing::Prompt, &[]).and_then(|req| s.projects.first().map(|p| p.ask_once(&req)).unwrap_or_else(|| Err("no project".into())));
    match &r {
        Ok(o) => eprintln!("taken: {o}"),
        Err(e) => eprintln!("refused: {e}"),
    }
    if let Some(p) = s.projects.first_mut() {
        p.live = Live::default();
    }
    s.tickets.texts.clear();
    // shown as the app shows it after the save: the edit closed
    unsafe { std::env::remove_var("YAAIHH_HALT_EDIT") };
    s.tickets.slow.editing.remove(&(t, h));
    tickets_scene(s, false);
    s.tickets.said = Some(r);
}

fn thread_comment_scene(s: &mut App, send: bool) {
    tickets_scene(s, false);
    s.tickets.unfolded.clear();
    let Some(v) = s.tickets.shown.and_then(|n| s.projects.first()?.live.tickets.get(&n).cloned()) else { return };
    let target = format!("thread:{}", v.id);
    let quotes: Vec<(u32, String, String)> = std::env::var("YAAIHH_QUOTES").unwrap_or_default().split(';').filter_map(|q| {
        let mut f = q.splitn(3, '|');
        Some((f.next()?.trim().parse().ok()?, f.next()?.to_string(), f.next().unwrap_or_default().to_string()))
    }).collect();
    let comments: Vec<ask::Comment> = quotes.iter().filter_map(|(n, q, note)| {
        let c = threads::scene_comment(s, &v, *n, q, note);
        if c.is_none() {
            eprintln!("not found in #{n}: {q}");
        }
        c
    }).collect();
    if !send {
        if let Some(c) = comments.first() {
            let p = ask::Picked { target, source: c.source.clone(), label: c.label.clone(), quote: c.quote.clone(), at: c.at.clone(), pi: s.cur, anchor: None, block: None, pane: None, texts: Vec::new() };
            ask::write(s, p, &c.note);
        }
        return;
    }
    for c in comments {
        let r = ask::alone(s, 0, &target, &c).and_then(|req| s.projects[0].ask_once(&req));
        match r {
            Ok(r) => eprintln!("replied: {r}"),
            Err(e) => eprintln!("not sent: {e}"),
        }
    }
    if let Some(p) = s.projects.first_mut() {
        p.live = Live::default();
    }
    s.tickets.texts.clear();
    tickets_scene(s, false);
    s.tickets.unfolded.clear();
}

fn thread_card_scene(s: &mut App, send: bool) {
    tickets_scene(s, false);
    s.tickets.unfolded.clear();
    let card = std::env::var("YAAIHH_CARD").unwrap_or_default();
    match threads::scene_card(s, &card, send) {
        Ok(r) if send => eprintln!("answered: {r}"),
        Ok(_) => {}
        Err(e) => eprintln!("not sent: {e}"),
    }
    if send {
        if let Some(p) = s.projects.first_mut() {
            p.live = Live::default();
        }
        s.tickets.texts.clear();
        s.composers.retain(|(_, place), _| !place.starts_with("thread:"));
        tickets_scene(s, false);
        s.tickets.unfolded.clear();
    }
}

/// The composer's sends for real, one at a time (the `agent-send`, `agent-esc`, `agent-writing` scenes).
fn composer_scene(s: &mut App, what: &str) {
    let name = std::env::var("YAAIHH_AGENT").unwrap_or_default();
    read_once(s, Sel::Agent(name.clone()));
    let reqs: Vec<Request> = match what {
        "send" => {
            s.composers.insert((0, name.clone()), TextEdit::new(std::env::var("YAAIHH_SAY").unwrap_or_default()));
            match s.composed(0, &name) {
                Some((r, _)) => vec![r],
                None => {
                    eprintln!("not sent: {}", s.notice.clone().unwrap_or_else(|| "nothing written".into()));
                    return;
                }
            }
        }
        "esc" => vec![Request::Esc { to: vec![name.clone()], step: false }],
        _ => vec![Request::Writing { agent: name.clone(), on: true, thread: None }, Request::Writing { agent: name.clone(), on: false, thread: None }],
    };
    let Some(p) = s.projects.first() else { return };
    for r in &reqs {
        eprintln!("request: {}", serde_json::to_string(r).unwrap_or_default());
    }
    for m in link::once(&p.cmd, &reqs) {
        if let Incoming::Reply(id, r) = m {
            eprintln!("reply {id}: ok={} {}", r.ok, r.out);
        }
    }
    s.composers.remove(&(0, name.clone()));
    // the chat as it is after
    read_once(s, Sel::Agent(name));
}

/// YAAIHH_SNAPSHOT_FILE=F: the agents, the sidebar's parts and his mail hold of a snapshot as JSON put in for the first
/// project, in place of its coordd's (the parts of N-412 ask 2 drawn before a coordd sends them; a held agent's row past
/// its reset under his Hold, N-1397 p167; an agent the router moves off the gateway, on the Accounts page, I-1469).
pub(crate) fn snapshot_scene(s: &mut App) {
    if let (Ok(f), Some(p)) = (std::env::var("YAAIHH_SNAPSHOT_FILE"), s.projects.first_mut()) {
        match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<coord_types::Snapshot>(&t).map_err(|e| e.to_string())) {
            Ok(snap) => {
                p.live.agents = snap.agents;
                p.live.parts = snap.parts;
                p.live.mail_hold = snap.mail_hold;
            }
            Err(e) => eprintln!("YAAIHH_SNAPSHOT_FILE {f}: {e}"),
        }
    }
}

pub(crate) fn read_once(s: &mut App, sel: Sel) {
    // YAAIHH_CIRCLE=NAME=TOKENS: agent NAME's session settings open with the context circle's mark dragged to TOKENS
    // (circle.rs), for a still of the drag
    if let Some((n, t)) = std::env::var("YAAIHH_CIRCLE").ok().as_deref().and_then(|v| v.split_once('=')) {
        s.streams.open.insert(format!("session|0|{n}"));
        if let Ok(t) = t.parse() {
            s.circle.on = Some((0, n.to_string(), t));
        }
    }
    let Some(p) = s.projects.first_mut() else { return };
    for m in link::once(&p.cmd, &[]) {
        match m {
            Incoming::Event(ev) => _ = p.apply(ev),
            // what the bridge speaks, as the live link takes it: a still asks what the window would
            Incoming::Hello(h) => p.protocol = h.protocol,
            _ => {}
        }
    }
    // YAAIHH_STILL_AFTER=look: a stand-in window (coordd's text-change still, consult-lead N-412 p1203) says on its
    // output that its link is up, and draws once the look came on that link (`Body::Look`), as his window takes a
    // change; never a look read from a file
    if std::env::var("YAAIHH_STILL_AFTER").is_ok_and(|v| v == "look") {
        use std::io::Write as _;
        println!("{}", coord_types::STILL_WATCHING);
        let _ = std::io::stdout().flush();
        for m in link::until(&p.cmd, |i| matches!(i, Incoming::Event(e) if matches!(e.body, coord_types::Body::Look(_)))) {
            if let Incoming::Event(ev) = m {
                _ = p.apply(ev);
            }
        }
    }
    p.down = None;
    // the labels, as the live window asks for them (refs.rs): a still shows coordd's own
    let ids = refs::ids_of(p);
    if !ids.is_empty() {
        let r = p.ask_once(&Request::Label { ids });
        p.labels.took(r.is_ok(), r.as_deref().unwrap_or(""));
    }
    // YAAIHH_ACTS=ACTOR:THING@X,Y: the menu of ACTOR's acts at THING open at window point X,Y, as the stand-in's coordd
    // answers it (acts.rs); YAAIHH_ACTS=AGENT@X,Y: the menu at AGENT's tile; YAAIHH_ACTS_ANYWAY=ACT: then his "Do it
    // anyway" for the greyed act ACT
    let acts_at = std::env::var("YAAIHH_ACTS").ok().and_then(|v| {
        let (who, at) = v.split_once('@')?;
        let asks = match who.split_once(':') {
            Some((a, t)) => vec![(a.to_string(), Some(t.to_string()))],
            None => acts::at_agent(who, &p.live.agents.iter().find(|a| a.name == who).map(|a| a.parent.clone()).unwrap_or_default()),
        };
        let (x, y) = at.split_once(',')?;
        let got: Vec<_> = asks.into_iter().map(|(actor, thing)| {
            let r = p.ask_once(&Request::Enabled { thing: thing.clone(), actor: Some(actor.clone()) });
            (actor, thing, r)
        }).collect();
        Some((got, rune::PointerPress { x: x.parse().ok()?, y: y.parse().ok()? }))
    });
    // YAAIHH_VIEW_DATA=ID=FILE: view ID shows FILE's data (a source's output not yet live in coordd)
    if let Some((id, file)) = std::env::var("YAAIHH_VIEW_DATA").ok().as_deref().and_then(|v| v.split_once('='))
        && let (Some(v), Ok(text)) = (p.live.views.get_mut(id), std::fs::read_to_string(file))
    {
        v.data = serde_json::from_str(&text).ok();
    }
    // YAAIHH_AGENT_FIELDS=FILE: fields laid over the agents named in it (`{NAME: {"no_box": …, "moved": …}}`), as a
    // coordd that fills them would send them (a field not on this coordd yet)
    if let Ok(text) = std::env::var("YAAIHH_AGENT_FIELDS").map(|f| std::fs::read_to_string(f).unwrap_or_default()) {
        let over: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text).unwrap_or_default();
        for a in p.live.agents.iter_mut() {
            if let Some(serde_json::Value::Object(f)) = over.get(&a.name) {
                let mut v = serde_json::to_value(&*a).unwrap_or_default();
                for (k, x) in f {
                    v[k] = x.clone();
                }
                if let Ok(b) = serde_json::from_value(v) {
                    *a = b;
                }
            }
        }
    }
    if let Some((got, at)) = acts_at {
        let anyway = std::env::var("YAAIHH_ACTS_ANYWAY").ok();
        s.acts.menu = Some(acts::Menu { p: 0, at, groups: got.iter().map(|(actor, thing, _)| acts::Group { actor: actor.clone(), thing: thing.clone(), got: None }).collect() });
        for (actor, thing, r) in got {
            acts::took(s, 0, thing, actor, r.is_ok(), r.unwrap_or_else(|e| e));
        }
        if let Some(act) = anyway {
            let all: Vec<_> = s.acts.menu.iter().flat_map(|m| m.groups.iter()).filter_map(|g| g.got.clone()?.ok()).flatten().collect();
            s.acts.anyway = all.into_iter().find(|a| a.act == act && !a.enabled).map(|a| (0, a));
            s.acts.menu = None;
        }
    }
    let Some(p) = s.projects.first_mut() else { return };
    // an agent scene without a name shows YAAIHH_AGENT, else the last agent added
    let sel = match sel {
        Sel::Agent(n) if n.is_empty() => {
            Sel::Agent(std::env::var("YAAIHH_AGENT").ok().or_else(|| p.live.agents.last().map(|a| a.name.clone())).unwrap_or_default())
        }
        x => x,
    };
    // With you (withyou.rs): YAAIHH_WITH_YOU_OPEN=ID shows that thread beside the list, YAAIHH_WITH_YOU_ON=ID binds
    // the box to it
    if let Sel::Agent(name) = &sel {
        withyou::scene(s, 0, name);
    }
    // YAAIHH_LOOK_FILE=FILE: the project's look as coordd would send it (look.toml read as TOML), for a still of a look
    // (coord_types::look, N-866); YAAIHH_LOOK is his dark or light (tickets_scene)
    if let (Ok(f), Some(p)) = (std::env::var("YAAIHH_LOOK_FILE"), s.projects.first_mut()) {
        match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| toml::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
            Ok(v) => p.live.set_look(Some(v)),
            Err(e) => eprintln!("YAAIHH_LOOK_FILE {f}: {e}"),
        }
    }
    let Some(p) = s.projects.first_mut() else { return };
    // the sessions the scene shows: the agent's, or the tiles'
    let names: Vec<String> = match &sel {
        Sel::Agent(name) => vec![name.clone()],
        Sel::Agents => agents::active(p).map(|a| a.name.clone()).collect(),
        _ => Vec::new(),
    };
    let reqs: Vec<Request> = names.iter().map(|n| Request::Transcript { name: n.clone(), before: None, after: None, session: None }).collect();
    if !reqs.is_empty() {
        for m in link::once(&p.cmd, &reqs) {
            if let Incoming::Reply(id, r) = m {
                let Some(name) = names.get((id as usize).wrapping_sub(1)) else { continue };
                if let Ok(page) = serde_json::from_str::<Transcript>(&r.out) {
                    p.convs.entry(name.clone()).or_default().merge(page, false);
                }
            }
        }
    }
    // the shown sessions' images, read and decoded before the frame
    let wanted: Vec<(String, Request)> =
        names.iter().filter_map(|n| Some((n, p.convs.get(n)?))).flat_map(|(n, c)| chat::conv_image_requests(n, c)).collect();
    if !wanted.is_empty() {
        let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
        let reqs: Vec<Request> = wanted.iter().map(|(_, r)| r.clone()).collect();
        for m in link::once(&p.cmd, &reqs) {
            if let Incoming::Reply(id, r) = m
                && let Some((key, _)) = wanted.get((id as usize).wrapping_sub(1))
            {
                // a still is one frame: decoded now, and held so
                let img = match chat::take_image(r.ok, &r.out) {
                    chat::Img::Bytes(b) => {
                        let i = cache.load(b);
                        let _ = i.wait();
                        chat::Img::Ready(i)
                    }
                    x => x,
                };
                s.images.insert(key.clone(), img);
            }
        }
    }
    // the question tickets' words, which Decisions and an item's page show
    let locs = crate::texts::scene_texts(&s.tickets, None, s.projects.first());
    for (key, loc) in locs {
        let r = s.projects.first().map(|p| p.ask_once(&Request::Text { loc })).unwrap_or_else(|| Err("no project".into()));
        s.tickets.texts.insert(key, Some(r));
    }
    // an agent's With you: the words of its posts, asked once in one pipe, as the live poll asks them (tickets::poll)
    if let (Sel::Agent(name), Some(p)) = (&sel, s.projects.first()) {
        let locs = crate::streams::wanted(&s.tickets, p, name, crate::streams::Which::You);
        let reqs: Vec<Request> = locs.iter().map(|l| Request::Text { loc: l.clone() }).collect();
        for m in link::once(&p.cmd, &reqs) {
            if let Incoming::Reply(i, r) = m
                && let Some(l) = (i as usize).checked_sub(1).and_then(|i| locs.get(i))
            {
                s.tickets.texts.insert(crate::texts::key(l), Some(if r.ok { Ok(r.out) } else { Err(r.out) }));
            }
        }
    }
    // YAAIHH_ANSWERED: Decisions shows the answered questions
    s.domain.answered |= std::env::var("YAAIHH_ANSWERED").is_ok();
    s.sel = sel;
}

/// The terminal scene: agent NAME's page switched to Terminal, its screen read once. With YAAIHH_TERM_TEST (see the
/// top) the one project is a local coord folder whose pipe is `coordc bridge` run here (the coordc beside this
/// binary); else the first configured project.
fn term_scene(s: &mut App) {
    let name = std::env::var("YAAIHH_TERM_AGENT").unwrap_or_else(|_| "t1".into());
    if let Ok(coord) = std::env::var("YAAIHH_TERM_TEST") {
        let coordc = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("coordc").display().to_string())).unwrap_or_else(|| "coordc".into());
        let cmd = vec!["env".into(), format!("COORD={coord}"), format!("COORDD_SOCK={coord}/coordd.sock"), coordc, "bridge".into()];
        s.projects = vec![Proj {
            name: "terminal test".into(),
            id: "term-test".into(),
            device: None,
            cmd,
            link: None,
            live: Live::default(),
            down: None,
            boxed: false,
            starting: None,
            forking: None,
            spin_ending: None,
            protocol: 0,
            release: None,
            unread: false,
            refused: None,
            start_error: None,
            convs: HashMap::new(),
            pending: HashMap::new(),
            commits: Vec::new(),
            older: Vec::new(),
            older_from: None,
            older_pending: false,
            inbox: Vec::new(),
            labels: refs::Held::default(),
        }];
    }
    if s.projects.is_empty() {
        s.notice = Some("no project: the term scene needs YAAIHH_TERM_TEST=COORD or a configured project".into());
        return;
    }
    s.cur = 0;
    read_once(s, Sel::Agent(name.clone()));
    s.term.pages.insert((0, name.clone()));
    term::attach(s, 0, &name);
    for m in link::once(&s.projects[0].cmd, &[Request::Attach { name: name.clone(), rows: 0, cols: 0 }]) {
        match m {
            Incoming::Pty(ev) => term::take(s, 0, ev),
            Incoming::Reply(_, r) if !r.ok => term::refused(&mut s.term, 0, &name, r.out),
            _ => {}
        }
    }
}

/// The core's scenes and the extensions' (`--snap F --scene NAME`).
fn scenes() -> &'static [(&'static str, fn(&mut App))] {
    let all: Vec<(&'static str, fn(&mut App))> = SCENES.iter().chain(ext::all().iter().flat_map(|e| e.scenes)).copied().collect();
    Box::leak(all.into_boxed_slice())
}

const SCENES: &[(&str, fn(&mut App))] = &[
    // the Tickets page on YAAIHH_TICKET (a number), else the newest ticket at the top of a tree; its texts read once
    ("tickets", |s| tickets_scene(s, false)),
    // the Tickets page's open form
    ("tickets-open", |s| tickets_scene(s, true)),
    // YAAIHH_AGENT's chat (else the last agent added) with YAAIHH_FIND's words searched in its With you list's field
    // (find.rs; on a page with no list the same box opens over it)
    ("find-over", |s| {
        read_once(s, Sel::Agent(std::env::var("YAAIHH_AGENT").unwrap_or_default()));
        s.tickets.find.q = TextEdit::new(std::env::var("YAAIHH_FIND").unwrap_or_else(|_| "window".into()));
        find::go(s);
        s.tickets.find.over = true;
    }),
    // the Threads page with History open over the bar's arrows (history.rs)
    ("history", |s| {
        tickets_scene(s, false);
        history::scene(s);
    }),
    // the structure pane and the lenses (lens.rs `scene`): YAAIHH_LENS's lens at YAAIHH_TICKET, YAAIHH_LENS_BESIDE's in a
    // pane beside it, YAAIHH_TREE_OPEN's threads unfolded in the structure
    ("lens", |s| {
        tickets_scene(s, false);
        lens::scene(s);
    }),
    // the Threads list with "Between agents" unfolded (design part 9b)
    ("threads-between", |s| {
        tickets_scene(s, false);
        s.tickets.between = true;
    }),
    // Home with its Devices (devices.rs): the reader's answer asked once of the stand-in, this machine read once
    // (YAAIHH_THIS_PC stands in for it in a still); YAAIHH_DEVICE_OPEN=NAME: that device's row pressed open
    ("devices", |s| {
        read_once(s, Sel::View(String::new()));
        let Some(p) = s.projects.first() else { return };
        s.sel = Sel::View(global::ID.into());
        let r = p.ask_once(&Request::Machines);
        devices::came(s, r);
        s.devices.this_pc = devices::this_pc();
        s.devices.open.extend(std::env::var("YAAIHH_DEVICE_OPEN").ok());
        // Home's Store on a stand-in until its request is in coord-types (store.rs): YAAIHH_STORE_FILE its rows,
        // YAAIHH_STORE_OPEN=ROW=FILE that row pressed open with FILE as its first page
        if let Ok(f) = std::env::var("YAAIHH_STORE_FILE") {
            store::came(s, std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}")));
        }
        // Home's cold storage on a stand-in: YAAIHH_COLD_FILE coordd's answer
        if let Ok(f) = std::env::var("YAAIHH_COLD_FILE") {
            cold::came(s, std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}")));
        }
        // on a stand-in: Settings (YAAIHH_POLICY_FILE, coordd's answer; YAAIHH_SETTINGS=global|project|AGENT the page
        // open there), the repositories' state (YAAIHH_REPO_STATE), the Files page on a PDF (YAAIHH_PDF), and the
        // window's Home (YAAIHH_GLOBAL=1)
        if let Ok(f) = std::env::var("YAAIHH_POLICY_FILE") {
            s.settings.asked = s.projects.first().map(|p| p.id.clone());
            policy::came(s, false, std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}")));
            let models = s.projects.first().map(|p| p.ask_once(&projects::models_asked(p)));
            policy::models_came(s, models.unwrap_or_else(|| Err("no project".into())));
        }
        if let Ok(scope) = std::env::var("YAAIHH_SETTINGS") {
            let scope = match scope.as_str() {
                "global" => policy::Scope::Global,
                "project" => policy::Scope::Project,
                n => policy::Scope::Agent(n.into()),
            };
            policy::open(s, scope);
        }
        if let (Ok(f), Some(id)) = (std::env::var("YAAIHH_REPO_STATE"), s.projects.first().map(|p| p.id.clone())) {
            home::repo_state_came(s, id, std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}")));
        }
        if std::env::var("YAAIHH_GLOBAL").is_ok_and(|v| v == "1") {
            s.sel = Sel::View(global::ID.into());
        }
        files::scene_pdf(s);
        // the walk's row unfolded (YAAIHH_STORE_FOLDERS=1)
        s.store.folders_open |= std::env::var("YAAIHH_STORE_FOLDERS").is_ok_and(|v| v == "1");
        // (`outside:FOLDER=FILE`: the walk's folder FOLDER pressed open, FILE its findings' page; YAAIHH_STORE_CONFIRM a
        // finding's Delete pressed once)
        if let Some((row, f)) = std::env::var("YAAIHH_STORE_OPEN").ok().as_deref().and_then(|v| v.split_once('=')) {
            let row = match row.split_once(':') {
                Some(("outside", folder)) => {
                    s.store.folders_open = true;
                    store::press_folder(&mut s.store, folder);
                    "outside"
                }
                _ => {
                    store::press(&mut s.store, row);
                    row
                }
            };
            store::paged(s, row, std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}")));
            s.store.confirming = std::env::var("YAAIHH_STORE_CONFIRM").ok();
        }
        // and Layers now on a stand-in until a2h-speed's run layers are in coordd: YAAIHH_LAYERS_FILE its answer,
        // YAAIHH_LAYER_OPEN a layer pressed open
        if let Ok(f) = std::env::var("YAAIHH_LAYERS_FILE") {
            layers::came(s, std::fs::read_to_string(&f).map_err(|e| format!("{f}: {e}")));
        }
        s.layers.open.extend(std::env::var("YAAIHH_LAYER_OPEN").ok());
    }),
    // plan/edge.md §5's S2 scenes: one ticket (YAAIHH_TICKET, else the latest top one) with its deliveries folded; the
    // first ticket that owes the owner a step, with the answer box; a ticket with every delivery's text unfolded
    ("ticket", |s| {
        tickets_scene(s, false);
        s.tickets.unfolded.clear();
    }),
    ("ticket-owner-step", |s| {
        let owed = std::env::var("YAAIHH_TICKET").is_err();
        tickets_scene(s, false);
        if owed {
            if let Some(n) = s.projects.first().and_then(|p| crate::waits::on_owner(p).iter().find(|w| w.kind == crate::waits::Kind::Step).map(|w| w.t.n)) {
                s.tickets.shown = Some(n);
            }
        }
        s.tickets.unfolded.clear();
    }),
    ("ticket-delivery", |s| tickets_scene(s, false)),
    // YAAIHH_TICKET's thread, and the picture its post names at YAAIHH_PICTURE opened as a click on it opens it on
    // Windows: found by its coordd (Locate) and read in pieces (ReadFile), in the viewer over the thread
    ("ticket-picture", |s| {
        tickets_scene(s, false);
        s.tickets.unfolded.clear();
        if let Ok(path) = std::env::var("YAAIHH_PICTURE") {
            if let Some(p) = s.projects.first_mut() {
                p.protocol = files::FILES;
            }
            s.cur = 0;
            // the live window's decoder (setup makes it; a headless scene has none)
            s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
            paths::locate_once(s, &path);
            viewer::drain(s);
            if let Some(key) = s.viewing.clone() {
                if let Some(chat::Img::Ready(img)) = chat::image(s, &key) {
                    let _ = img.wait();
                }
                // fitted once its size is known, in the window's area, as the image-viewer scene does
                viewer::show(s, key);
                viewer::scene_zoom(s, None);
            }
        }
    }),
    ("tickets-answer", tickets_answer_scene),
    ("ticket-once-save", ticket_once_save_scene),
    // the Kinds page with YAAIHH_EDIT_KIND's edit typed (YAAIHH_EDIT_TEXT, YAAIHH_EDIT_WHY), then its Save pressed: the
    // kind written and committed and coordd's reload sent, for real (dev copy only); its reply under the Save's line
    ("kinds-save", kinds_save_scene),
    ("ticket-halt-save", ticket_halt_save_scene),
    // the Threads page's card (design §16, 3) on YAAIHH_TICKET, filled from YAAIHH_CARD (`N: 1=text; 2=text`), as
    // clicks and typing leave it; the -send one sends what its Send button posts, for real, then shows the thread after
    // a passage of an agent's message commented on in the thread (YAAIHH_TICKET): YAAIHH_QUOTES="P|passage|note;…"
    // (P the post's number); the first's box open, as the selection's Comment leaves it; the -send one sends each as
    // its own reply, for real, as the thread's Send does, then shows the thread after
    ("thread-comment", |s| thread_comment_scene(s, false)),
    ("thread-comment-send", |s| thread_comment_scene(s, true)),
    // the find box (find.rs): YAAIHH_FIND typed and Enter pressed (an id opens it, words list the hits); `*` the
    // Starred list
    ("find", |s| {
        tickets_scene(s, false);
        let q = std::env::var("YAAIHH_FIND").unwrap_or_default();
        if q == "*" {
            s.tickets.find.shown = Some(find::Shown::Starred(find::ask(s, String::new(), true)));
        } else {
            s.tickets.find.q = TextEdit::new(q);
            find::go(s);
            // the thread an id opened: its texts read, as the page's poll reads them in the app
            let shown = s.tickets.shown.and_then(|n| find::ticket(s, s.projects.first()?, n).cloned());
            for (key, loc) in crate::texts::scene_texts(&s.tickets, shown.as_ref(), s.projects.first()) {
                let r = s.projects.first().map(|p| p.ask_once(&Request::Text { loc })).unwrap_or_else(|| Err("no project".into()));
                s.tickets.texts.insert(key, Some(r));
            }
        }
    }),
    ("thread-card", |s| thread_card_scene(s, false)),
    ("thread-card-send", |s| thread_card_scene(s, true)),
    ("items", |s| read_once(s, Sel::View("items".into()))),
    ("closed", |s| {
        read_once(s, Sel::View("items".into()));
        s.show_closed = true;
    }),
    ("runner", |s| read_once(s, Sel::View("runner".into()))),
    ("kinds", kinds_scene),
    // the Files page on a folder the scene makes (files.rs): README.md shown, or YAAIHH_FILE (src/main.rs, say)
    ("files", files::scene),
    // the project's home (its overview), or YAAIHH_VIEW's own page (a text change's still of a view, change.rs
    // `still_page`)
    ("overview", |s| read_once(s, Sel::View(std::env::var("YAAIHH_VIEW").unwrap_or_default()))),
    ("items-board", |s| {
        read_once(s, Sel::View("items".into()));
        s.domain.board = true;
    }),
    ("decisions", |s| read_once(s, Sel::View("decisions".into()))),
    // the detail panel: the first item an agent works on (else the first item), the first card in a lane
    ("item-detail", |s| {
        read_once(s, Sel::View("items".into()));
        let Some(p) = s.projects.first() else { return };
        let id = p.live.agents.iter().flat_map(|a| a.items.first()).next().cloned().or_else(|| p.live.views.get("items")?.data.as_ref()?.pointer("/items/0/id")?.as_str().map(String::from));
        s.domain.detail = id.map(domain::Detail::Item);
    }),
    ("runner-detail", |s| {
        read_once(s, Sel::View("runner".into()));
        let Some(cards) = s.projects.first().and_then(|p| p.live.views.get("runner")?.data.as_ref()?.get("cards")?.as_array().cloned()) else { return };
        let card = cards.iter().find(|c| c.get("lane").is_some_and(|l| !l.is_null())).or(cards.first());
        s.domain.detail = card.and_then(|c| c.get("id")?.as_str()).map(|id| domain::Detail::Card(id.into()));
    }),
    // the old mail, now at the foot of Threads (design part 9b), its last message open
    ("mail", |s| {
        read_once(s, Sel::Tickets);
        s.open_mail = s.projects.first().and_then(|p| p.live.mail.last()).map(|m| m.id.clone());
    }),
    ("agent", |s| read_once(s, Sel::Agent(String::new()))),
    // an agent's chat as its two streams (streams.rs), over the tickets the tickets scenes read (YAAIHH_TICKETS_FILE):
    // YAAIHH_AGENT's chat, YAAIHH_STREAM's view
    ("agent-streams", |s| {
        tickets_scene(s, false);
        s.sel = Sel::Agent(std::env::var("YAAIHH_AGENT").unwrap_or_default());
    }),
    // YAAIHH_SNAPSHOT_FILE=F ([`snapshot_scene`]); YAAIHH_AGENT_JOBS=NAME: that tile's last jobs open, as a press on
    // its job line opens them (jobs.rs)
    ("agents", |s| {
        read_once(s, Sel::Agents);
        s.agents_jobs = std::env::var("YAAIHH_AGENT_JOBS").ok();
        snapshot_scene(s);
    }),
    // the composer's sends, for real, through a one-shot pipe (a try on a dev coord, paper-research N-101.p1374): the
    // request and coordd's reply on stderr. YAAIHH_AGENT's chat; agent-send: YAAIHH_SAY as typed and Send pressed, the
    // chat read again after; agent-esc: Esc; agent-writing: the box holding words, then emptied
    ("agent-send", |s| composer_scene(s, "send")),
    ("agent-esc", |s| composer_scene(s, "esc")),
    ("agent-writing", |s| composer_scene(s, "writing")),
    // the agent scene with its first image opened over the window (a click on it in a chat)
    ("agent-image", |s| {
        read_once(s, Sel::Agent(String::new()));
        // YAAIHH_IMAGE=FILE: that picture stands in when the chat holds none
        if let Ok(bytes) = std::env::var("YAAIHH_IMAGE").map(std::fs::read).unwrap_or(Ok(Vec::new())) && !bytes.is_empty() {
            let cache = s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
            let img = cache.load(bytes);
            let _ = img.wait();
            s.images.insert("scene".into(), chat::Img::Ready(img));
        }
        s.viewing = s.images.iter().find(|(_, i)| matches!(i, chat::Img::Ready(_))).map(|(k, _)| k.clone());
    }),
    // the image viewer (viewer.rs) over the agent page: YAAIHH_IMAGES=F[:F…] opened in order, the last shown;
    // YAAIHH_IMAGE_SHOW=I shows the I-th instead; YAAIHH_IMAGE_ZOOM=F,X,Y zooms by F about point X,Y of the image area
    ("image-viewer", |s| {
        read_once(s, Sel::Agent(String::new()));
        for f in std::env::var("YAAIHH_IMAGES").unwrap_or_default().split(':').filter(|f| !f.is_empty()) {
            s.image_cache.get_or_insert_with(|| rune::ImageCache::new(std::sync::Arc::new(|| {})));
            // held as the app holds it, its bytes, and decoded as it is when shown (chat::image), before the frame
            let key = format!("path:{f}");
            s.images.insert(key.clone(), chat::Img::Bytes(std::fs::read(f).unwrap_or_default().into()));
            if let Some(chat::Img::Ready(img)) = chat::image(s, &key) {
                let _ = img.wait();
            }
            viewer::show(s, key);
        }
        if let Some(i) = std::env::var("YAAIHH_IMAGE_SHOW").ok().and_then(|i| i.parse::<usize>().ok()) {
            s.viewing = s.shown.opened.get(i).cloned();
        }
        viewer::scene_zoom(s, std::env::var("YAAIHH_IMAGE_ZOOM").ok());
    }),
    // the agent page's Rename dialog, open on the agent shown (projects.rs)
    ("agent-rename", |s| {
        read_once(s, Sel::Agent(String::new()));
        if let Sel::Agent(n) = s.sel.clone() {
            s.rename = projects::Rename { open: true, project: s.cur, from: n.clone(), name: TextEdit::new(&format!("{n}-2")), running: None, error: None };
        }
    }),
    // the Agents page's New agent dialog, its models and guesses read from coordd (projects.rs); YAAIHH_NEW_AGENT_PICK=MODEL: that
    // model chosen and the Model list open on it, as a press on the field leaves it
    ("new-agent", |s| {
        read_once(s, Sel::Agents);
        let out = s.projects.get(s.cur).and_then(|p| p.ask_once(&projects::models_asked(p)).ok()).unwrap_or_default();
        let (lines, guesses) = projects::models_of(&out);
        let models = projects::listed_models(&lines);
        let pick = std::env::var("YAAIHH_NEW_AGENT_PICK").ok().filter(|m| models.contains(m));
        let model = pick.clone().or_else(|| models.first().cloned());
        s.new_agent = projects::NewAgent { open: true, project: s.cur, name: TextEdit::new("try-1"), model, models, guesses, ..Default::default() };
        if pick.is_some() {
            let na = &mut s.new_agent;
            let _ = na.pick.step(rune_kit::ComboboxEvent::Toggle { current: na.model.as_deref() });
        }
    }),
    // the split: orch's chat beside LAZY's (panes.rs)
    // With you as threads in the narrow pane of two (withyou.rs): the agents' grid beside YAAIHH_AGENT's page
    ("with-you-split", |s| {
        read_once(s, Sel::Agents);
        s.split();
        read_once(s, Sel::Agent(String::new()));
    }),
    ("split", |s| {
        read_once(s, Sel::Agent("LAZY".into()));
        s.split();
        read_once(s, Sel::Agent("orch".into()));
    }),
    // the edge menus (tabs.rs): two tabs, the first with a split window, orch's page and the agents' grid
    ("tabs-top", |s| tabs_scene(s, tabs::Edge::Top)),
    ("tabs-windows", |s| tabs_scene(s, tabs::Edge::Side)),
    // the same tabs and windows with no menu open: a script rests the pointer at an edge (film with --realtime)
    ("tabs", |s| {
        tabs_scene(s, tabs::Edge::Top);
        s.tabs.open = None;
    }),
    // the comment bar with a note being written and one comment waiting (YAAIHH_AGENT's chat)
    ("comment", comment_scene),
    // an @ being typed in an agent chat's composer: the agents offered at the caret (owner m118)
    ("mention", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let mut w = TextEdit::new("merge it and ask @");
        w.set_caret(w.text.len());
        w.focused = true;
        s.composers.insert((s.cur, a.clone()), w);
        s.notes.mention = Some(ask::Mention { field: ask::Field::Composer(s.cur, a), at: 17, query: String::new(), active: 0 });
    }),
    // a mail the owner sent that has not reached the agent yet, queued at the chat's foot (chat.rs `queued`)
    ("queued", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let Some(p) = s.projects.first_mut() else { return };
        if let Some(x) = p.live.agents.iter_mut().find(|x| x.name == a) {
            x.state = "working".into();
        }
        p.live.mail.put(coord_types::MailView {
            id: "m9999".into(), at: "2026-11-08T14:02:00Z".into(), from: "owner".into(), to: vec![a], subject: String::new(),
            body: "once the runner is green, merge it and tell me".into(), esc: false, soon: false, delivered: Vec::new(), withdrawn: Vec::new(),
            kind: String::new(), re: None, quotes: Vec::new(), state: None, recognised: None, filed: None, unfiled: false, clerk: None,
        });
    }),
    // a mail of the owner's the clerk sent back, with Deliver anyway (owner m116)
    ("queued-clerk", |s| {
        read_once(s, Sel::Agent(String::new()));
        let Sel::Agent(a) = s.sel.clone() else { return };
        let Some(p) = s.projects.first_mut() else { return };
        p.live.mail.put(coord_types::MailView {
            id: "m9999".into(), at: "2026-11-08T14:02:00Z".into(), from: "owner".into(), to: vec![a], subject: String::new(),
            body: "merge it once its green".into(), esc: false, soon: false, delivered: Vec::new(), withdrawn: Vec::new(),
            kind: String::new(), re: None, quotes: Vec::new(), state: None, recognised: None, filed: None, unfiled: false,
            clerk: Some("sent back: which branch: core/l1 or lazy/portfolio?".into()),
        });
    }),
    ("agents-all", |s| {
        read_once(s, Sel::Agents);
        s.agents_tab = 1;
    }),
    ("new-project", |s| {
        s.new_project.open = true;
        s.new_project.name = TextEdit::new("Scratch");
    }),
    ("term", term_scene),
    // the owner's message YAAIHH_SAY to agent YAAIHH_SAY_TO, for real (dev copy only); then the Tickets page
    ("say", |s| {
        if let (Some(p), Ok(text)) = (s.projects.first(), std::env::var("YAAIHH_SAY")) {
            match p.ask_once(&Request::Say { to: std::env::var("YAAIHH_SAY_TO").ok(), names: Vec::new(), text, follows: None, wait: false, under: None, label: None, thread: false, typed: None, split_from: None, comments: Vec::new() }) {
                Ok(r) => eprintln!("said: {r}"),
                Err(e) => eprintln!("not said: {e}"),
            }
        }
        tickets_scene(s, false);
    }),
    // an agent's chat with its raw pane beside it (owner m750): YAAIHH_RAW_AGENT, or the first agent
    ("raw", |s| {
        read_once(s, Sel::Agents);
        let Some(name) = std::env::var("YAAIHH_RAW_AGENT").ok().or_else(|| s.projects.first()?.live.agents.first().map(|a| a.name.clone())) else { return };
        read_once(s, Sel::Agent(name.clone()));
        s.open_raw(0, &name);
        // YAAIHH_RAW_LIVE: not filled here; the pane's reader reads its page as in the app, and it comes in some
        // frames after the open (the frames of an open, N-40 post 5)
        if std::env::var("YAAIHH_RAW_LIVE").is_ok() {
            raw::scene_read(s, 0, &name, std::env::var("YAAIHH_RAW_LIVE").ok().and_then(|n| n.parse().ok()).unwrap_or(10));
        } else {
            raw::scene_fill(s, 0, &name);
        }
    }),
    ("commits", |s| read_once(s, Sel::View("commits".into()))),
    ("jump", |s| {
        read_once(s, Sel::View("commits".into()));
        if let Some(c) = s.commits().iter().cloned().find(|c| s.projects[0].live.agents.iter().any(|a| a.name == c.agent)) {
            let req = Request::Transcript { name: c.agent.clone(), before: None, after: None, session: None };
            for m in link::once(&s.projects[0].cmd, &[req]) {
                if let Incoming::Reply(1, r) = m {
                    if let Ok(page) = serde_json::from_str::<Transcript>(&r.out) {
                        s.projects[0].convs.entry(c.agent.clone()).or_default().merge(page, false);
                    }
                }
            }
            s.sel = Sel::Agent(c.agent.clone());
            s.seek = Some(c); // the first frame's poll finds it and scrolls there
        }
    }),
];

/// Whether this is a dev instance (`--dev`): beside prod, its own socket and tray icon (tray.rs); an extension leaves
/// to prod what only one instance should do.
static DEV: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn dev() -> bool {
    DEV.load(std::sync::atomic::Ordering::Relaxed)
}

/// `--live`: a headless run whose projects' pipes start as the window's do, on a scratch coordd the runner made on an
/// empty folder (yaaihh-e2e's Scratch), never a coordd of the owner's (`live_refused`).
static LIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn live() -> bool {
    LIVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// The scene `kinds-save` (SCENES): Save on the kind edit open, as its button does, the reload asked once.
fn kinds_save_scene(s: &mut App) {
    kinds_scene(s);
    let Some((commit, why)) = edit::save_kind_file(s) else { return };
    let r = s.projects.first().map(|p| p.ask_once(&Request::Reload { commit, why })).unwrap_or_else(|| Err("no project".into()));
    let (ok, out) = match r {
        Ok(t) => (true, t),
        Err(e) => (false, e),
    };
    eprintln!("reload: {ok} {out}");
    edit::replied(s, true, ok, &out);
    // the kinds and tickets read again, as the app's next events bring them
    if let Some(p) = s.projects.first() {
        s.tickets.kinds = Some(p.ask_once(&Request::Kinds).and_then(|o| serde_json::from_str(&o).map_err(|e| e.to_string())));
    }
}

/// The Kinds page with the kinds read from coordd, as the page asks for them.
fn kinds_scene(s: &mut App) {
    read_once(s, Sel::Kinds);
    let Some(p) = s.projects.first() else { return };
    s.tickets.kinds = Some(p.ask_once(&Request::Kinds).and_then(|o| serde_json::from_str(&o).map_err(|e| e.to_string())));
    // YAAIHH_DOCKET=kind:K or who:NAME (comma-separated): the Docket's program K shown under it, NAME's arrows lit
    for at in std::env::var("YAAIHH_DOCKET").unwrap_or_default().split(',') {
        match at.split_once(':') {
            Some(("kind", n)) => {
                s.tickets.pic_open.insert(format!("docket-kind:{n}"));
                s.kinds_open.insert(n.to_string());
            }
            Some(("who", n)) => _ = s.tickets.pic_open.insert(format!("docket-focus:{n}")),
            _ => {}
        }
    }
    // YAAIHH_KIND=K: kind K's text open
    if let Ok(k) = std::env::var("YAAIHH_KIND") {
        for k in k.split(',').filter(|k| !k.is_empty()) {
            s.kinds_open.insert(k.to_string());
        }
    }
    // YAAIHH_PICTURE_FILE=F: a kinds file's picture (a draft, not installed) above the project's kinds
    if let Ok(f) = std::env::var("YAAIHH_PICTURE_FILE") {
        s.tickets.picture_file = Some(f);
    }
    // YAAIHH_EDIT_KIND=K: kind K in its edit (edit.rs), as "Edit kind" leaves it; YAAIHH_CALL=C: call C's template open
    // (a step clicked); YAAIHH_EDIT_CALL=C: that template in its edit; YAAIHH_EDIT_TEXT=FROM=>TO: the edit's text with
    // FROM replaced, as typed; YAAIHH_EDIT_WHY: the why typed
    if let Ok(kind) = std::env::var("YAAIHH_EDIT_KIND") {
        let draft = s.tickets.picture_file.clone();
        edit::start_kind(s, &kind, draft.as_deref());
    }
    if let Ok(c) = std::env::var("YAAIHH_CALL") {
        s.tickets.edits.call = Some(c);
    }
    if let Ok(c) = std::env::var("YAAIHH_EDIT_CALL") {
        s.tickets.edits.call = Some(c.clone());
        edit::start_call(s, &c);
    }
    edit::scene_text(s);
    // YAAIHH_EDIT_LOOK=1: the window's look in its edit (look.rs `scene`), its text as coordd serves it
    if std::env::var("YAAIHH_EDIT_LOOK").is_ok_and(|v| v == "1") {
        look::scene(s);
    }
    doc_hover_scene(s);
    for k in std::env::var("YAAIHH_PIC_OPEN").unwrap_or_default().split(',').filter(|k| !k.is_empty()) {
        s.tickets.pic_open.insert(k.to_string());
    }
    // YAAIHH_RELOAD_REPLY=FILE: the edit open was saved and coordd answered its reload with FILE's text
    if let Ok(text) = std::env::var("YAAIHH_RELOAD_REPLY").map(|f| std::fs::read_to_string(f).unwrap_or_default()) {
        s.tickets.edits.said = Some(Ok("Saved, a1b2c3d.".into()));
        edit::replied(s, true, true, &text);
    }
}

/// The comment scene: a comment being written on a passage of the agent's latest message.
fn comment_scene(s: &mut App) {
    read_once(s, Sel::Agent(String::new()));
    let Sel::Agent(a) = s.sel.clone() else { return };
    // a passage of the agent's messages, being commented on
    let said: Vec<(String, String)> = s.projects[s.cur].convs.get(&a).map_or(Vec::new(), |c| {
        c.entries.iter().filter(|e| e.kind == "assistant").map(|e| (format!("{a}@{}", e.offset), crate::paths::linkify(&e.text))).collect()
    });
    let passage = |(source, src): &(String, String)| {
        let plain = |w: &&str| w.chars().all(|c| c.is_alphanumeric() || ",.'".contains(c));
        let words: Vec<&str> = src.lines().map(|l| l.split_whitespace().skip_while(|w| !plain(w)).take_while(plain).take(5).collect::<Vec<_>>()).find(|w| w.len() >= 4)?;
        let quote = words.join(" ");
        let at = rune_kit::markdown::find(src, &quote, rune_kit::markdown::Case::Sensitive).into_iter().next()?;
        Some((source.clone(), quote, at))
    };
    // the latest message, on screen
    if let Some((source, quote, at)) = said.iter().rev().find_map(passage) {
        let p = ask::Picked { target: a, source, label: "your message".into(), quote, at, pi: s.cur, anchor: None, block: None, pane: None, texts: Vec::new() };
        ask::write(s, p, "good, that is how it should work");
        // the scene's box shows unfocused, as it stood
        if let Some(e) = s.composers.get_mut(&(s.cur, message_box::COMMENT_PLACE.into())) {
            e.focused = false;
        }
    }
}

/// Run yaaihh with the extensions EXTS (ext.rs; none: the core alone): the window, or with `--background` the tray
/// only, or with `--snap` one scene's still.
pub fn run(exts: Vec<ext::Extension>) {
    // a panic's backtrace in stderr.log (an old window aborted as it quit, N-585.p39), but none taken for errors
    // (`Backtrace::capture`, anyhow), which RUST_BACKTRACE alone would make slow; before any thread starts. Whole: the
    // short one of W5's quit had no frame at all (N-804), and it is the quitting process's own setting that counts
    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // SAFETY: no other thread runs yet
        unsafe {
            std::env::set_var("RUST_BACKTRACE", "full");
            if std::env::var_os("RUST_LIB_BACKTRACE").is_none() {
                std::env::set_var("RUST_LIB_BACKTRACE", "0");
            }
        }
    }
    ext::register(exts);
    // `--background` (login autostart): the tray only, no window. `--replace`: the running yaaihh quits first (a
    // release going live). `--dev`: beside prod (tray.rs). `--live`: a still that keeps its pipes (`live`). Every
    // other argument is Rune's (the scenes).
    let ours = ["--background", "--replace", "--dev", "--live", OWNER_CONFIG];
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--after PID`: started by a window restarting onto this release (restart.rs); it ends first
    if let Some(i) = args.iter().position(|a| a == "--after") {
        let pid = args.get(i + 1).and_then(|p| p.parse::<u32>().ok());
        args.drain(i..(i + 2).min(args.len()));
        if let Some(pid) = pid {
            restart::after(pid);
        }
    }
    // `--take-over PID …`: started by a window swapping itself for this release (restart.rs)
    let taking = restart::take_args(&mut args);
    // a window started alone is his from now, before any pipe says hello; one taking over, from its "go" (link.rs
    // `SINCE`, restart.rs `ready`; paper-research N-1446 p1392)
    if taking.is_none() {
        link::his_since(chrono::Utc::now());
    }
    let flag = |f: &str| args.iter().any(|a| a == f);
    // a headless run (a still, frames, a video, a tree): no tray, no hand-over to the yaaihh running
    let still = ["--snap", "--video", "--out", "--strip", "--tree", "--html"].iter().any(|f| flag(f));
    let background = flag("--background");
    let refused = still_refused(still, std::env::var_os("YAAIHH_CONFIG").is_some(), flag(OWNER_CONFIG)).or_else(|| {
        let config = projects::config_path();
        let (his_config, reached, his) = if flag("--live") { live_coords(&config) } else { Default::default() };
        live_refused(flag("--live"), flag(OWNER_CONFIG), std::path::Path::new(&config), &his_config, &reached, &his)
    });
    if let Some(why) = refused {
        eprintln!("{why}");
        std::process::exit(2);
    }
    // a still asks coordd over one pipe for all it reads: one snapshot on coordd, not one for each post's text
    if still {
        link::keep_pipes();
    }
    DEV.store(flag("--dev"), std::sync::atomic::Ordering::Relaxed);
    LIVE.store(still && flag("--live"), std::sync::atomic::Ordering::Relaxed);
    if flag("--replace") && !tray::replace() {
        eprintln!("yaaihh --replace: the running yaaihh did not quit; it keeps running");
        std::process::exit(1);
    }
    if !still && taking.is_none() && tray::hand_over() {
        return; // a yaaihh runs: it opened its window
    }
    if !still && !rune::is_headless() {
        log::open();
    }
    let chrome = Chrome { full_size_content: true, traffic_lights: Some((16.0, 25.0)), drag_regions: true };
    let app = rune::App::new(AppSpec {
        setup: Some(setup),
        poll: Some(poll),
        on_key: Some(on_key),
        on_wheel: Some(zoom::wheel),
        scenes: scenes(),
        min_size: Some((720.0, 480.0)),
        // mono text (tool calls, results) in the system's monospace face
        families: ("Inter", "ui-monospace"),
        ..AppSpec::new("yaaihh", (1280.0, 800.0), init, view)
    })
    .chrome(chrome)
    .on_focus(|s: &mut App, focused, _| s.window_focused = focused)
    // the windows a request opened in close with it (sent_window.rs)
    .on_close(|s: &mut App, _| sent_window::close_all(s))
    .args(args.into_iter().filter(|a| !ours.contains(&a.as_str())));
    if still {
        return app.run();
    }
    // a take-over opens hidden where the old window stood, and shows once the old one has written its state
    let app = match &taking {
        Some(t) => app.window_options(rune::WindowOptions {
            visible: false,
            position: t.at.map(|a| (a.0, a.1)),
            physical_size: t.at.map(|a| (a.2, a.3)),
            ..Default::default()
        }),
        None => app,
    };
    let handle = app.handle();
    restart::app(handle.clone());
    let app = app.on_window(restart::window).on_ready(move |h| restart::ready(h, handle.clone()));
    // closing the window hides it: the usage collector and the boxes' links go on (tray.rs), and on Windows too
    // (N-804). There a hidden window once kept the next release from opening (pid 9616, N-497): it now gives way, as
    // it still reads release.txt and swaps hidden, reads it at once when a second start or the tray opens it, and
    // the one-window claim goes with its process at the swap (restart.rs)
    let app = if background { app.windowless() } else { app.keep_running() };
    tray::start(app.handle(), desk_marks::DeskMarks::load().on);
    app.run();
    tray::end();
}

/// The flag a still takes to run on the owner's own config (`~/.config/yaaihh/projects.toml`, `%APPDATA%\yaaihh` on
/// Windows), and so on his projects' coordds.
const OWNER_CONFIG: &str = "--owner-config";

/// Why a run is refused: a still (STILL) with no YAAIHH_CONFIG (CONFIG_SET) and no `--owner-config` (OWNER) would read
/// the owner's config and take a whole snapshot on prod's coordd for each read (research's engine lock, 600–800 ms a
/// time, in bursts while a window agent shot stills: a2h-speed, paper-research N-518 p5).
fn still_refused(still: bool, config_set: bool, owner: bool) -> Option<String> {
    (still && !config_set && !owner).then(|| {
        format!(
            "yaaihh: a still reads the projects of YAAIHH_CONFIG; set it to a projects.toml of your own. Without it the still would read the owner's {} and his coordds; {OWNER_CONFIG} says that is meant.",
            projects::config_path()
        )
    })
}

/// Why a `--live` run is refused: its presses would act on the owner's coordds, by `--owner-config` (OWNER), by his own
/// file as CONFIG (HIS_CONFIG, the same file), or by a coord folder it reaches (REACHED) that one of his projects has (HIS).
fn live_refused(live: bool, owner: bool, config: &std::path::Path, his_config: &std::path::Path, reached: &[std::path::PathBuf], his: &[std::path::PathBuf]) -> Option<String> {
    let same = |a: &std::path::Path, b: &std::path::Path| a.canonicalize().unwrap_or_else(|_| a.into()) == b.canonicalize().unwrap_or_else(|_| b.into());
    if !live {
        return None;
    }
    if owner || same(config, his_config) {
        return Some(format!("yaaihh: --live drives the projects it reads; it takes a YAAIHH_CONFIG of its own, never {OWNER_CONFIG} or {}", his_config.display()));
    }
    let mine = reached.iter().find(|r| his.iter().any(|h| same(r, h)))?;
    Some(format!("yaaihh: --live would drive {}, a coord folder of the owner's; it runs on a scratch coordd only", mine.display()))
}

/// The coord folders a live run on CONFIG reaches (COORD with YAAIHH_LOCAL_PIPE, else each project's under HOME), and
/// those of the owner's own projects (his own config file under his account's home, from the passwd file).
fn live_coords(config: &str) -> (std::path::PathBuf, Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    let ids = |file: &std::path::Path| -> Vec<String> {
        let read = std::fs::read_to_string(file).ok().and_then(|t| toml::from_str::<projects::Config>(&t).ok());
        read.map(|c| c.project.into_iter().map(|p| p.cfg.id).collect()).unwrap_or_default()
    };
    let reached = match std::env::var_os("COORD").filter(|_| std::env::var_os("YAAIHH_LOCAL_PIPE").is_some()) {
        Some(c) => vec![c.into()],
        None => ids(std::path::Path::new(config)).iter().map(|i| projects::coord_dir(i).into()).collect(),
    };
    let home = account_home().unwrap_or_else(|| std::env::var("HOME").unwrap_or_default().into());
    let his_config = home.join(".config/yaaihh/projects.toml");
    let his = ids(&his_config).iter().map(|i| home.join("agent-coord").join(i)).collect();
    (his_config, reached, his)
}

/// This account's home as the passwd file has it, whatever HOME says; none on Windows, where HOME's is used.
#[cfg(not(unix))]
fn account_home() -> Option<std::path::PathBuf> {
    None
}

/// This account's home as the passwd file has it, whatever HOME says.
#[cfg(unix)]
fn account_home() -> Option<std::path::PathBuf> {
    let uid = std::os::unix::fs::MetadataExt::uid(&std::fs::metadata("/proc/self").ok()?).to_string();
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().map(|l| l.split(':').collect::<Vec<_>>()).find(|f| f.len() > 5 && f[2] == uid).map(|f| f[5].into())
}

/// Which side a hello that does not fit asks to update (N-437): the app, when the bridge needs a newer protocol than
/// it speaks; else coordd's release, whose bridge speaks one older than the app still takes.
fn update_which(h: &coord_types::Hello) -> &'static str {
    if h.min > coord_types::PROTOCOL { "update the app" } else { "update the server (coordd's release)" }
}

#[cfg(test)]
mod refused_tests {
    use coord_types::Hello;

    /// A still with no YAAIHH_CONFIG is refused, saying what to set; one with it, or with --owner-config, and the
    /// window itself, run (N-518 p5).
    #[test]
    fn a_still_without_its_own_config_is_refused() {
        let why = crate::still_refused(true, false, false).expect("refused");
        assert!(why.contains("YAAIHH_CONFIG") && why.contains(crate::OWNER_CONFIG), "{why}");
        assert_eq!(crate::still_refused(true, true, false), None);
        assert_eq!(crate::still_refused(true, false, true), None);
        assert_eq!(crate::still_refused(false, false, false), None);
    }

    /// A live run on the owner's config is refused, by the flag or by his own file named in YAAIHH_CONFIG; on a config
    /// of its own, and a run that is not live, it runs.
    #[test]
    fn a_live_run_on_the_owners_config_is_refused() {
        let d = std::env::temp_dir().join(format!("live-refused-{}", std::process::id()));
        std::fs::create_dir_all(d.join("his/.config/yaaihh")).unwrap();
        let his = d.join("his/.config/yaaihh/projects.toml");
        std::fs::write(&his, "").unwrap();
        let mine = d.join("projects.toml");
        std::fs::write(&mine, "").unwrap();
        let why = crate::live_refused(true, true, &mine, &his, &[], &[]).expect("refused");
        assert!(why.contains("--live") && why.contains(crate::OWNER_CONFIG), "{why}");
        // his own file, named by another path to it
        let by_other_path = d.join("his/.config/../.config/yaaihh/projects.toml");
        assert!(crate::live_refused(true, false, &by_other_path, &his, &[], &[]).is_some(), "his own file in YAAIHH_CONFIG");
        assert_eq!(crate::live_refused(true, false, &mine, &his, &[], &[]), None);
        assert_eq!(crate::live_refused(false, true, &his, &his, &[], &[]), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A live run whose config of its own reaches a coord folder of one of his projects is refused; a scratch one runs.
    #[test]
    fn a_live_run_reaching_one_of_his_coords_is_refused() {
        let d = std::env::temp_dir().join(format!("live-coords-{}", std::process::id()));
        let (his, scratch) = (d.join("his/agent-coord/research"), d.join("scratch/agent-coord/e2e"));
        std::fs::create_dir_all(&his).unwrap();
        std::fs::create_dir_all(&scratch).unwrap();
        let (mine, his_config) = (d.join("projects.toml"), d.join("his.toml"));
        let why = crate::live_refused(true, false, &mine, &his_config, &[d.join("his/agent-coord/../agent-coord/research")], &[his.clone()]).expect("refused");
        assert!(why.contains("coord folder of the owner's"), "{why}");
        assert_eq!(crate::live_refused(true, false, &mine, &his_config, &[scratch], &[his]), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A bridge that refuses this app: the reason stays over the pipe's end that follows, says which side to update,
    /// and a hello that fits clears it.
    #[test]
    fn a_refusal_stays_the_reason_and_names_the_side_to_update() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let i = s.projects.len() - 1;
        let newer = Hello { protocol: coord_types::PROTOCOL + 1, min: coord_types::PROTOCOL + 1, release: None, refused: Some("this app speaks protocol 1, older than 9".into()), since: None };
        s.take(i, crate::link::Incoming::Hello(newer));
        s.take(i, crate::link::Incoming::Down("the pipe ended (5)".into()));
        let down = s.projects[i].down.clone().unwrap_or_default();
        assert!(down.contains("older than 9") && down.ends_with("update the app"), "{down}");
        s.take(i, crate::link::Incoming::Hello(Hello::ours(None)));
        assert!(s.projects[i].refused.is_none());
        s.take(i, crate::link::Incoming::Down("the pipe ended (1)".into()));
        assert_eq!(s.projects[i].down.as_deref(), Some("the pipe ended (1)"));
    }

    #[test]
    fn a_pipe_that_went_down_with_his_stars_asked_is_asked_again_when_it_is_back() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let i = s.projects.len() - 1;
        s.tickets.find.owned_asked.insert("r".into());
        s.projects[i].pending.insert(7, crate::Pending::OwnStars);
        s.take(i, crate::link::Incoming::Down("the pipe ended (1)".into()));
        assert!(!s.tickets.find.owned_asked.contains("r"), "asked again once the pipe is back");
        assert!(s.projects[i].pending.is_empty(), "the unanswered ask is gone with the pipe");
    }

    /// What a pipe's end loses is asked again or said, never left reading (proto-reader, N-1446 p251): a chat's image
    /// is asked again once the pipe is back with its chat on screen; a picture he opened says the pipe ended. (His
    /// box's split: split.rs `a_split_the_pipes_end_loses_is_asked_again`.)
    #[test]
    fn an_image_the_pipes_end_loses_is_asked_again_and_a_picture_says_the_pipe_ended() {
        use crate::chat::Img;
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["pr"]);
        let entry: coord_types::Entry = serde_json::from_value(serde_json::json!({"offset": 0, "at": "2026-11-20T09:00:00Z", "kind": "user", "text": "this", "images": [0]})).unwrap();
        s.projects[0].convs.insert("pr".into(), crate::live::Conv { entries: vec![entry], session: "s-1".into(), loaded: true, ..Default::default() });
        s.sel = crate::Sel::Agent("pr".into());
        let images_asked = |s: &crate::App| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Image { .. })).count();
        s.ask_images(0, "pr");
        let key = crate::chat::image_key("pr", "s-1", 0, 0);
        assert!(matches!(s.images.get(&key), Some(Img::Asked)) && images_asked(&s) == 1);
        let picture = std::path::PathBuf::from("/srv/shot.png");
        s.projects[0].pending.insert(900, crate::Pending::Picture { path: picture.clone(), from: 0 });
        s.images.insert(format!("path:{}", picture.display()), Img::Asked);
        s.take(0, crate::link::Incoming::Down("the pipe ended (1)".into()));
        assert!(!s.images.contains_key(&key), "the image is as if never asked");
        assert!(matches!(s.images.get(&format!("path:{}", picture.display())), Some(Img::Failed(e)) if e == "disconnected before the picture came"));
        // the pipe back: coordd's snapshot, the chat on screen asks its image again
        let agents = s.projects[0].live.agents.clone();
        let snap = coord_types::Snapshot { agents, ..Default::default() };
        s.take(0, crate::link::Incoming::Event(coord_types::Event { seq: 1, body: coord_types::Body::Snapshot(snap) }));
        assert!(matches!(s.images.get(&key), Some(Img::Asked)) && images_asked(&s) == 1, "asked again");
    }
}
