//! The Accounts page of the native path: the owner's own accounts that agents run on with nothing of ours in the
//! network path, each as a card with its tier, sign-in state, limit windows, agents and this month's requests. coordd
//! keeps all of it (`Request::Accounts { json: true }`); the page only shows it.
//!
//! Accounts are shared by all of the owner's coordds, but each has only its own coord's usage rows, so the page asks every
//! project's coordd and adds an account's usage across them. A coordd from before the native path adds nothing.
//!
//! One overview: the cards stand in tiers (`Tier 1` taken first; the tier is the `Priority` number), and its one `Add
//! account` opens the sheet (sheet.rs). No cost is shown: a subscription is not billed per token.
//!
//! With the gateway's pages in the build, the gateway's Accounts tab shows the same cards ([`native_card`], [`shown`])
//! in its tiers, opens the same sheet ([`add_button`], [`sheet`]) and asks through [`keep_asking`] and [`set_tier`];
//! this page is shown only where that tab is not built (Windows).

use crate::App;
use crate::ext::Extension;
use crate::spend::{Spend, compact, count, spend};
use coord_types::{AgentView, LimitWindow, AccountUsage, ModelOut, ModelWeek, NativeAccount, NativeAccounts, NativeAgent, RemovedAccount, Request, ResetOffer, Secret};
use rune::{El, ScrollSpec, TextEdit, Tx, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit, Menu, MenuItem, Spinner, Switch, TextInput};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

pub mod logos;
mod sheet;
pub use sheet::{close_sheet, open_claude, open_on, open_sheet, sheet, sheet_made, sheet_open};

pub(crate) const NAME: &str = "accounts";

/// How often the page asks again while it shows (the proxy's cards refresh as often).
const EVERY: Duration = Duration::from_secs(30);

/// How often it asks while the Add account sheet waits for the account it made (the gateway's logins are asked after
/// every 3 s too, `add.rs` `POLL`).
const SOON: Duration = Duration::from_secs(3);

/// The protocol from which a coordd answers the account requests (`Request::Accounts` and the `Account…` ones); a
/// project whose coordd is older is not asked ([`served`]).
pub(crate) const ACCOUNTS: u32 = 8;

/// Whether the project's coordd answers the account requests.
fn served(p: &crate::Proj) -> bool {
    p.protocol >= ACCOUNTS
}

/// The extension that adds the page; the core binary registers it (main.rs).
pub fn extension() -> Extension {
    Extension {
        name: NAME,
        init: |s| s.ext.insert(Accounts::default()),
        setup: |_, _| {},
        poll,
        rows: |_, _, _| Vec::new(),
        nav_key: |_| String::new(),
        page,
        title: |_| "Accounts".into(),
        reply,
        prices: None,
        scenes: &[("accounts", scene)],
        replaces: &[],
        window_pages: &[],
        accounts: None,
    }
}

/// Where a built-in account's resets are when it is not linked to a gateway login: claude.ai's usage page, opened
/// from the card's `Resets: on claude.ai ↗`.
pub const CLAUDE_USAGE: &str = "https://claude.ai/settings/usage";

/// The line that says whose key the field takes.
const KEY_HINT: &str = "Its Anthropic API key (console.anthropic.com → API keys):";

/// The same line on an agy key account's field.
const GEMINI_KEY_HINT: &str = "Its Gemini API key (aistudio.google.com → API keys):";

/// What the agents of an agy account t3code signed in run under, said on its card: t3code's threads, whose file reads
/// t3code approves.
const AGY_T3CODE_TERMS: &str = "t3code approves its reads";

/// What the agents of an agy account on agy's own client run under, said on its card: agy asks before every tool
/// call.
const AGY_OWN_TERMS: &str = "Asks before each tool call";

/// How an agy sign-in account was signed in, as coordd says (`NativeAccount::signed_in_by`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum AgyWay {
    /// By agy's own sign-in: its agents run on agy's own client.
    Own,
    /// By t3code, before agy's own was used here: its agents run as t3code's threads.
    T3code,
}

/// A's way in when it is an agy sign-in account and coordd said it; None for a key, another harness, or a coordd before
/// it said, when the window names no program.
fn agy_way(a: &NativeAccount) -> Option<AgyWay> {
    match a.signed_in_by.as_deref() {
        Some("agy") => Some(AgyWay::Own),
        Some("t3code") => Some(AgyWay::T3code),
        _ => None,
    }
    .filter(|_| is_agy(a) && !a.key)
}

/// What a card says when coordd has taken a key: set in its file, or (an opencode account) taken by opencode's sign-in.
const KEY_SET: &str = "Key set.";

/// The page's state.
#[derive(Default)]
pub struct Accounts {
    /// Each project's coordd's last answer, by the project's place in the list.
    got: HashMap<usize, Result<NativeAccounts, String>>,
    /// When the projects were last asked, or an answer last came: the next round is [`EVERY`] after it.
    asked: Option<Instant>,
    /// How many answers have come, to every project: the Map makes its accounts' keys again when it moves
    /// (map/join.rs).
    came: u64,
    /// The accounts whose usage per model is open.
    open: HashSet<String>,
    /// What coordd last answered to an action on an account (a tier set, a sign-in step), shown on its card.
    said: HashMap<String, Result<String, String>>,
    /// The sign-ins started from a card, by account.
    signin: HashMap<String, SignIn>,
    /// The Add account sheet (sheet.rs): opened by the page's one `Add account`.
    add: Add,
    /// The key fields open, by account: a secret field of its own, none of the message box's paths.
    keys: HashMap<String, TextEdit>,
    /// The keys sent and not answered yet, by account, and whether Cancel was pressed: an opencode account's key waits
    /// meanwhile for opencode's prompt, and Cancel beside "Setting the key…" ends that run ([`cancel_key`]).
    setting: HashMap<String, bool>,
    /// The account whose Remove was pressed: its card asks once before it is sent.
    confirm_remove: Option<String>,
    /// What coordd said last of an account that has no card to say it on (removed, or not made after the sheet
    /// closed), shown at the page's foot.
    removed: Option<String>,
    /// The removed accounts' list is open (under the add entry).
    show_removed: bool,
    /// The key account whose `Remove key` was pressed: its card asks once before it is sent.
    confirm_key: Option<String>,
    /// The account and the reset kind whose `Use` was pressed: its card asks once before it is sent, as the
    /// gateway's card does.
    confirm_reset: Option<(String, String)>,
    /// The `Agents ▸` lists open (0.2 D21), by account id, or [`GATEWAY`] for the gateway's.
    agents_open: HashSet<String>,
    /// The deaf cards whose why and what to do are open (0.2 D22 D), by account id.
    deaf_open: HashSet<String>,
    /// The card whose `⋯` menu is open, by the key [`card_menu`] was given.
    menu: Option<String>,
    /// The name fields open by `Rename`, by account: the name typed until coordd takes it.
    naming: HashMap<String, TextEdit>,
    /// The page's `Check limits` ([`check_all`]): the accounts whose check waits for room among those out.
    checking: VecDeque<String>,
}

/// The Add account sheet's state: the tile picked, and for the core's own two (a Claude subscription, an Anthropic
/// API key) the name, the tier and the account once sent. Accounts are made in the window, nothing typed in a terminal.
#[derive(Default)]
struct Add {
    /// The sheet is open.
    open: bool,
    /// The tile picked; None: the tiles.
    pick: Option<Pick>,
    label: TextEdit,
    /// The place in the tiers offered ([`sheet::offered`]) chosen.
    tier: usize,
    /// The account sent, by id and label, until the sheet closes: its sign-in or its key step shows in the sheet, and
    /// its card carries the same once the accounts answer has it.
    made: Option<(String, String)>,
    /// What coordd said when it refused it.
    error: Option<String>,
    /// The API key typed for a key account before the account is made: moved into the account's key field and sent from
    /// there once coordd made it (one request, never shown again).
    secret: TextEdit,
    /// The `opencode` tile's provider chosen, by its place in [`OPENCODE_PROVIDERS`].
    provider: usize,
    /// The address filled in on Claude's sign-in page for the account it makes (`AccountSignin`'s `email`, Claude
    /// Code's `auth login --email`): a gateway login's being moved here ([`sheet::open_claude`]). None otherwise.
    email: Option<String>,
}

/// A tile of the sheet picked.
#[derive(Clone, PartialEq, Debug)]
enum Pick {
    /// A Claude subscription: Claude Code's own sign-in.
    Claude,
    /// An Anthropic API key.
    Anthropic,
    /// An opencode account: opencode's own sign-in for one of [`OPENCODE_PROVIDERS`], by its key or its browser way.
    Opencode,
    /// An agy account on his Google sign-in, by agy's own sign-in: Google's address, then the code Google shows.
    Agy,
    /// An agy account on a Gemini API key, in the masked field.
    AgyKey,
    /// An extension's tile: the extension's name, the tile's id and label.
    Ext(&'static str, String, String),
}

/// The Add account sheet as he left it, for a window swap (state.rs): opened, the name typed, the tier. The name only
/// on one of the core's two steps. The account already sent is [`add_made_kept`]'s; never a key or a code.
pub(crate) fn add_kept(s: &App) -> Option<(bool, String, usize)> {
    let a = &s.ext.try_get::<Accounts>()?.add;
    let label = if matches!(a.pick, Some(Pick::Claude | Pick::Anthropic)) { a.label.text.clone() } else { String::new() };
    (a.open || !label.is_empty()).then(|| (a.open, label, a.tier))
}

/// Whether the sheet left has the API key tile picked: kept apart from `add_kept`, so an older ui.json reads as it was.
pub(crate) fn add_key_kept(s: &App) -> bool {
    s.ext.try_get::<Accounts>().is_some_and(|a| a.add.pick == Some(Pick::Anthropic))
}

/// Put back after [`add_put_back`]: the API key tile picked, where the sheet was left on one of the core's two.
pub(crate) fn add_key_put_back(s: &mut App, key: bool) {
    if let Some(acc) = s.ext.try_get_mut::<Accounts>()
        && key
        && (acc.add.open || !acc.add.label.text.is_empty())
    {
        acc.add.pick = Some(Pick::Anthropic);
    }
}

/// The account the sheet left on Claude Code's sign-in step had already sent, its id and name, kept apart from
/// `add_kept`. After a swap the sheet comes back on that account, so its `Sign in with Claude` never makes a second
/// account of the name while the first one's sign-in runs on. Only there: a key step's key is never kept.
pub(crate) fn add_made_kept(s: &App) -> Option<(String, String)> {
    let a = &s.ext.try_get::<Accounts>()?.add;
    a.made.clone().filter(|_| a.pick == Some(Pick::Claude))
}

/// Put back after [`add_key_put_back`]: the sheet comes back on the step of the account it had sent, whose sign-in
/// button asks the engine for the sign-in that still runs (it answers that one's link: the code the browser shows
/// still fits) instead of making another account.
pub(crate) fn add_made_put_back(s: &mut App, made: Option<(String, String)>) {
    if let Some(acc) = s.ext.try_get_mut::<Accounts>()
        && acc.add.pick.is_some()
    {
        acc.add.made = made;
    }
}

/// The sheet as it was left: a name typed means one of the core's two tiles was picked (the subscription, unless
/// [`add_key_put_back`] says the key).
pub(crate) fn add_put_back(s: &mut App, open: bool, label: &str, tier: usize) {
    if let Some(acc) = s.ext.try_get_mut::<Accounts>() {
        acc.add.open = open;
        acc.add.label = TextEdit::new(label.to_string());
        acc.add.tier = tier;
        acc.add.pick = (!label.is_empty()).then_some(Pick::Claude);
    }
}

/// The id of an account labelled LABEL (lower case ASCII letters and digits, `-` between words, at most 32 long, so
/// a numbered one stays within coordd's 40: `Store::bad_id`), not one of TAKEN: a second `work` is `work-2`. A label
/// with no ASCII letter or digit gets `account`; an empty label, nothing.
fn id_for(label: &str, taken: &[&str]) -> String {
    if label.trim().is_empty() {
        return String::new();
    }
    let words: Vec<String> = label.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase()).filter(|w| !w.is_empty()).collect();
    let mut base = words.join("-");
    base.truncate(32);
    let base = match base.trim_end_matches('-') {
        "" => "account".to_string(),
        b => b.to_string(),
    };
    if !taken.contains(&base.as_str()) {
        return base;
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| !taken.contains(&id.as_str())).unwrap_or(base)
}

/// A provider the `opencode` tile offers: its id as opencode names it (`-p`), its name, where its key is made where
/// opencode takes a key, and opencode's browser way where the window can carry it (its link on
/// `coord_types::OPENCODE_SIGN_IN_HOSTS`, nothing asked in the terminal).
pub struct OcProvider {
    pub id: &'static str,
    pub name: &'static str,
    /// Where its key is made; None where `auth login -p ID` asks for no key first, so no key is taken for it.
    pub key_from: Option<&'static str>,
    pub way: Option<(&'static str, &'static str)>,
}

/// The `opencode` tile's providers, his Go plan first (opencode 1.18.34: `opencode-go`, `opencode` and `openai`, the
/// ChatGPT way that waits for a code typed in the browser). OpenAI takes no key here: `auth login -p openai` asks
/// "Login method" first, so it signs in by ChatGPT alone.
pub static OPENCODE_PROVIDERS: [OcProvider; 3] = [
    OcProvider { id: "opencode-go", name: "OpenCode Go", key_from: Some("From opencode.ai → API keys"), way: None },
    OcProvider { id: "opencode", name: "OpenCode Zen", key_from: Some("From opencode.ai/auth → API keys."), way: None },
    OcProvider { id: "openai", name: "OpenAI", key_from: None, way: Some(("ChatGPT Pro/Plus (headless)", "Sign in with ChatGPT")) },
];

/// The provider of opencode account A, as the tile offers it; None for another's or one the tile does not offer.
fn oc_provider(a: &NativeAccount) -> Option<&'static OcProvider> {
    OPENCODE_PROVIDERS.iter().find(|p| a.provider.as_deref() == Some(p.id))
}

/// Whether A is an opencode account (opencode's own sign-in keeps its key or login, under the account's folder).
/// The question for an account whose agents keep it while their sessions run: WHO (ONE agent or more) on LABEL, stopped
/// first.
fn stop_first(who: &str, one: bool, label: &str) -> String {
    format!("{who} {} on {label}; stop {} first", if one { "runs" } else { "run" }, if one { "it" } else { "them" })
}

fn is_oc(a: &NativeAccount) -> bool {
    a.harness == "opencode"
}

/// Where opencode account A's key is made, when its key goes in the field: its provider's words, or the field's plain
/// ones for a provider the tile does not offer; None for one that takes no key (OpenAI).
fn oc_key_from(a: &NativeAccount) -> Option<&'static str> {
    oc_provider(a).map_or(Some("Its API key:"), |p| p.key_from)
}

/// Whether opencode account A's card draws OpenCode Go's usage (its 5-hour, weekly and monthly windows as Go's page
/// names them): a Go account signed in, from a coordd that reads that page (`GO_USAGE_PROTOCOL`).
fn go_windows(s: &App, a: &NativeAccount) -> bool {
    is_oc(a) && a.provider.as_deref() == Some("opencode-go") && a.signed_in && s.proj().is_some_and(|p| p.protocol >= coord_types::GO_USAGE_PROTOCOL)
}

/// Whether account A goes on an API key: a key account, or an opencode account whose provider takes a key. Its card
/// says why it is out as a key card does (`Key refused`, `No credit`, `At its limit`).
fn keyed(a: &NativeAccount) -> bool {
    a.key || is_oc(a) && oc_key_from(a).is_some()
}

/// The row's name of a window of some models, M: an agy account's under t3code's own label ("Gemini · 5-hour"), which
/// says its length (AGY); one of `/usage`'s weeks of one model as `Weekly (Fable)`.
fn week_name(m: &ModelWeek, agy: bool) -> String {
    if agy { m.model.clone() } else { format!("Weekly ({})", m.model) }
}

/// An agy account's windows, WEEKS, under the limits t3code names in their labels ("Gemini · 5-hour": the limit, then
/// the window's length): each limit once, in the order t3code first gives it, with its windows by their own words and
/// their whole labels. A label with no ` · ` is a window of its own, under no name. On a narrow card a whole label
/// lost its length ("Claude & GPT (shared) · 5-…").
fn agy_groups(weeks: &[ModelWeek]) -> Vec<(&str, Vec<(&str, &ModelWeek)>)> {
    let mut groups: Vec<(&str, Vec<(&str, &ModelWeek)>)> = Vec::new();
    for m in weeks {
        let (limit, own) = m.model.split_once(" · ").unwrap_or(("", &m.model));
        match groups.iter_mut().find(|(l, _)| *l == limit) {
            Some((_, ws)) => ws.push((own, m)),
            None => groups.push((limit, vec![(own, m)])),
        }
    }
    groups
}

/// What an agy sign-in account signed in with, by t3code's kind of it (`NativeAccount::auth_method`), in t3code's
/// words: a personal Google account, or Gemini Enterprise. t3code names no Google AI plan, only its limits. An older
/// coordd says no kind: its Google sign-in.
fn agy_sign_in(kind: Option<&str>) -> &'static str {
    match kind {
        Some("oauth-personal") => "personal Google account",
        Some("oauth-business") => "Gemini Enterprise sign-in",
        _ => "Google sign-in",
    }
}

/// Claude Code's harness, as an account names it (`NativeAccount::harness`).
pub const CLAUDE: &str = "claude";

/// The harnesses on the overview, in its order, each with its logo and its heading: its accounts and its tiers under
/// it ([`tiers`]). Public for the gateway's Accounts tab, which gives each harness but Claude Code's a family of its own
/// under the same logo and heading.
pub const FAMILIES: [(&str, &rune::Icon, &str); 3] = [(CLAUDE, &logos::CLAUDE, "Claude"), ("opencode", &logos::OPENCODE, "OpenCode"), ("agy", &logos::ANTIGRAVITY, "Antigravity")];

/// Whether A is an agy account (Google's Antigravity CLI): one on a Google sign-in (agy's own, or t3code's from before
/// it, [`agy_way`]), or one on a Gemini API key.
fn is_agy(a: &NativeAccount) -> bool {
    a.harness == "agy"
}

/// The program that signs account A out, as the window names it: an agy account's is agy's own, or t3code's on one
/// t3code signed in ([`agy_way`]).
fn signs(a: &NativeAccount) -> &'static str {
    match a.harness.as_str() {
        "opencode" => "opencode",
        "agy" if agy_way(a) == Some(AgyWay::T3code) => "t3code",
        "agy" => "agy",
        _ => "Claude Code",
    }
}

/// Whether the current project's coordd makes agy accounts (`AGY_ACCOUNTS`): before it, an `AccountNew` for one would
/// make a Claude Code account, so the two Antigravity tiles are not offered.
fn agy_served(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::AGY_ACCOUNTS)
}

/// Whether the current project's coordd makes opencode accounts (`OPENCODE_ACCOUNTS`): before it, an `AccountNew` for
/// one would make a Claude Code account, so the tile is not offered.
fn opencode_served(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::OPENCODE_ACCOUNTS)
}

/// A sign-in started from an account's card or the sheet: Claude Code's own, opencode's by a provider's browser way
/// (its link on `coord_types::OPENCODE_SIGN_IN_HOSTS`), or agy's own Google sign-in of an agy account (on
/// `coord_types::AGY_SIGN_IN_HOSTS`), run by coordd (`coord account signin`). He copies its URL into the browser he
/// means; the window never opens it.
struct SignIn {
    /// The URL coordd answered; None until it answers.
    url: Option<String>,
    /// The code the browser showed, when it shows one instead of finishing by itself.
    code: TextEdit,
    /// The code is handed in and coordd's answer awaited (it comes when the sign-in ends).
    sending: bool,
    /// The harness of the account it signs in, given where it is started: there is no sign-in without one.
    whose: Whose,
    /// What opencode said to do on that page, the answer's second line (`Enter code: …`).
    said: Option<String>,
}

/// The harness of the account a sign-in is for, by `NativeAccount::harness`.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Whose {
    Claude,
    /// An opencode account, signing in by opencode's browser way (its method, `ChatGPT Pro/Plus (headless)`).
    Opencode(&'static str),
    /// An agy account, signing in with Google by agy's own sign-in: the code Google shows is always pasted back.
    Agy,
}

impl SignIn {
    fn new(whose: Whose) -> SignIn {
        SignIn { url: None, code: TextEdit::new(""), sending: false, whose, said: None }
    }

    /// Whether its link URL is a sign-in page its harness is known to use: any for a Claude Code account; for opencode
    /// only `https` to a host of `OPENCODE_SIGN_IN_HOSTS`; for agy, of `AGY_SIGN_IN_HOSTS`. Any other is shown with a
    /// warning ([`signing_in`]).
    fn known(&self, url: &str) -> bool {
        match self.whose {
            Whose::Claude => true,
            Whose::Opencode(_) => coord_types::opencode_link_opens(url),
            Whose::Agy => coord_types::agy_link_opens(url),
        }
    }

    /// The program whose sign-in it is, as the window names it.
    fn program(&self) -> &'static str {
        match self.whose {
            Whose::Claude => "Claude Code",
            Whose::Opencode(_) => "opencode",
            Whose::Agy => "agy",
        }
    }
}

/// An action's request, kept for its reply: the account it was on, and which.
struct Act(String, What);

/// The actions a card sends.
#[derive(Clone, Copy, PartialEq)]
enum What {
    /// An account made, and the sign-in that starts on it next (Claude Code's, opencode's by a browser way, or
    /// t3code's); None: the key typed goes to it instead. The request carries it: the sheet may have closed, or be on
    /// another tile, by the time coordd answers.
    New { signs: Option<Whose> },
    Tier,
    Signin,
    Code,
    /// His `Check whether it is signed in` or `Check limits`: one `Request::AccountCheck` ([`check`]), whose answer has
    /// the accounts asked again ([`Limits`](What::Limits)).
    Check,
    /// The accounts asked of every project now, after a check's answer; answered as the poll is.
    Limits,
    /// His `Use` of one of Claude Code's resets on a built-in account (`Request::AccountReset`).
    Reset,
    Cancel,
    Clear,
    Key,
    /// The Cancel of a key while opencode's sign-in waits for it (`AccountCancel`).
    KeyCancel,
    Remove,
    All,
    PutBack,
    KeyRemove,
    Keep,
    /// His `Rename`, `Turn off` or `Turn on` (`Request::AccountEdit`).
    Edit,
}

/// While the page shows, ask every project's coordd every [`EVERY`]; nothing while another page shows
/// (N-1933.p1: an answer took 22 to 70 s, asked while he read an agent). An agent's line names its account by the label
/// last answered, else by its id ([`agent_account`]).
fn poll(s: &mut App, tx: &mut Tx) -> bool {
    let lost = checks_lost(s);
    if matches!(&s.sel, crate::Sel::Ext(n, _) if n == NAME) {
        keep_asking(s, tx);
    }
    lost
}

/// Ask every project's coordd for the accounts when [`EVERY`] has passed since the last time, and wake the window
/// for the next, while the page shows: this one, or the proxy's Accounts tab. The Map's corner draws the last answer
/// and asks nothing (N-1933.p9).
pub fn keep_asking(s: &mut App, tx: &mut Tx) {
    // while the sheet waits for a sign-in or a key to land, as often as the gateway asks after its logins: the sheet
    // says "Signed in" when coordd does, not half a minute later
    let st = s.ext.get::<Accounts>();
    let every = if st.add.open && st.add.made.is_some() { SOON } else { EVERY };
    s.wake_every(every, tx);
    if s.ext.get::<Accounts>().asked.is_some_and(|t| t.elapsed() < every) {
        return;
    }
    s.ext.get_mut::<Accounts>().asked = Some(Instant::now());
    // a project whose last round's answer has not come is not asked again: a slow coordd (22 to 70 s, N-1933.p1) gets
    // one request at a time, never a pile
    let waiting = |p: &crate::Proj| p.pending.values().any(|w| matches!(w, crate::Pending::Ext { ext, data } if *ext == NAME && data.is::<()>()));
    for p in s.projects.iter_mut().filter(|p| served(p) && !waiting(p)) {
        p.ask(&Request::Accounts { json: true }, NAME, Box::new(()));
    }
}

/// The accounts as all projects' coordds answered them, merged ([`merged`]), and the month their usage is of; none in
/// a build without this extension.
pub fn shown(s: &App) -> (Vec<NativeAccount>, String) {
    if crate::ext::get(NAME).is_none() {
        return (Vec::new(), String::new());
    }
    let st = s.ext.get::<Accounts>();
    (merged(&st.got), st.got.values().find_map(|r| Some(r.as_ref().ok()?.month.clone())).unwrap_or_default())
}

/// Project PI's coordd's last answer, for the Map's corner (map/join.rs; the cards read them merged, [`shown`]); none
/// in a build without this extension, before the first answer, or when it was refused.
pub(crate) fn answer(s: &App, pi: usize) -> Option<&NativeAccounts> {
    s.ext.try_get::<Accounts>()?.got.get(&pi)?.as_ref().ok()
}

/// How many answers have come, to every project ([`answer`]): the Map makes its accounts' keys again when it moves.
pub(crate) fn answers(s: &App) -> u64 {
    s.ext.try_get::<Accounts>().map_or(0, |st| st.came)
}

/// The label of account ID as the accounts answer says it, if the page has asked; None in a build without this
/// extension or before the first answer.
pub fn label_of(s: &App, id: &str) -> Option<String> {
    crate::ext::get(NAME)?;
    s.ext.get::<Accounts>().got.values().filter_map(|r| r.as_ref().ok()).flat_map(|n| &n.accounts).find(|a| a.id == id).map(|a| a.label.clone())
}

/// The account agent A runs on, as its line names it: the account's label, else its id until the accounts are asked;
/// None for an agent through the model route.
pub fn agent_account(s: &App, a: &coord_types::AgentView) -> Option<String> {
    let id = a.account.as_ref()?;
    Some(label_of(s, id).unwrap_or_else(|| id.clone()))
}

/// Agent A's Account cell in the Agents table, and its hover:
/// - the account it runs on, by its label;
/// - `waiting`, with until when coordd gave a time (`account_until`), for one waiting for an account
///   (`account_waiting`);
/// - `Gateway` for one he put on his accounts that runs on none of them now and is not stopped;
/// - nothing for one through the model route (the gateway's list, [`on_gateway`], has those), or stopped.
/// The hover is its last move ([`moved`]), else coordd's note.
pub fn account_cell(s: &App, a: &AgentView) -> (String, Option<String>) {
    let note = a.account_note.as_deref();
    let shown = match agent_account(s, a) {
        Some(label) => label,
        None if a.account_waiting => match a.account_until {
            Some(t) => format!("waiting · until {}", crate::when::secs(t)),
            None => "waiting".to_string(),
        },
        None if a.native && a.state != "stopped" => "Gateway".to_string(),
        None => String::new(),
    };
    (shown, moved(s, a).or(note.map(str::to_string)))
}

/// Agent A's last move, as coordd's registry line gave it: `since 08:40 · from Team seat: its account does not take
/// messages`, the account it came from by its label; each part left out when coordd said none. None before its
/// first move.
fn moved(s: &App, a: &AgentView) -> Option<String> {
    let since = a.account_since.as_deref().map(|t| format!("since {}", crate::when::at(t)));
    let from = a.account_from.as_deref().map(|id| label_of(s, id).unwrap_or_else(|| id.to_string()));
    let why = match (from, a.account_why.as_deref()) {
        (Some(f), Some(w)) => Some(format!("from {f}: {w}")),
        (Some(f), None) => Some(format!("from {f}")),
        (None, w) => w.map(str::to_string),
    };
    let parts: Vec<String> = [since, why].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// An agent as an `Agents ▸` list shows it (0.2 D21): `coord.agent`, its id where its project is open in this
/// window, and since when and why it runs where it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    pub name: String,
    pub id: Option<String>,
    pub detail: Option<String>,
    /// Its view says it is stopped: still bound to the account, but not running on it.
    pub stopped: bool,
}

/// The view of agent NAME of coord COORD, where a project of this window is that coord.
fn view_of<'a>(s: &'a App, coord: &str, name: &str) -> Option<&'a AgentView> {
    let st = s.ext.get::<Accounts>();
    let (i, _) = st.got.iter().find(|(_, r)| matches!(r, Ok(n) if n.coord == coord))?;
    s.projects.get(*i)?.live.agents.iter().find(|a| a.name == name)
}

/// Agent X of an account's list: its view's last move when there is one, else since when it is bound there; with
/// coordd's note of what is due. One its view says is stopped says so first: it is still bound there (an agent keeps
/// its account over its restarts) but runs on it no more. One of a coord this window has no view of is not known to be
/// stopped.
fn listed(s: &App, x: &NativeAgent) -> Listed {
    let v = view_of(s, &x.coord, &x.agent);
    let stopped = v.is_some_and(|v| v.state == "stopped");
    let since = v.and_then(|v| moved(s, v)).unwrap_or_else(|| format!("since {}", crate::when::at(&x.since)));
    let since = if stopped { format!("stopped · {since}") } else { since };
    let detail = Some(match &x.note {
        Some(n) => format!("{since} · {n}"),
        None => since,
    });
    Listed { name: format!("{}.{}", x.coord, x.agent), id: v.map(|v| v.id.clone()), detail, stopped }
}

/// The key the gateway's list is kept open under in `agents_open` (no account id has a `*`).
const GATEWAY: &str = "*gateway";

/// A card's `Agents  18 ▸` line: LABEL, the count (WORD after it, for the gateway's `17 agents`), and a click that
/// opens the list of AGENTS under it, each by name and id with since when and why. Where some are stopped the count says
/// how many run (`Agents  2 · 1 running`) and the stopped ones are listed under the running ones. KEY: its fold's, in
/// `agents_open`.
fn agents_fold(s: &App, k: &Kit, key: &str, label: &str, word: bool, agents: &[Listed]) -> El {
    let open = s.ext.get::<Accounts>().agents_open.contains(key);
    let n = agents.len();
    let count = match (n, word) {
        (0, _) => "no agent".to_string(),
        (1, true) => "1 agent".to_string(),
        (n, true) => format!("{n} agents"),
        (n, false) => n.to_string(),
    };
    let stopped = agents.iter().filter(|x| x.stopped).count();
    let count = if stopped > 0 { format!("{count} · {} running", n - stopped) } else { count };
    let head = row().w_full().gap(12.0).center_y().child(text(label.to_string()).font(k.ty.small).color(k.c.text_muted).w(if word { 128.0 } else { 64.0 }));
    if n == 0 {
        return head.child(text(count).font(k.ty.small).color(k.c.text_base));
    }
    let el_key = format!("agents-fold-{key}");
    let key = key.to_string();
    let toggle = Button::new(format!("{count} {}", if open { "▾" } else { "▸" }))
        .control_size(ControlSize::Regular)
        .variant(ButtonVariant::Ghost)
        .on_press(move |s: &mut App| {
            let o = &mut s.ext.get_mut::<Accounts>().agents_open;
            if !o.remove(&key) {
                o.insert(key.clone());
            }
        })
        .build(k)
        .key(el_key);
    let head = head.child(toggle);
    if !open {
        return head;
    }
    let list = agents.iter().filter(|x| !x.stopped).chain(agents.iter().filter(|x| x.stopped)).map(|x| {
        let name = match &x.id {
            Some(id) => format!("{} ({id})", x.name),
            None => x.name.clone(),
        };
        col().w_full().gap(1.0).child(text(name).font(k.ty.small).color(k.c.text_base).truncate()).children(x.detail.as_ref().map(|d| faint(k, d.clone())))
    });
    col().w_full().gap(6.0).child(head).child(col().w_full().gap(6.0).padding(8.0).radius(k.radius.sm).fill(k.c.layer_02).children(list))
}

/// The gateway's line, `Through the gateway  17 agents ▸`, with its list: for the gateway extension's Accounts tab,
/// which takes this page's place in a window built with it (a window without it draws [`gateway_card`]). None where
/// no coordd answered that it has a gateway, or where this build has no accounts extension (the gateway's tab is
/// drawn without it in the gateway's own tests).
pub fn gateway_agents(s: &App, k: &Kit) -> Option<El> {
    s.ext.try_get::<Accounts>()?;
    Some(agents_fold(s, k, GATEWAY, "Through the gateway", true, &on_gateway(s)?))
}

/// Ask the current project's coordd to put account ID in tier TIER (accounts are shared by his coordds: any of them
/// writes it); the accounts are asked again when it answers.
pub fn set_tier(s: &mut App, id: &str, tier: i64) {
    act(s, id, What::Tier, Request::AccountTier { id: id.to_string(), tier });
}

/// Send REQ, action WHAT on account ID, to the current project's coordd; its answer is kept for the card.
fn act(s: &mut App, id: &str, what: What, req: Request) {
    let cur = s.cur;
    let refused = match s.projects.get_mut(cur) {
        None => Some("The engine is not reached"),
        Some(p) if !served(p) => Some("This project's engine is from before the accounts"),
        Some(p) => (!p.ask(&req, NAME, Box::new(Act(id.to_string(), what)))).then_some("The engine is not reached"),
    };
    let st = s.ext.get_mut::<Accounts>();
    match refused {
        None => st.said.remove(id),
        Some(why) => st.said.insert(id.to_string(), Err(why.into())),
    };
}

/// Account ID's limit cleared: what its card's `It can take requests` and `Try again` press, and the Map's inspector's
/// `Clear cooldown`.
pub(crate) fn clear(s: &mut App, id: &str) {
    act(s, id, What::Clear, Request::AccountClear { id: id.to_string() });
}

/// Of account ID's clear, as its card draws it: whether it is with a coordd and not answered, and what was refused
/// last (`act`'s own words, or coordd's): the Map's inspector draws its `Clear cooldown` with them.
pub(crate) fn clearing(s: &App, id: &str) -> (bool, Option<String>) {
    let refused = s.ext.try_get::<Accounts>().and_then(|st| st.said.get(id)).and_then(|r| r.as_ref().err().cloned());
    (asking(s, id, What::Clear), refused)
}

/// Action WHAT on account ID is with a coordd and not answered: the ask is among a project's pending ones. Its answer
/// takes it out, and so does the pipe's end, which brings no answer (lib.rs `Incoming::Down`), so a ring drawn from it
/// does not turn for ever.
fn asking(s: &App, id: &str, what: What) -> bool {
    s.projects.iter().flat_map(|p| p.pending.values()).any(|w| matches!(w, crate::Pending::Ext { data, .. } if data.downcast_ref::<Act>().is_some_and(|a| a.0 == id && a.1 == what)))
}

/// His check of account ID is out: the card's ring with `Querying`, as the gateway's card says it, from his press until
/// coordd has answered and then every project's coordd with the accounts it recorded (0.2 D20: refresh shows it is
/// happening).
pub fn querying(s: &App, id: &str) -> bool {
    asking(s, id, What::Check) || asking(s, id, What::Limits)
}

/// Whether the current project's coordd's check also reads a signed-in account's windows (`ACCOUNT_RESET`): before
/// it, the card offers no `Check limits`.
fn reads_limits(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_RESET)
}

/// How many of the page's checks are out at once ([`check_all`]), as the gateway's `Refresh` asks four of its logins at
/// a time (proxy quota_page.rs `refresh_all`): each starts Claude Code in an account's folder.
const CHECKS_AT_ONCE: usize = 4;

/// Whether built-in account A's card offers `Check limits` at NOW: a Claude Code sign-in or an agy one,
/// signed in, its login neither run out nor refused, from a coordd whose check reads the windows ([`reads_limits`]);
/// or an OpenCode Go account whose card draws Go's usage ([`go_windows`]) and whose key coordd keeps to read it.
fn checks(s: &App, a: &NativeAccount, now: i64) -> bool {
    (a.signed_in && !a.key && !is_oc(a) && !login_ran_out(a, now) && !login_refused(a, now) && reads_limits(s)) || (go_windows(s, a) && a.usage_key)
}

/// The page's `Check limits`: every card that offers it and is not being checked is checked, [`CHECKS_AT_ONCE`] at a
/// time and the rest as answers come ([`check_next`]); on his press only, and nothing is used.
pub fn check_all(s: &mut App) {
    let now = now_s();
    let ids: VecDeque<String> = merged(&s.ext.get::<Accounts>().got).into_iter().filter(|a| checks(s, a, now) && !querying(s, &a.id)).map(|a| a.id).collect();
    s.ext.get_mut::<Accounts>().checking = ids;
    check_next(s);
}

/// How many checks of cards are out with a coordd: the page's ([`check_all`]) and his presses on a card alike.
fn checks_out(s: &App) -> usize {
    s.projects.iter().flat_map(|p| p.pending.values()).filter(|w| matches!(w, crate::Pending::Ext { data, .. } if data.downcast_ref::<Act>().is_some_and(|a| a.1 == What::Check))).count()
}

/// Send the page's checks that wait ([`check_all`]) while fewer than [`CHECKS_AT_ONCE`] are out with a coordd.
fn check_next(s: &mut App) {
    while checks_out(s) < CHECKS_AT_ONCE {
        let Some(id) = s.ext.get_mut::<Accounts>().checking.pop_front() else { return };
        check(s, &id);
    }
}

/// Drop the page's waiting checks when none is out: the pipe's end took those out unanswered (lib.rs
/// `Incoming::Down`), and only an answer sends the next, so the bar would say `Checking…` for good. As the pipe's end
/// takes an ask out ([`asking`]), it takes the page's run with it; true when it did.
fn checks_lost(s: &mut App) -> bool {
    let lost = checks_out(s) == 0 && !s.ext.get::<Accounts>().checking.is_empty();
    if lost {
        s.ext.get_mut::<Accounts>().checking.clear();
    }
    lost
}

/// The page's `Check limits` ([`check_all`]) where a card offers it, `Checking…` while its checks run. Public for the
/// gateway's Accounts tab, whose toolbar it stands in.
pub fn check_all_button(s: &App, k: &Kit) -> Option<El> {
    let now = now_s();
    let st = s.ext.get::<Accounts>();
    let accounts: Vec<NativeAccount> = merged(&st.got).into_iter().filter(|a| checks(s, a, now)).collect();
    if accounts.is_empty() {
        return None;
    }
    let running = !st.checking.is_empty() || accounts.iter().any(|a| querying(s, &a.id));
    let button = Button::new(if running { "Checking…" } else { "Check limits" }).control_size(ControlSize::Regular).variant(ButtonVariant::Ghost).help("Read every account's limits; uses nothing");
    Some(if running { button.disabled(true) } else { button.on_press(|s: &mut App| check_all(s)) }.build(k))
}

/// `Check whether it is signed in` and `Check limits` on built-in account ID's card, one request: the current
/// project's coordd asks Claude Code whether it is signed in and reads a signed-in one's windows by Claude Code's own
/// `/usage` in the account's folder, which sends nothing to a model. Its answer has every coordd asked for the accounts
/// at once, so the card shows what was just read.
fn check(s: &mut App, id: &str) {
    act(s, id, What::Check, Request::AccountCheck { id: id.to_string() });
}

/// Whether the current project's coordd takes `Rename` and `Turn off` · `Turn on` (`ACCOUNT_EDIT`).
fn edits(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_EDIT)
}

/// What `Check limits` on account A's card reads: an OpenCode Go account's, Go's usage page ([`go_windows`]); an agy
/// account t3code signed in, what t3code says of its sign-in, models and limits; another agy account's, whether agy
/// takes its sign-in or key and the models it offers on it; a built-in Claude account's, from `ACCOUNT_EDIT` on
/// also the resets `/limit-reset` lists, closed with Escape so nothing is chosen or used.
fn check_help(s: &App, a: &NativeAccount) -> &'static str {
    if is_oc(a) {
        "Read its plan's usage; uses nothing"
    } else if agy_way(a) == Some(AgyWay::T3code) {
        "Read its sign-in and limits; uses nothing"
    } else if is_agy(a) {
        "Read its sign-in and models; uses nothing"
    } else if edits(s) {
        "Read its limits and resets; uses nothing"
    } else {
        "Read its limits; uses nothing"
    }
}

/// His change of account ID's own settings, LABEL or OFF, the rest as they were (`Request::AccountEdit`): the gateway
/// card's `Turn off` · `Turn on` on his own accounts too, and a name of his.
fn edit(s: &mut App, id: &str, label: Option<String>, off: Option<bool>) {
    act(s, id, What::Edit, Request::AccountEdit { id: id.to_string(), tier: None, label, off });
}

/// Send the name typed in account ID's `Rename` field.
fn rename(s: &mut App, id: &str) {
    let Some(label) = s.ext.get::<Accounts>().naming.get(id).map(|f| f.text.trim().to_string()).filter(|l| !l.is_empty()) else { return };
    edit(s, id, Some(label), None);
}

/// Account A's reset row: the resets Claude Code's `/limit-reset` offers in its folder that he may use, as coordd
/// gives them (a kind refused at this limit is left out until the limit ends, the account is back, or a notice names
/// resets again); whether a press is under way (SENT); the refusal's kind and Claude Code's words, and whether that
/// press is not known to be used, so its kind may be pressed again (`ResetTry::again`); and Claude Code's words once
/// applied.
struct Resets<'a> {
    usable: Vec<&'a ResetOffer>,
    applying: bool,
    refused: Option<(&'a str, &'a str)>,
    again: bool,
    done: Option<&'a str>,
}

impl Resets<'_> {
    /// Nothing offered, pressed, refused or applied: the card draws no row, not a "none".
    fn is_empty(&self) -> bool {
        self.usable.is_empty() && !self.applying && self.refused.is_none() && self.done.is_none()
    }
}

fn resets(a: &NativeAccount, sent: bool) -> Resets<'_> {
    let refused = a.reset.as_ref().and_then(|r| Some((r.kind.as_str(), r.refused.as_deref()?)));
    let done = a.reset.as_ref().and_then(|r| r.done.as_deref());
    Resets {
        usable: a.resets.iter().collect(),
        applying: sent || a.reset.as_ref().is_some_and(|r| r.refused.is_none() && r.done.is_none()),
        refused,
        again: a.reset.as_ref().is_some_and(|r| r.again),
        done,
    }
}

/// Start the sign-in WHOSE of account ID: its URL comes in the answer, and for opencode's browser way (its method
/// sent) what to do there. Claude Code's for the account the sheet made fills in the sheet's address on Claude's page
/// ([`Add::email`]), a card's none.
fn sign_in(s: &mut App, id: &str, whose: Whose) {
    let st = s.ext.get_mut::<Accounts>();
    st.signin.insert(id.to_string(), SignIn::new(whose));
    let email = st.add.email.clone().filter(|_| whose == Whose::Claude && st.add.made.as_ref().is_some_and(|(m, _)| m == id));
    let method = match whose {
        Whose::Opencode(way) => Some(way.to_string()),
        Whose::Claude | Whose::Agy => None,
    };
    act(s, id, What::Signin, Request::AccountSignin { id: id.to_string(), email, method });
}

/// Add the account the sheet's form says: coordd makes it (`AccountNew`), and its answer starts the sign-in, or sends
/// the key typed.
fn add(s: &mut App) {
    let (accounts, _) = shown(s);
    let gone = removed_all(&s.ext.get::<Accounts>().got);
    let taken = taken(&accounts, &gone);
    let Some(pick) = s.ext.get::<Accounts>().add.pick.clone() else { return };
    let offered = sheet::offered(s, sheet::harness_of(&pick));
    let st = s.ext.get_mut::<Accounts>();
    let label = st.add.label.text.trim().to_string();
    let id = id_for(&label, &taken);
    let oc = pick == Pick::Opencode;
    let provider = &OPENCODE_PROVIDERS[st.add.provider.min(OPENCODE_PROVIDERS.len() - 1)];
    // a provider that takes no key (OpenAI) is given none, whatever the field held
    let typed = !st.add.secret.text.trim().is_empty() && !(oc && provider.key_from.is_none());
    // a subscription tile's account goes on to its harness's own sign-in; an opencode one by the key typed, else by its
    // provider's browser way; a key tile's by its key. With neither a sign-in nor a key it is not made
    let signs = match pick {
        Pick::Claude => Some(Whose::Claude),
        Pick::Agy => Some(Whose::Agy),
        Pick::Opencode if !typed => provider.way.map(|w| Whose::Opencode(w.0)),
        Pick::Opencode | Pick::Anthropic | Pick::AgyKey | Pick::Ext(..) => None,
    };
    if id.is_empty() || st.add.made.is_some() || matches!(pick, Pick::Ext(..)) || (signs.is_none() && !typed) {
        return;
    }
    let tier = offered[st.add.tier.min(offered.len() - 1)].1;
    st.add.made = Some((id.clone(), label.clone()));
    st.add.error = None;
    // opencode keeps the key it is given itself: its request asks for no key file
    let (harness, provider, key) = match pick {
        Pick::Opencode => (Some("opencode".to_string()), Some(provider.id.to_string()), false),
        Pick::Agy | Pick::AgyKey => (Some("agy".to_string()), None, signs.is_none()),
        _ => (None, None, signs.is_none()),
    };
    act(s, &id, What::New { signs }, Request::AccountNew { id: id.clone(), label: Some(label), tier, key, harness, provider });
    // not sent: what act said goes to the form
    let st = s.ext.get_mut::<Accounts>();
    if let Some(Err(e)) = st.said.remove(&id) {
        st.add.made = None;
        st.add.error = Some(e);
    }
}

/// Whether the current project's coordd takes the key field (`ACCOUNT_KEY`): before it an `AccountNew` with a key would
/// make a sign-in account, so the kind is not offered.
fn keys_served(s: &App) -> bool {
    s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_KEY)
}

/// Send the key typed in ID's field: the text is moved out of the field into a [`Secret`] (no copy is made here), the
/// field goes, and the request with it is dropped once written (the Secret's bytes overwritten then).
fn send_key(s: &mut App, id: &str) {
    let Some(mut field) = s.ext.get_mut::<Accounts>().keys.remove(id) else { return };
    let key = Secret::new(std::mem::take(&mut field.text));
    drop(field);
    if key.expose().trim().is_empty() {
        s.ext.get_mut::<Accounts>().keys.insert(id.to_string(), TextEdit::new(""));
        return;
    }
    act(s, id, What::Key, Request::AccountKeyStore { id: id.to_string(), key });
    let st = s.ext.get_mut::<Accounts>();
    if !st.said.contains_key(id) {
        st.setting.insert(id.to_string(), false);
    }
}

/// Cancel opencode account ID's key while coordd hands it to opencode's sign-in: the run is ended (`AccountCancel`)
/// and nothing is typed into it; the refusal that follows is the cancel's, so the account stays not signed in and its
/// field comes back empty, with no fault said.
fn cancel_key(s: &mut App, id: &str) {
    match s.ext.get_mut::<Accounts>().setting.get_mut(id) {
        Some(cancelled) if !*cancelled => *cancelled = true,
        _ => return,
    }
    act(s, id, What::KeyCancel, Request::AccountCancel { id: id.to_string() });
    // not sent: the key's answer is said as it comes
    let st = s.ext.get_mut::<Accounts>();
    if st.said.contains_key(id)
        && let Some(cancelled) = st.setting.get_mut(id)
    {
        *cancelled = false;
    }
}

/// "Setting the key…" while coordd hands opencode account ID's key to opencode, with its Cancel ([`cancel_key`]);
/// CANCELLED: pressed, and the run's end is awaited.
fn setting_key(k: &Kit, id: &str, cancelled: bool) -> El {
    let i = id.to_string();
    let cancel = Button::new(if cancelled { "Cancelling…" } else { "Cancel" }).variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).disabled(cancelled).on_press(move |s: &mut App| cancel_key(s, &i)).build(k);
    row().w_full().gap(8.0).center_y().child(faint(k, "Setting the key…").grow(1.0)).child(cancel)
}

/// The key field of account ID: masked (rune's secure input: no undo, copy, cut, IME or accessibility text, its
/// buffers zeroed), its own state, `Add key` / `Replace key` sending it; Cancel when a key is set already.
fn key_field(k: &Kit, id: &str, field: &TextEdit, has_key: bool) -> El {
    let (i1, i2, i3) = (id.to_string(), id.to_string(), id.to_string());
    let input = TextInput::new(field)
        .secure()
        .placeholder("API key")
        .width(None)
        .on_edit(move |s: &mut App| s.ext.get_mut::<Accounts>().keys.entry(i1.clone()).or_insert_with(|| TextEdit::new("")))
        .on_submit(move |s: &mut App, _| send_key(s, &i2))
        .build(k);
    let send = Button::new(if has_key { "Replace key" } else { "Add key" }).variant(ButtonVariant::Contrast).control_size(ControlSize::Regular).disabled(field.text.trim().is_empty()).on_press(move |s: &mut App| send_key(s, &i3)).build(k);
    let mut r = row().w_full().gap(6.0).center_y().child(input.grow(1.0)).child(send);
    if has_key {
        let i = id.to_string();
        r = r.child(Button::new("Cancel").variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).on_press(move |s: &mut App| drop(s.ext.get_mut::<Accounts>().keys.remove(&i))).build(k));
    }
    r
}

/// Account ID's `Rename` field: the name as it stands, `Save` sending it (SENDING: sent and not answered), and
/// `Cancel`; it closes once coordd took the name.
fn name_field(k: &Kit, id: &str, field: &TextEdit, sending: bool) -> El {
    let (i1, i2, i3, i4) = (id.to_string(), id.to_string(), id.to_string(), id.to_string());
    let input = TextInput::new(field)
        .placeholder("Name")
        .width(None)
        .on_edit(move |s: &mut App| s.ext.get_mut::<Accounts>().naming.entry(i1.clone()).or_insert_with(|| TextEdit::new("")))
        .on_submit(move |s: &mut App, _| rename(s, &i2))
        .build(k);
    let save = Button::new("Save").variant(ButtonVariant::Contrast).control_size(ControlSize::Regular).disabled(sending || field.text.trim().is_empty()).on_press(move |s: &mut App| rename(s, &i3)).build(k);
    let cancel = Button::new("Cancel").variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).on_press(move |s: &mut App| drop(s.ext.get_mut::<Accounts>().naming.remove(&i4))).build(k);
    row().w_full().gap(6.0).center_y().child(input.grow(1.0)).child(save).child(cancel)
}

/// The key the switch's answers are kept under in `said` (no account id has a `*`).
const ALL: &str = "*all*";

/// The projects whose coordd takes the switch `All agents use my accounts` (`ACCOUNTS_ALL`), by place.
fn all_served(s: &App) -> Vec<usize> {
    s.projects.iter().enumerate().filter(|(_, p)| p.protocol >= coord_types::ACCOUNTS_ALL).map(|(i, _)| i).collect()
}

/// Turn the switch to ON in every coordd that takes it (each coordd has its own; one switch on the page sets them
/// all); what the last one answers is said under it.
fn set_all(s: &mut App, on: bool) {
    for i in all_served(s) {
        if let Some(p) = s.projects.get_mut(i) {
            p.ask(&Request::AccountsAll { on }, NAME, Box::new(Act(format!("{ALL}{}", p.name), What::All)));
        }
    }
    s.ext.get_mut::<Accounts>().said.retain(|k, _| !k.starts_with(ALL));
}

/// The switch `All agents use my accounts` and its one line: how it turned on by itself, or which coords differ.
/// Nothing where no coordd takes it. Public for the gateway's Accounts tab, which takes this page's place.
pub fn all_switch(s: &App, k: &Kit) -> Option<El> {
    let served = all_served(s);
    if served.is_empty() {
        return None;
    }
    let st = s.ext.get::<Accounts>();
    let states: Vec<(&str, Option<&coord_types::AllAgents>)> = served.iter().filter_map(|i| Some((s.projects.get(*i)?.name.as_str(), st.got.get(i)?.as_ref().ok()?.all.as_ref()))).collect();
    let on = |a: Option<&coord_types::AllAgents>| a.is_some_and(|a| a.on);
    let all_on = !states.is_empty() && states.iter().all(|(_, a)| on(*a));
    let mixed = states.iter().any(|(_, a)| on(*a)) && !all_on;
    let mut c = col().gap(4.0).child(Switch::new(all_on).label("All agents use my accounts").on_toggle(move |s: &mut App| set_all(s, !all_on)).build(k));
    if mixed {
        let say = |want: bool| states.iter().filter(|(_, a)| on(*a) == want).map(|(n, _)| *n).collect::<Vec<_>>().join(", ");
        c = c.child(faint(k, format!("On for {}, off for {}.", say(true), say(false))));
    } else if all_on && let Some(a) = states.iter().filter_map(|(_, a)| *a).find(|a| a.by_account.is_some()) {
        c = c.child(faint(k, format!("On since {} signed in ({})", a.by_account.as_deref().unwrap_or(""), crate::when::secs(a.at))));
    }
    // each coordd's answer, its project named when there are more than one
    let answers: Vec<(&String, &Result<String, String>)> = st.said.iter().filter(|(k, _)| k.starts_with(ALL)).collect();
    let named = answers.len() > 1;
    for (key, r) in answers {
        let project = &key[ALL.len()..];
        let say = |t: &str| if named { format!("{project}: {t}") } else { t.to_string() };
        c = match r {
            Ok(t) => c.child(faint(k, say(t))),
            Err(e) => c.child(needs_you(k, say(e))),
        };
    }
    Some(c)
}

/// Hand in the code typed for ID's sign-in; coordd answers when the sign-in ends.
fn send_code(s: &mut App, id: &str) {
    let Some(si) = s.ext.get_mut::<Accounts>().signin.get_mut(id) else { return };
    let code = si.code.text.trim().to_string();
    if code.is_empty() || si.sending {
        return;
    }
    si.sending = true;
    act(s, id, What::Code, Request::AccountCode { id: id.to_string(), code });
}

/// Project I's coordd answered: its accounts (the poll's, or his `Check limits`), or why there are none from it; or an
/// action on an account, after which the accounts are asked again at the next poll.
fn reply(s: &mut App, i: usize, data: Box<dyn std::any::Any>, r: Result<String, String>) {
    let Ok(act) = data.downcast::<Act>() else { return took(s.ext.get_mut::<Accounts>(), i, r) };
    let Act(id, what) = *act;
    // a check answered makes room for the next of the page's ([`check_all`])
    if what == What::Check {
        check_next(s);
    }
    let st = s.ext.get_mut::<Accounts>();
    match (what, r) {
        (What::Limits, r) => return took(st, i, r),
        // what it read comes with the accounts, asked of every coordd now. The windows its words end with are drawn, not said
        // again; why they were not read is said in the gateway card's words, and any other end whole; a reply with no windows
        // (not signed in, or a coordd before `ACCOUNT_RESET`) is said as before.
        (What::Check, Ok(out)) => {
            let said = match out.split_once(" · ").map(|(_, tail)| tail) {
                Some(w) if w.starts_with("5-hour ") || w.starts_with("no 5-hour limit") => None,
                Some(w) => Some(w.strip_prefix("its windows not read: ").map_or_else(|| Ok(w.to_string()), |why| Err(format!("Could not read its limits: {why}")))),
                None => Some(Ok(out.clone())),
            };
            match said {
                Some(r) => drop(st.said.insert(id.clone(), r)),
                None => drop(st.said.remove(&id)),
            }
            st.asked = None;
            for p in s.projects.iter_mut().filter(|p| served(p)) {
                p.ask(&Request::Accounts { json: true }, NAME, Box::new(Act(id.clone(), What::Limits)));
            }
            return;
        }
        // made: straight on to its sign-in, shown under the form until its card comes
        (What::New { signs }, Ok(out)) => {
            // whether the sheet still waits for this account: he may have closed it, or gone back, before the answer
            let waited = st.add.open && st.add.made.as_ref().is_some_and(|(made, _)| *made == id);
            // the id coordd made it under (another than the one sent where a removed account had it): the sign-in and the card
            // wait for that one
            let id = made_id(&out).unwrap_or(id);
            if waited && let Some((made, _)) = st.add.made.as_mut() {
                *made = id.clone();
            }
            st.asked = None;
            // a key account's key goes at once, from the account's own field, into which the text typed is moved (no
            // copy is made here); a sheet that no longer waits holds no key of this account, and its card asks for
            // it. A sign-in one goes on to its sign-in, whose link the sheet shows when it comes.
            match signs {
                None => {
                    let typed = if waited { std::mem::replace(&mut st.add.secret, TextEdit::new("")) } else { TextEdit::new("") };
                    st.keys.insert(id.clone(), typed);
                    send_key(s, &id);
                }
                Some(whose) => sign_in(s, &id, whose),
            }
            return;
        }
        (What::Key, r) => {
            let cancelled = st.setting.remove(&id).unwrap_or(false);
            if r.is_err() {
                st.keys.insert(id.clone(), TextEdit::new(""));
            }
            // cancelled: nothing to say, its empty field takes a second paste; else coordd's "account ID: key set" names the
            // id he never typed, and the header says when
            if cancelled && r.is_err() {
                st.said.remove(&id);
            } else {
                // an agy key's answer goes on with agy's check of it: models it could not list are said
                let r = r.and_then(|out| match out.split_once("agy's models were not read: ") {
                    Some((_, why)) => Err(format!("Key set; models not listed: {why}")),
                    None => Ok(KEY_SET.to_string()),
                });
                st.said.insert(id, r.map_err(|e| plain(&e)));
            }
        }
        // ended: the key's refusal follows; a cancel that failed lets that refusal be said, and is said itself only while
        // the key is still being set
        (What::KeyCancel, Ok(_)) => {}
        (What::KeyCancel, Err(e)) => {
            if let Some(cancelled) = st.setting.get_mut(&id) {
                *cancelled = false;
                st.said.insert(id, Err(plain(&e)));
            }
        }
        (What::KeyRemove, r) => {
            st.confirm_key = None;
            st.said.insert(id, r.map(|_| "Key removed.".to_string()).map_err(|e| plain(&e)));
        }
        (What::PutBack, r) => {
            let label = removed_all(&st.got).into_iter().find(|g| g.name == id).map_or(id.clone(), |g| g.label);
            st.removed = Some(match r {
                Ok(out) => put_back_line(&out, &label),
                Err(e) => plain(&e),
            });
        }
        // not made: the sheet's step says why; a sheet that no longer waits for it, the page's foot
        (What::New { .. }, Err(e)) => {
            if st.add.open && st.add.made.as_ref().is_some_and(|(made, _)| *made == id) {
                st.add.made = None;
                st.add.error = Some(plain(&e));
            } else {
                st.removed = Some(format!("Account {id} was not added: {}", plain(&e)));
            }
        }
        // the answer's first line is the URL; the rest is words for a terminal, the card says its own
        (What::Signin, Ok(out)) => {
            if let Some(si) = st.signin.get_mut(&id) {
                let mut lines = out.lines();
                si.url = lines.next().map(str::to_string);
                // opencode's instructions; Claude Code's answer goes on in words for a terminal, which the card says
                // its own way
                si.said = matches!(si.whose, Whose::Opencode(_)).then(|| lines.next()).flatten().map(str::to_string);
            }
            return;
        }
        (What::Signin, Err(e)) => {
            st.signin.remove(&id);
            st.said.insert(id, Err(plain(&e)));
        }
        (What::Code, Err(e)) => {
            if let Some(si) = st.signin.get_mut(&id) {
                si.sending = false;
            }
            st.said.insert(id, Err(plain(&e)));
        }
        (What::Code | What::Cancel, r) => {
            st.signin.remove(&id);
            st.said.insert(id, r.map_err(|e| plain(&e)));
        }
        // agents on it: it is marked and removed once they have left; its card says so (`leaving`)
        (What::Remove, Ok(out)) if out.contains("then it is removed") || out.contains("being removed already") => {
            st.confirm_remove = None;
        }
        (What::Keep, r) => {
            st.said.insert(id, r.map(|_| "Kept.".to_string()).map_err(|e| plain(&e)));
        }
        (What::Remove, Ok(out)) => {
            let label = merged(&st.got).into_iter().find(|a| a.id == id).map_or(id.clone(), |a| a.label);
            st.confirm_remove = None;
            st.removed = Some(removed_line(&out, &label));
        }
        (What::Remove, Err(e)) => {
            st.confirm_remove = None;
            let label = merged(&st.got).into_iter().find(|a| a.id == id).map_or(id.clone(), |a| a.label);
            st.said.insert(id.clone(), Err(by_label(&plain(&e), &id, &label)));
        }
        // the switch's answer without its "all agents:" lead
        (What::All, r) => {
            let lead = |t: String| t.strip_prefix("all agents: ").map(str::to_string).unwrap_or(t);
            st.said.insert(id, r.map(|o| lead(plain(&o))).map_err(|e| lead(plain(&e))));
        }
        // taken: the accounts' next answer says it is under way (`reset`), or refused
        (What::Reset, Ok(_)) => drop(st.said.remove(&id)),
        (What::Reset, Err(e)) => drop(st.said.insert(id, Err(plain(&e)))),
        // renamed, turned off or on: the card shows it from the accounts asked next; a refusal is said on the card,
        // the name field kept with what was typed
        (What::Edit, Ok(_)) => {
            st.naming.remove(&id);
            st.said.remove(&id);
        }
        (What::Edit, Err(e)) => drop(st.said.insert(id, Err(plain(&e)))),
        (What::Tier | What::Check | What::Clear, r) => {
            st.said.insert(id, r.map_err(|e| plain(&e)));
        }
    }
    st.asked = None;
}

/// Open LINK in his browser on his press of it (the deaf line's channels settings); never a sign-in's link, which he
/// copies. Not in a test: nothing of a test leaves it.
fn open_link(link: String) {
    if !cfg!(test) {
        crate::views::open(link);
    }
}

/// coordd's refusal E as the window says it: written for a terminal, it begins with the command
/// (`coord account new: `) and may end in one to type (` (coord accounts)`); both go, so no window text names a
/// `coord` command.
pub fn plain(e: &str) -> String {
    let mut t = e.trim();
    loop {
        // the engine's own name before its words is no word of his
        if let Some(rest) = t.strip_prefix("coordd: ").or_else(|| t.strip_prefix("coordd-v2: ")) {
            t = rest.trim_start();
            continue;
        }
        let Some(rest) = t.strip_prefix("coord ") else { break };
        let Some((_, after)) = rest.split_once(": ") else { break };
        t = after.trim_start();
    }
    let mut out = String::new();
    let mut rest = t;
    while let Some(at) = rest.find(" (coord ") {
        out.push_str(&rest[..at]);
        rest = rest[at..].find(')').map_or("", |end| &rest[at + end + 1..]);
    }
    out.push_str(rest);
    out
}

/// coordd's answer to a removal, OUT, as the page's one line, by the account's LABEL: that it is removed, and that its
/// harness's sign-out (Claude Code's, opencode's, agy's or t3code's) failed and why when it did (agy's, only that it
/// failed). The set-aside folder's path is left to the terminal. Put back comes with the list of removed accounts.
fn removed_line(out: &str, label: &str) -> String {
    let t = plain(out);
    let failed = ["Claude Code", "opencode", "agy", "t3code"].into_iter().find_map(|who| Some((who, t.split_once(&format!(", but {who}'s sign-out failed: "))?.1)));
    match failed {
        // agy-rpc's words and its exit are for coordd's log (consult-lead-c N-1956 p8)
        Some(("agy", _)) => format!("{label} removed; agy's sign-out failed"),
        Some((_, why)) => format!("{label} removed; sign-out failed: {}", why.split(". Its folder").next().unwrap_or(why)),
        None => format!("{label} removed"),
    }
}

fn parse(r: Result<String, String>) -> Result<NativeAccounts, String> {
    serde_json::from_str(&r?).map_err(|_| "its engine is from before the accounts".to_string())
}

/// Project I's accounts answer R, the poll's or his `Check limits`: kept for the cards.
fn took(st: &mut Accounts, i: usize, r: Result<String, String>) {
    let got = parse(r);
    // a sign-in that ended by the browser's redirect: the account now says signed in, and its card says so
    if let Ok(n) = &got {
        st.signin.retain(|id, si| si.sending || !n.accounts.iter().any(|a| a.id == *id && a.signed_in));
    }
    st.got.insert(i, got);
    st.came += 1;
    st.asked = Some(Instant::now());
}

/// A test's accounts answer R of project I, taken as the poll takes one (its state made first: a test binary
/// registers no extension).
#[cfg(test)]
pub(crate) fn answered(s: &mut App, i: usize, r: Result<String, String>) {
    if s.ext.try_get::<Accounts>().is_none() {
        s.ext.insert(Accounts::default());
    }
    took(s.ext.get_mut::<Accounts>(), i, r);
}

/// The ids an account made now may not have: the accounts' and the removed ones' (coordd never gives an id twice; the
/// window picks the same `work-2` coordd would, so its sign-in goes to that one).
fn taken<'a>(accounts: &'a [NativeAccount], removed: &'a [RemovedAccount]) -> Vec<&'a str> {
    accounts.iter().map(|a| a.id.as_str()).chain(removed.iter().map(|r| r.id.as_str())).collect()
}

/// The removed accounts as every coordd's answer lists them, each once (by its set-aside folder), newest first.
fn removed_all(got: &HashMap<usize, Result<NativeAccounts, String>>) -> Vec<RemovedAccount> {
    let mut out: Vec<RemovedAccount> = Vec::new();
    for r in got.values().filter_map(|r| r.as_ref().ok()).flat_map(|n| &n.removed) {
        match out.iter_mut().find(|o| o.harness == r.harness && o.name == r.name) {
            // Seen by two coordds: not signed out if either says so, so the line never looks finished too soon.
            Some(o) => o.not_signed_out |= r.not_signed_out,
            None => out.push(r.clone()),
        }
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| a.name.cmp(&b.name)));
    out
}

/// What he can do about a removed account Claude Code has not signed out, on the pointer over its line: Put back and
/// Remove again (no retry of ours is built).
const NOT_SIGNED_OUT_HELP: &str = "Still signed in; Put back, then Remove";

fn removed_help(r: &RemovedAccount) -> Option<&'static str> {
    r.not_signed_out.then_some(NOT_SIGNED_OUT_HELP)
}

/// A removed account's line: its label, when, and that its usage is kept; "not signed out" in its place while Claude
/// Code has not signed it out, so the line does not look finished.
fn removed_row(r: &RemovedAccount) -> String {
    let tail = if r.not_signed_out { "not signed out" } else { "its usage is kept" };
    match r.at {
        0 => format!("{} · {tail}", r.label),
        at => format!("{} · removed {} · {tail}", r.label, crate::when::secs(at)),
    }
}

/// Whether no coordd answered with the gateway: yaaihh's own calls then have no way (they stay on Codex, so a key
/// account gives them none).
fn no_own_calls(got: &HashMap<usize, Result<NativeAccounts, String>>) -> bool {
    let answers: Vec<&NativeAccounts> = got.values().filter_map(|r| r.as_ref().ok()).collect();
    !answers.is_empty() && !answers.iter().any(|n| n.gateway)
}

/// Where A's agents go when it is removed, as the coordd asked judges it: None with no agent on it;
/// Some(Ok("to Work" | "to another account" | "to the gateway")); Some(Err(())) with nowhere to go. Only an account of
/// A's own harness takes them.
fn moves_to(a: &NativeAccount, accounts: &[NativeAccount], gateway: bool, now: i64) -> Option<Result<String, ()>> {
    if a.agents.is_empty() {
        return None;
    }
    let others: Vec<&NativeAccount> = accounts.iter().filter(|o| o.id != a.id && o.harness == a.harness && o.signed_in && o.leaving.is_none() && o.out_until.is_none_or(|t| t <= now)).collect();
    Some(match others.as_slice() {
        [one] => Ok(format!("to {}", one.label)),
        [_, ..] => Ok("to another account".into()),
        [] if gateway => Ok("to the gateway".into()),
        [] => Err(()),
    })
}

/// A's agents as one phrase, each as the Agents row names it: `research.orch`, `research.orch and research.w2`,
/// `research.orch and 2 others`.
fn who(a: &NativeAccount) -> String {
    let full: Vec<String> = a.agents.iter().map(|x| format!("{}.{}", x.coord, x.agent)).collect();
    let names: Vec<&str> = full.iter().map(String::as_str).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [one, two] => format!("{one} and {two}"),
        [one, rest @ ..] => format!("{one} and {} others", rest.len()),
    }
}

/// Whether a sign-in moves agents now: every coordd answering has never had the switch set (`NativeAccounts.all`
/// None), so it turns on by itself at the first sign-in.
fn moves_on_sign_in(got: &HashMap<usize, Result<NativeAccounts, String>>) -> bool {
    let answers: Vec<&NativeAccounts> = got.values().filter_map(|r| r.as_ref().ok()).collect();
    !answers.is_empty() && answers.iter().all(|n| n.all.is_none())
}

/// coordd's line T with its lead `account ID: ` said by the account's LABEL, the name he gave it.
fn by_label(t: &str, id: &str, label: &str) -> String {
    match t.strip_prefix(&format!("account {id}: ")) {
        Some(rest) => format!("{label}: {rest}"),
        None => t.to_string(),
    }
}

/// The id in coordd's answer to `AccountNew`, `account ID: made …`.
fn made_id(out: &str) -> Option<String> {
    let (id, _) = out.trim().strip_prefix("account ")?.split_once(": made")?;
    (!id.is_empty() && !id.contains(char::is_whitespace)).then(|| id.to_string())
}

/// coordd's answer OUT to `Put back` as the page says it, by the account's LABEL: what he does next, if anything.
fn put_back_line(out: &str, label: &str) -> String {
    if out.contains("not signed in") {
        format!("{label} put back; sign in again")
    } else {
        format!("{label} put back.")
    }
}

/// The accounts of all answers, each once: the first answer's account with every coord's agents, and the usage of
/// every coord added up per model (a coord answered twice, by two projects naming it, counted once).
fn merged(got: &HashMap<usize, Result<NativeAccounts, String>>) -> Vec<NativeAccount> {
    let mut answers: Vec<(&usize, &NativeAccounts)> = got.iter().filter_map(|(i, r)| Some((i, r.as_ref().ok()?))).collect();
    answers.sort_by_key(|(i, _)| **i);
    let mut coords = HashSet::new();
    let mut out: Vec<NativeAccount> = Vec::new();
    for (_, a) in answers.into_iter().filter(|(_, a)| coords.insert(a.coord.clone())) {
        for acc in &a.accounts {
            let Some(m) = out.iter_mut().find(|m| m.harness == acc.harness && m.id == acc.id) else {
                out.push(acc.clone());
                continue;
            };
            m.signin_running |= acc.signin_running;
            m.leaving = m.leaving.or(acc.leaving);
            m.locked_by = m.locked_by.clone().or(acc.locked_by.clone());
            m.put_back = m.put_back.or(acc.put_back);
            for ag in &acc.agents {
                match m.agents.iter_mut().find(|x| x.coord == ag.coord && x.agent == ag.agent) {
                    Some(x) => x.note = x.note.take().or(ag.note.clone()),
                    None => m.agents.push(ag.clone()),
                }
            }
            for u in &acc.usage {
                match m.usage.iter_mut().find(|x| x.model == u.model) {
                    Some(x) => {
                        x.requests += u.requests;
                        x.failed += u.failed;
                        x.tokens += u.tokens;
                        x.cache_write_1h += u.cache_write_1h;
                        x.duration_ms += u.duration_ms;
                        x.timed += u.timed;
                    }
                    None => m.usage.push(u.clone()),
                }
            }
        }
    }
    out
}

/// Now, in seconds since the epoch.
fn now_s() -> i64 {
    chrono::Utc::now().timestamp()
}

/// How far off AT (seconds) is from NOW, as the gateway's cards say it ("in 2 h 18 m", [`crate::when::span`]).
fn countdown(at: i64, now: i64) -> String {
    if at <= now { "reset time reached".into() } else { format!("in {}", crate::when::span(at - now)) }
}

fn faint(k: &Kit, t: impl Into<String>) -> El {
    text(t.into()).font(k.ty.small).color(k.c.text_muted).wrap()
}

/// A line that needs him: the card's one accent colour, which means this needs you (an account out until a
/// known time needs nothing and is not drawn in it).
pub fn needs_you(k: &Kit, t: impl Into<String>) -> El {
    text(t.into()).font(k.ty.small_medium).color(k.c.warning.fg).wrap()
}

/// An action on the card, shown only while the pointer is on the card.
fn on_hover(e: El) -> El {
    e.group_opacity(0.0, 1.0)
}

/// The three dots of a card's menu.
static MORE: rune::Icon = rune::Icon { name: "accounts-more", view: 24.0, body: r#"<circle cx="5" cy="12" r="1.8" fill="currentColor"/><circle cx="12" cy="12" r="1.8" fill="currentColor"/><circle cx="19" cy="12" r="1.8" fill="currentColor"/>"# };

/// One entry of a card's `⋯` menu ([`card_menu`]): choosing it closes the menu, then does F.
pub fn quiet(label: impl Into<String>, f: impl Fn(&mut App) + 'static) -> MenuItem {
    MenuItem::new(label).on_select(move |s: &mut App| {
        s.ext.get_mut::<Accounts>().menu = None;
        f(s)
    })
}

/// [`quiet`] for an entry that needs the frame's `Tx` (a copy to the clipboard).
pub fn quiet_tx(label: impl Into<String>, f: impl Fn(&mut App, &mut Tx) + 'static) -> MenuItem {
    MenuItem::new(label).on_select_tx(move |s: &mut App, tx: &mut Tx| {
        s.ext.get_mut::<Accounts>().menu = None;
        f(s, tx)
    })
}

/// The `⋯` at a card's top right, and behind it the card's quiet actions as a small menu: at rest a card shows what
/// it says and the one thing to do. One for every kind of card on the page, the gateway's too. KEY names the card, one
/// menu open at a time; None where the card has no such action, so no `⋯` opens an empty menu.
pub fn card_menu(s: &App, k: &Kit, key: &str, items: Vec<MenuItem>, layer: &mut Vec<El>) -> Option<El> {
    if items.is_empty() {
        return None;
    }
    let open = s.ext.try_get::<Accounts>().is_some_and(|a| a.menu.as_deref() == Some(key));
    let mine = key.to_string();
    let trigger = IconButton::new(&MORE)
        .variant(IconButtonVariant::GhostMuted)
        .label("More actions")
        .help("More actions")
        .on_press(move |s: &mut App| {
            let menu = &mut s.ext.get_mut::<Accounts>().menu;
            *menu = (menu.as_deref() != Some(mine.as_str())).then(|| mine.clone());
        })
        .build(k);
    let mine = key.to_string();
    let mut menu = Menu::new(format!("card-menu-{key}")).trigger(trigger).open(open).placement(rune_kit::Placement::BottomEnd).on_close(move |s: &mut App| {
        let menu = &mut s.ext.get_mut::<Accounts>().menu;
        if menu.as_deref() == Some(mine.as_str()) {
            *menu = None;
        }
    });
    for i in items {
        menu = menu.item(i);
    }
    Some(menu.build(k, layer))
}

/// A limit window's line: what is left of it, when it starts again, and its meter. NAME is the row's words and LABEL
/// its meter's name: the same, but for a window drawn under its limit's name ([`agy_groups`]: "5-hour" under
/// "Gemini", its meter "Gemini · 5-hour"). SPAN, the window's length when known, puts the even pace's mark on a
/// running window's meter ([`pace`]); a window not begun or past its reset has none.
fn window(k: &Kit, (name, label): (&str, &str), w: &LimitWindow, span: Option<Span>, (at, from): (i64, &str), now: i64) -> El {
    let left = (100.0 - w.used_percent).clamp(0.0, 100.0);
    // not used yet, so not begun: no time it starts again (an Antigravity window, as t3code reads it)
    if w.resets_at == coord_types::NOT_STARTED {
        let seen = format!("not started · seen {} from {from}", crate::when::secs(at));
        return limit_line(k, name, label, &format!("{}% remaining", left.round() as i64), "not started", None, Some(left)).help(seen);
    }
    let seen = format!("starts again {} · seen {} from {from}", crate::when::secs(w.resets_at), crate::when::secs(at));
    // past its reset with nothing read since: what was used before is no share of the new window, and the new one's use
    // is not known
    if w.resets_at <= now {
        return limit_line(k, name, label, "reset, not seen since", &format!("at {}", crate::when::secs(w.resets_at)), None, None).help(seen);
    }
    let due = span.and_then(|s| pace(w, s, now));
    let seen = match due {
        Some(d) => format!("{seen} · on pace: {}% left", d.round() as i64),
        None => seen,
    };
    paced_limit_line(k, name, label, &format!("{}% remaining", left.round() as i64), &countdown(w.resets_at, now), None, Some(left), due).help(seen)
}

/// How long a limit window runs, for where its use would stand by now at an even pace ([`pace`]): a fixed number of
/// seconds, or a calendar month back from its reset (Go's monthly, which starts again on a date).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Span {
    Secs(i64),
    Month,
}

impl Span {
    /// A week: Claude Code's weekly windows, of all models and of one.
    const WEEK: Span = Span::Secs(7 * 86_400);

    /// A window's length from its own words, as the card names its own windows ("5-hour", "Weekly", "Monthly") and
    /// t3code names an agy window's after its limit ("Gemini · 5-hour"); None for words that say no length, and that
    /// window then has no mark.
    fn named(words: &str) -> Option<Span> {
        match words {
            "Weekly" => Some(Span::WEEK),
            "Monthly" => Some(Span::Month),
            w => w.strip_suffix("-hour").and_then(|h| h.parse::<i64>().ok()).filter(|h| *h > 0).map(|h| Span::Secs(h * 3600)),
        }
    }

    /// When a window of this length that starts again at RESETS began.
    fn began(self, resets: i64) -> Option<i64> {
        match self {
            Span::Secs(s) => Some(resets - s),
            Span::Month => chrono::DateTime::from_timestamp(resets, 0)?.checked_sub_months(chrono::Months::new(1)).map(|t| t.timestamp()),
        }
    }
}

/// What would remain of window W (0 to 100) at NOW had it been used evenly over its SPAN: 100 as it begins, 0 at its
/// reset. Its meter shows it as a red mark: what is left short of it is ahead of the pace and runs out before the
/// reset.
fn pace(w: &LimitWindow, span: Span, now: i64) -> Option<f64> {
    let len = (w.resets_at - span.began(w.resets_at)?) as f64;
    (len > 0.0).then(|| (100.0 * (w.resets_at - now) as f64 / len).clamp(0.0, 100.0))
}

/// How long a status line keeps an account's windows live (seconds): a session of ours said them that recently, so
/// the card shows them with no time. Past it, or from another source, the card says when they were read ("as of
/// 18:40") with `Check limits` beside, the one refresh.
const LIVE: i64 = 600;

/// When the limits a card shows were read ("as of 18:40"), AT in seconds, with the refresh PRESS beside: the line
/// under a reading that is not live, on both cards (the gateway's `quota_card` and [`native_card`]).
pub fn as_of(k: &Kit, at: i64, press: Option<El>) -> El {
    // the words take the row's slack, so a narrow card keeps them on one line beside the press
    row().w_full().gap(8.0).center_y().child(faint(k, format!("as of {}", crate::when::secs(at))).grow(1.0).shrink().w(0.0)).children(press)
}

/// Whether account A's windows come live from a session's status line at NOW ([`LIVE`]).
fn live(a: &NativeAccount, now: i64) -> bool {
    a.limits_from == "statusline" && now - a.limits_seen < LIVE
}

/// The colour of a remaining share RATE (0 to 1) on a limit's meter, the same on every card (the gateway's
/// `requestRateColor`): low → medium → high meter colours.
pub fn meter(k: &Kit, rate: f32) -> rune::Color {
    let r = if rate.is_finite() { rate.clamp(0.0, 1.0) } else { 0.0 };
    if r < 0.5 { k.c.danger.icon.mix(k.c.warning.icon, r * 2.0) } else { k.c.warning.icon.mix(k.c.success.icon, (r - 0.5) * 2.0) }
}

/// One limit window's line, the one drawing of both cards (the gateway's `quota_card` and [`native_card`]), with no
/// pace mark: [`paced_limit_line`] with no DUE. The gateway's quota page calls this form.
pub fn limit_line(k: &Kit, name: &str, label: &str, status: &str, when: &str, when_help: Option<String>, left: Option<f64>) -> El {
    paced_limit_line(k, name, label, status, when, when_help, left, None)
}

/// One limit window's line, the one drawing of both cards: NAME at the left; STATUS ("38% remaining") and WHEN (its
/// countdown, "in 2 h 18 m"; empty: none) at the right, WHEN_HELP the time it starts again on WHEN's hover; under them,
/// when LEFT (0 to 100) is known, its meter coloured by what is left ([`meter`]), named LABEL, with a red mark across it
/// at DUE, what would remain by now at an even pace ([`pace`]); no mark when DUE is None.
pub fn paced_limit_line(k: &Kit, name: &str, label: &str, status: &str, when: &str, when_help: Option<String>, left: Option<f64>, due: Option<f64>) -> El {
    let mut right = row().gap(8.0).center_y().child(text(status.to_string()).font(k.ty.small_medium).color(k.c.text_base));
    if !when.is_empty() {
        let w = text(when.to_string()).font(k.ty.small).color(k.c.text_muted);
        right = right.child(match when_help {
            Some(h) => w.help(h),
            None => w,
        });
    }
    let mut line = col().w_full().gap(4.0).child(row().w_full().gap(8.0).center_y().justify_between().child(text(name.to_string()).font(k.ty.small).color(k.c.text_base).truncate()).child(right));
    if let Some(p) = left {
        let frac = (p.clamp(0.0, 100.0) / 100.0) as f32;
        let mut bar = row().w_full().h(6.0).radius(3.0).fill(k.c.layer_02).child(el().h(6.0).radius(3.0).w_frac(frac).fill(meter(k, frac))).label(label.to_string());
        // the mark over the bar, standing out of it above and below, at DUE's share of its width
        if let Some(d) = due {
            let at = (d.clamp(0.0, 100.0) / 100.0) as f32;
            let mark = el().w(2.0).h_full().fill(k.c.danger.icon).label(format!("{label} at an even pace: {}% by now", d.round() as i64));
            bar = bar.child(row().pin(Some(0.0), Some(-2.0), Some(0.0), Some(-2.0)).child(el().w_frac(at)).child(mark));
        }
        line = line.child(bar);
    }
    line
}

/// A "label value" line of a card's details (a reset held, a grant, the credit balance), on both cards.
pub fn detail(k: &Kit, label: impl Into<String>, value: impl Into<String>) -> El {
    // the label's column: 180 on a wide page, and a third of a narrow card's box, so a value such as
    // "2 left · use by 27 Nov" keeps one line there
    let label = text(label.into()).font(k.ty.small).color(k.c.text_muted).w_frac(0.35).max_w(180.0);
    row().w_full().gap(12.0).child(label).child(text(value.into()).font(k.ty.small).color(k.c.text_base).wrap_anywhere().grow(1.0))
}

/// The box a card's [`detail`] lines stand in.
pub fn details(k: &Kit) -> El {
    col().w_full().gap(4.0).padding(8.0).radius(k.radius.sm).fill(k.c.layer_02)
}

/// Built-in account A's resets, below its windows as on the gateway's card: each kind Claude Code holds with how many
/// are left, by when, what it gives back and why it cannot be used at this limit, in its words; `Use Fable reset`,
/// `Use full reset`, one per kind offered at a limit, each asked once before it is sent, dimmed with Claude Code's
/// reason when it gave one; `applying…` while a press is under way; Claude Code's words once applied, or its refusal,
/// that kind's `Use` gone; a press not known to be used says so and offers `Try … again`. None when it offered none,
/// and from a coordd before `ACCOUNT_RESET`.
fn reset_row(s: &App, k: &Kit, a: &NativeAccount) -> Option<El> {
    if !s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_RESET) {
        return None;
    }
    let st = s.ext.get::<Accounts>();
    let r = resets(a, asking(s, &a.id, What::Reset));
    // what Claude Code says it holds, at any time (E5's `held`, from its notice or its `/limit-reset` list on his
    // `Check limits`), as the gateway's card lists its grants; from a coordd before it, the offers' own counts
    let listed: Vec<&ResetOffer> = if a.held.is_empty() { r.usable.clone() } else { a.held.iter().collect() };
    if r.is_empty() && listed.is_empty() {
        return None;
    }
    let small = |label: &str| Button::new(label).control_size(ControlSize::Regular);
    let mut out = col().w_full().gap(8.0);
    // each kind with how many are left and by when, what it gives back and why it cannot be used now (an offer at a
    // limit with none listed says none of these), and when Claude Code listed them: a list read at rest is no press of
    // `Use` (only `resets` take one); that `Use` comes at a limit
    let lines: Vec<El> = listed.iter().filter_map(|o| Some(detail(k, &o.kind, grant(o)?))).collect();
    if !lines.is_empty() {
        let when = a.held_seen.filter(|_| !a.held.is_empty()).map(|t| faint(k, format!("as of {}", crate::when::secs(t))));
        let at_rest = (r.usable.is_empty() && !r.applying && !a.held.is_empty()).then(|| faint(k, "Offered at a limit"));
        out = out.child(details(k).children(lines).children(when).children(at_rest));
    }
    match st.confirm_reset.as_ref().filter(|(id, _)| *id == a.id) {
        _ if r.applying => out = out.child(small("applying…").disabled(true).build(k).w_full()),
        Some((_, kind)) => {
            let (i, kd) = (a.id.clone(), kind.clone());
            out = out.child(
                col()
                    .w_full()
                    .gap(8.0)
                    .padding(10.0)
                    .radius(k.radius.sm)
                    .fill(k.c.layer_02)
                    .child(text(format!("Use the {kind} for “{}”?", a.label)).font(k.ty.small_medium).color(k.c.text_base).wrap())
                    .child(faint(k, "One use; it does not come back."))
                    .child(
                        row()
                            .gap(6.0)
                            .child(
                                small(format!("Use {kind}").as_str())
                                    .variant(ButtonVariant::Contrast)
                                    .on_press(move |s: &mut App| {
                                        s.ext.get_mut::<Accounts>().confirm_reset = None;
                                        act(s, &i, What::Reset, Request::AccountReset { id: i.clone(), kind: kd.clone() });
                                    })
                                    .build(k),
                            )
                            .child(small("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.ext.get_mut::<Accounts>().confirm_reset = None).build(k)),
                    ),
            );
        }
        None => {
            for o in &r.usable {
                let (i, kind) = (a.id.clone(), o.kind.clone());
                // the last press of this kind not known to be used: the same press again
                let label = match r.refused {
                    Some((tried, _)) if r.again && tried == o.kind => format!("Try {} again", o.kind),
                    _ => format!("Use {}", o.kind),
                };
                let button = small(&label).help(o.why.clone().or_else(|| held(o)).unwrap_or_else(|| format!("Claude Code's {}", o.kind)));
                out = out.child(match o.why {
                    Some(_) => button.disabled(true).build(k).w_full(),
                    None => button.on_press(move |s: &mut App| s.ext.get_mut::<Accounts>().confirm_reset = Some((i.clone(), kind.clone()))).build(k).w_full(),
                });
            }
        }
    }
    if let Some(words) = r.done {
        out = out.child(faint(k, words.to_string()));
    }
    match r.refused {
        Some((kind, words)) if r.again => out = out.child(faint(k, format!("{kind}: {words}"))),
        Some((kind, words)) => out = out.child(faint(k, format!("{kind} refused: {words}"))),
        None => {}
    }
    Some(out)
}

/// Offer O's grant line as the gateway's card words one ("Fable reset ×2 — gives back … · expires …", "blocked: …"),
/// in Claude Code's words: how many are left and by when ([`held`]), what it gives back, and why it cannot be used at
/// this limit, each part after a dash; None when it said none of these.
fn grant(o: &ResetOffer) -> Option<String> {
    let parts: Vec<String> = [held(o), o.gives.clone(), o.why.clone()].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" — "))
}

/// How many of offer O are left and by when, as Claude Code said them ("2 left · use by 27 Nov"); None when it said
/// neither.
fn held(o: &ResetOffer) -> Option<String> {
    // each part kept on one line (no break inside "27 Nov"); the line breaks between parts only
    let said: Vec<String> = [o.left.map(|n| format!("{n} left")), o.use_by.map(|t| format!("use by {}", crate::when::day(t)))].into_iter().flatten().map(|p| p.replace(' ', "\u{a0}")).collect();
    (!said.is_empty()).then(|| said.join(" · "))
}

/// Usage U's tokens as a [`Spend`] without a price.
fn tokens(u: &AccountUsage) -> Spend {
    let t = u.tokens;
    Spend { input: t.input as f64, cache_read: t.cache_read as f64, cache_write: t.cache_write as f64, output: t.output as f64, reasoning: t.reasoning as f64, cost: None }
}

/// What `out_until` is when an account was found out with no time it comes back (coordd's `limits::UNKNOWN_RESET`).
const UNKNOWN_RESET: i64 = i64::MAX;

/// One account's card (the gateway's quota card in form): its label, what needs him (not signed in, a login
/// about to run out, out with no known return), its windows, its agents, and this month's requests, per model behind
/// "details". One quiet colour for the bars and one accent for what needs him; each thing said once; `Sign in` in
/// sight where it is the thing to do, the quiet actions behind the `⋯` at its top right ([`card_menu`], its overlay
/// into LAYER); the agents' notes behind a hover. HANDLE: the gateway tab's drag handle, first in the header.
pub fn native_card(s: &App, k: &Kit, a: &NativeAccount, month: &str, now: i64, handle: Option<El>, layer: &mut Vec<El>) -> El {
    let st = s.ext.get::<Accounts>();
    let open = st.open.contains(&a.id);
    let mut header = row().w_full().gap(8.0).center_y();
    if let Some(h) = handle {
        header = header.child(h);
    }
    // its provider by its logo (opencode's mark in the text's colour), then its name, which keeps the header's width
    // but for `Sign in` and the `⋯` (a name was cut to "…" by the room three hidden buttons kept in the header: W6, a
    // card not signed in or on a key)
    let (oc, agy) = (is_oc(a), is_agy(a));
    header = header.child(match () {
        _ if oc => icon(&logos::OPENCODE, 18.0).color(k.c.text_base),
        _ if agy => icon(&logos::ANTIGRAVITY, 18.0),
        _ => icon(&logos::CLAUDE, 18.0),
    });
    header = header.child(text(a.label.clone()).font(k.ty.ui_medium).color(k.c.text_base).truncate().help(format!("{} ({})", a.label, a.id)).grow(1.0));
    let small = |label: &str| Button::new(label).control_size(ControlSize::Regular);
    let id = a.id.clone();
    // while the sheet is on this account's step (it was just made there), its sign-in and its key field are the
    // sheet's: the card does not show them a second time under it
    let in_sheet = st.add.open && st.add.made.as_ref().is_some_and(|(made, _)| *made == a.id);
    // an opencode account takes its key in the same masked field (but one whose provider takes none): coordd hands it
    // to opencode's own sign-in
    let oc_key = oc && oc_key_from(a).is_some();
    let field = st.keys.get(&a.id).filter(|_| (a.key || oc_key) && !in_sheet);
    let setting = st.setting.get(&a.id).copied().filter(|_| oc && !in_sheet);
    let signing = st.signin.get(&a.id).filter(|_| !a.key && !in_sheet);
    // what kind of account it is, said once: a subscription Claude Code signed in to, with its plan as Claude Code
    // names it ("Plan Max", as the gateway's card says its logins'), or an API key and since when; an agy account's
    // Google sign-in, naming the program that signed it in when coordd says which (`agy_way`), or its Gemini API key
    let kind = match (a.key, a.key_since) {
        _ if oc => format!("opencode · {}", oc_provider(a).map_or(a.provider.as_deref().unwrap_or("no provider"), |p| p.name)),
        (true, Some(t)) if agy => format!("Antigravity · Gemini API key · key set {}", crate::when::day(t)),
        (true, None) if agy => "Antigravity · Gemini API key".to_string(),
        (false, _) if agy => match agy_way(a) {
            Some(AgyWay::Own) => format!("Antigravity · {}, agy's own sign-in", agy_sign_in(a.auth_method.as_deref())),
            Some(AgyWay::T3code) => format!("Antigravity · {}, through t3code", agy_sign_in(a.auth_method.as_deref())),
            None => format!("Antigravity · {}", agy_sign_in(a.auth_method.as_deref())),
        },
        (true, Some(t)) => format!("API key · key set {}", crate::when::day(t)),
        (true, None) => "API key".to_string(),
        (false, _) => {
            let mut plan = a.plan.as_deref().unwrap_or_default().chars();
            match plan.next() {
                Some(first) => format!("subscription · Plan {}", first.to_uppercase().chain(plan).collect::<String>()),
                None => "subscription".to_string(),
            }
        }
    };
    // the quiet actions, behind the card's `⋯`: hidden until the pointer came, a row of them would keep its height and
    // leave an empty band
    let mut more: Vec<MenuItem> = Vec::new();
    // a login past its end needs a new sign-in though coordd's last ask still says signed in: Claude Code does not
    // renew it ("Using Claude Code doesn't extend the login", its authentication page)
    let expired = login_ran_out(a, now);
    // two turns refused: Claude Code's login there no longer works, though its last ask said signed in (0.2 D22 D)
    let refused = login_refused(a, now);
    // out until a time (a limit met), or with none known; a key account's own outs are said by `key_out_lines`
    let out = a.out_until.filter(|t| *t > now && !refused && !(keyed(a) && (a.out_for.as_deref() != Some("rate_limit") || a.again_at.is_some())));
    // where the windows are drawn: Claude's 5-hour and weekly; OpenCode Go's 5-hour, weekly and monthly, from Go's
    // usage page; an agy sign-in's as t3code read them, its weeks of some models alone after them (another opencode
    // provider's limits, and an agy key's, are not seen)
    let claude_windows = a.signed_in && !a.key && !oc && !agy;
    let go_windows = go_windows(s, a);
    let agy_windows = agy && a.signed_in && !a.key;
    let drawn = claude_windows || go_windows || agy_windows;
    // the check, by what it is for here (one request, [`check`]): `Check limits` for a signed-in built-in account whose
    // coordd's check reads the windows, first as on the gateway's card (proxy quota_page.rs `quota_card`)
    let checks = checks(s, a, now);
    if checks {
        let i = id.clone();
        more.push(quiet("Check limits", move |s: &mut App| check(s, &i)).disabled(querying(s, &a.id)));
    }
    // his own settings, from a coordd that takes them: a name of his, and off · on as the gateway card's
    let naming = st.naming.get(&a.id).filter(|_| !in_sheet);
    if edits(s) && a.leaving.is_none() {
        if naming.is_none() {
            let (i, label) = (id.clone(), a.label.clone());
            more.push(quiet("Rename", move |s: &mut App| drop(s.ext.get_mut::<Accounts>().naming.insert(i.clone(), TextEdit::new(&label)))));
        }
        let (i, off) = (id.clone(), a.off.is_none());
        more.push(quiet(if off { "Turn off" } else { "Turn on" }, move |s: &mut App| edit(s, &i, None, Some(off))).disabled(asking(s, &a.id, What::Edit)));
    }
    // out until a known time, or an agy account out for some of its models: he may say it is back before then, as the
    // gateway card's `Clear cooldown` (one with no time known has it in sight, below)
    let models_out: Vec<&ModelOut> = a.models_out.iter().filter(|o| o.until > now).collect();
    if out.is_some_and(|t| t != UNKNOWN_RESET) || out.is_none() && !models_out.is_empty() {
        let i = id.clone();
        more.push(quiet("It can take requests", move |s: &mut App| clear(s, &i)).disabled(asking(s, &a.id, What::Clear)));
    }
    // a key set, or an opencode account signed in (by a key or a login: a key typed goes to opencode in its place)
    if ((a.key && a.key_since.is_some()) || (oc_key && a.signed_in)) && field.is_none() && setting.is_none() {
        let i = id.clone();
        more.push(quiet("Replace key", move |s: &mut App| drop(s.ext.get_mut::<Accounts>().keys.insert(i.clone(), TextEdit::new("")))));
    }
    // and where Claude Code refused a reset for this login: it must sign in again first
    let relogin = a.reset.as_ref().is_some_and(|r| r.login);
    if !a.key && (!a.signed_in || expired || refused || relogin) && signing.is_none() && !in_sheet {
        let i = id.clone();
        more.push(quiet("Check whether it is signed in", move |s: &mut App| check(s, &i)));
        // Sign in stays in sight: it is the one thing he must do, and after a window swap it brings back a sign-in that
        // still runs (coordd answers its URL again)
        let i = id.clone();
        let button = small(if a.signin_running { "Open sign-in" } else { "Sign in" }).variant(ButtonVariant::Contrast);
        if agy {
            header = header.child(button.help("Sign in with Google").on_press(move |s: &mut App| sign_in(s, &i, Whose::Agy)).build(k));
        } else if !oc {
            header = header.child(button.help("Sign in with Claude Code").on_press(move |s: &mut App| sign_in(s, &i, Whose::Claude)).build(k));
        } else if let Some((way, words)) = oc_provider(a).and_then(|p| p.way) {
            // opencode's browser way, where its provider has one; its key goes in the field under the header
            header = header.child(button.help(format!("{words}: opencode's own sign-in")).on_press(move |s: &mut App| sign_in(s, &i, Whose::Opencode(way))).build(k));
        }
    }
    // Remove, from the protocol that serves it
    let removable = s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_REMOVE);
    let confirming = st.confirm_remove.as_deref() == Some(a.id.as_str());
    if a.signed_in && !a.key {
        header = header.help(format!("Signed in (asked {})", crate::when::secs(a.signed_in_seen)));
    }
    // Remove key, from the protocol that deletes the file in coordd, asked once
    let confirming_key = st.confirm_key.as_deref() == Some(a.id.as_str());
    if a.key && a.key_since.is_some() && removable && field.is_none() && !confirming_key {
        let i = id.clone();
        more.push(quiet("Remove key", move |s: &mut App| s.ext.get_mut::<Accounts>().confirm_key = Some(i.clone())));
    }
    // Remove last; each of the two asks once on the card before anything is sent
    if removable && !confirming && a.leaving.is_none() {
        let i = id.clone();
        more.push(quiet("Remove", move |s: &mut App| s.ext.get_mut::<Accounts>().confirm_remove = Some(i.clone())));
    }
    header = header.children(card_menu(s, k, &format!("{}-{}", a.harness, a.id), more, layer));
    let kind = text(kind).font(k.ty.small).color(k.c.text_muted).truncate();
    let kind = match agy_way(a) {
        Some(AgyWay::T3code) => kind.help("Plan unknown; limits shown below"),
        Some(AgyWay::Own) => kind.help("Plan and limits unknown"),
        None => kind,
    };
    let mut card = col().w(340.0).gap(10.0).padding(14.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).hover_group().child(col().w_full().gap(2.0).child(header).child(kind));
    // an agy account: the models agy offers on it, named on the hover; on a sign-in t3code made, that t3code, in
    // Supervised, approves its agents' file reads itself, with nothing of coordd's `pre` seeing them; on agy's own,
    // that agy asks before every tool call
    if agy && !a.models.is_empty() {
        let names: Vec<&str> = a.models.iter().map(|m| m.strip_prefix("agy/").unwrap_or(m)).collect();
        card = card.child(faint(k, format!("{} model{}", names.len(), if names.len() == 1 { "" } else { "s" })).help(names.join("\n")));
    }
    match agy_way(a) {
        Some(AgyWay::T3code) => card = card.child(faint(k, AGY_T3CODE_TERMS).help("Agents run inside t3code")),
        Some(AgyWay::Own) => card = card.child(faint(k, AGY_OWN_TERMS).help("Yolo agents run tools without asking")),
        None => {}
    }
    if let Some(f) = naming {
        card = card.child(name_field(k, &a.id, f, asking(s, &a.id, What::Edit)));
    }
    // turned off by him: said once, with the one action that turns it on, as the gateway card says a login turned off
    if a.off.is_some() {
        let i = id.clone();
        let words = if oc || agy { "Off · takes no new agents" } else { "Off · takes no agents" };
        let on = small("Turn on").variant(ButtonVariant::Ghost).disabled(asking(s, &a.id, What::Edit)).on_press(move |s: &mut App| edit(s, &i, None, Some(false))).build(k);
        card = card.child(row().w_full().gap(8.0).center_y().justify_between().child(faint(k, words).grow(1.0).shrink().w(0.0)).children(edits(s).then_some(on)));
    }
    // its key refused, a key account's or an opencode account's on a key; an agy key that agy did not take at its check
    // (coordd's `record_key`: set, and not signed in)
    let agy_refused = agy && a.key && a.key_since.is_some() && !a.signed_in;
    let key_refused = key_out(a, now) == Some("authentication_failed") || agy_refused;
    match signing {
        // an account on an API key: "No key" and the field, or its key set
        _ if a.key => {
            if a.key_since.is_none() {
                card = card.child(needs_you(k, "No key"));
            }
            if agy_refused {
                card = card.child(needs_you(k, "Key refused: agy did not take it"));
            }
            if confirming_key {
                let (i, j) = (id.clone(), id.clone());
                card = card.child(
                    col()
                        .w_full()
                        .gap(8.0)
                        .padding(10.0)
                        .radius(k.radius.sm)
                        .fill(k.c.layer_02)
                        .child(text("Remove this key? Its agents wait.").font(k.ty.small).color(k.c.text_base).wrap())
                        .child(
                            row()
                                .gap(6.0)
                                .child(small("Remove key").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| act(s, &i, What::KeyRemove, Request::AccountKeyRemove { id: i.clone() })).build(k))
                                .child(small("Cancel").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| drop(s.ext.get_mut::<Accounts>().confirm_key.take().filter(|c| *c == j))).build(k)),
                        ),
                );
            }
            card = key_out_lines(k, a, now, card);
            // a refused key: the field for a new one is open at once
            if field.is_some() || a.key_since.is_none() || key_refused {
                // whose key, on a card met later too
                if a.key_since.is_none() {
                    card = card.child(faint(k, if agy { GEMINI_KEY_HINT } else { KEY_HINT }));
                }
                let empty = TextEdit::new("");
                card = card.child(key_field(k, &a.id, field.unwrap_or(&empty), a.key_since.is_some()));
            }
        }
        Some(si) => card = card.child(signing_in(k, &a.id, si, false)),
        // its key on its way into opencode's sign-in, which Cancel ends
        None if setting.is_some() => card = card.child(setting_key(k, &a.id, setting == Some(true))),
        // an opencode account: its key goes in the masked field, which coordd hands to opencode's sign-in, once; one whose
        // provider takes no key signs in by its way, the header's `Sign in`. One on a key says why it is out as a key card
        // does, the field open at once on a refused key
        None if oc && (!a.signed_in || field.is_some() || key_out(a, now).is_some()) => {
            if !a.signed_in {
                card = card.child(needs_you(k, if a.signin_running { "Sign-in running" } else if a.put_back.is_some() { "Put back: sign in again" } else { "Not signed in" }));
            }
            card = key_out_lines(k, a, now, card);
            if (!a.signed_in || field.is_some() || key_refused) && !in_sheet && let Some(from) = oc_key_from(a) {
                let empty = TextEdit::new("");
                card = card.child(faint(k, from)).child(key_field(k, &a.id, field.unwrap_or(&empty), a.signed_in));
            }
        }
        // a sign-in started before (another window, or before a swap) still waits for its link to be opened
        None if !a.signed_in => card = card.child(needs_you(k, if a.signin_running { "Sign-in waiting: open the link" } else if a.put_back.is_some() { "Put back: sign in again" } else { "Not signed in" })),
        None => {}
    }
    if a.deaf.is_some() {
        card = card.child(deaf_line(s, k, &a.id));
    }
    if confirming {
        let i = id.clone();
        // who moves where, asked once; below ACCOUNT_LEAVE coordd refuses in words
        let leave = s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_LEAVE);
        let gateway = st.got.get(&s.cur).and_then(|r| r.as_ref().ok()).is_some_and(|n| n.gateway);
        let accounts = merged(&st.got);
        let one = a.agents.len() == 1;
        let (question, nowhere) = match moves_to(a, &accounts, gateway, now).filter(|_| leave) {
            // an opencode or agy agent keeps its account while its session runs: nothing moves it, and coordd refuses
            Some(_) if oc || agy => (stop_first(&who(a), one, &a.label), true),
            None if a.key => (format!("Remove {}?", a.label), false),
            None => (format!("Remove {}? {} signs it out.", a.label, signs(a)), false),
            Some(Ok(to)) => (format!("Remove {}? {} move{} {to} after their turn{}.", a.label, who(a), if a.agents.len() == 1 { "s" } else { "" }, if a.key { String::new() } else { format!(", then {} signs it out", signs(a)) }), false),
            Some(Err(())) => {
                (format!("{} {} on {}; sign in another account first", who(a), if one { "runs" } else { "run" }, a.label), true)
            }
        };
        card = card.child(
            col()
                .w_full()
                .gap(8.0)
                .padding(10.0)
                .radius(k.radius.sm)
                .fill(k.c.layer_02)
                .child(text(question).font(k.ty.small_medium).color(k.c.text_base).wrap())
                .child(
                    row()
                        .gap(6.0)
                        .children((!nowhere).then(|| small("Remove").variant(ButtonVariant::Contrast).on_press(move |s: &mut App| act(s, &i, What::Remove, Request::AccountRemove { id: i.clone() })).build(k)))
                        .child(small("Cancel").variant(ButtonVariant::Ghost).on_press(|s: &mut App| s.ext.get_mut::<Accounts>().confirm_remove = None).build(k)),
                ),
        );
    }
    // being removed: its agents leave at the end of their turn; Keep it takes the mark away
    if a.leaving.is_some() {
        let i = id.clone();
        let words = if a.agents.is_empty() { "Being removed".to_string() } else { format!("Being removed: {} move{} after their turn", who(a), if a.agents.len() == 1 { "s" } else { "" }) };
        let keep = small("Keep it").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| act(s, &i, What::Keep, Request::AccountKeep { id: i.clone() })).build(k);
        card = card.child(row().w_full().gap(8.0).center_y().child(faint(k, words).grow(1.0)).child(keep));
        // an agent with nowhere to go waits on it: said on the card, not behind the hover
        card = card.children(a.agents.iter().filter_map(|x| Some(faint(k, format!("{}.{}: {}", x.coord, x.agent, x.note.as_deref()?)))));
    }
    // another coord's coordd holds it now
    if let Some(d) = &a.locked_by {
        card = card.child(faint(k, format!("Waiting for {d} to finish with this account")));
    }
    let go_month = Some(("Monthly", &a.month)).filter(|_| go_windows);
    let windows: Vec<(String, &LimitWindow)> = if drawn { [("5-hour", &a.five_hour), ("Weekly", &a.seven_day)].into_iter().chain(go_month).filter_map(|(n, w)| Some((n.to_string(), w.as_ref()?))).collect() } else { Vec::new() };
    // a refused login is said as one: until 0.2 D22 D it fell into `Out, reset unknown`; coordd's words on the hover
    if refused {
        let i = id.clone();
        let again = small("Try again").variant(ButtonVariant::Ghost).help("Mark its login as working").on_press(move |s: &mut App| clear(s, &i)).build(k);
        let mut line = needs_you(k, "Login refused: sign in again");
        if let Some(w) = a.out_words.as_deref().filter(|w| !w.trim().is_empty()) {
            line = line.help(w.to_string());
        }
        card = card.child(row().w_full().gap(8.0).center_y().justify_between().child(line).child(on_hover(again)));
    }
    match out {
        Some(UNKNOWN_RESET) => {
            let i = id.clone();
            let clear = small("It can take requests").variant(ButtonVariant::Ghost).help("Mark this account as back").on_press(move |s: &mut App| clear(s, &i)).build(k);
            card = card.child(row().w_full().gap(8.0).center_y().justify_between().child(needs_you(k, "Out, reset unknown")).child(on_hover(clear)));
        }
        // said once: a used-up window that starts again then already says it
        Some(t) if windows.iter().any(|(_, w)| w.used_percent >= 100.0 && (w.resets_at - t).abs() <= 60) => {}
        Some(t) => card = card.child(faint(k, format!("Out until {} ({})", crate::when::secs(t), countdown(t, now)))),
        None => {}
    }
    // an agy account out for some of its models (coordd's `State::models_out`): its other models still take agents; each
    // said by agy's name, the refusal's words on the hover
    for o in models_out {
        let name = o.model.strip_prefix("agy/").unwrap_or(&o.model);
        let line = match o.until {
            UNKNOWN_RESET => faint(k, format!("Out for {name}, reset unknown")),
            t => faint(k, format!("Out for {name} until {} ({})", crate::when::secs(t), countdown(t, now))),
        };
        card = card.child(match o.words.as_deref().filter(|w| !w.trim().is_empty()) {
            Some(w) => line.help(w.to_string()),
            None => line,
        });
    }
    // past a limit with extra usage on, Claude Code's `/usage` says its requests run on it until the limit resets, as
    // the gateway's card has an "Extra usage" row (its amounts are not in Claude Code's words); coordd sends it no
    // agent while it is out all the same
    if let Some(t) = a.extra_usage_until.filter(|t| *t > now) {
        card = card.child(faint(k, format!("On extra usage until {} ({})", crate::when::secs(t), countdown(t, now))).help("Paid past the plan's limits until reset"));
    }
    // refresh shows it is happening (0.2 D20): `act` took the last answer away, and the card said nothing until the
    // new one came. One ring, with the gateway card's word, where the windows stand (they are drawn again from what it
    // read), else here.
    let ringing = querying(s, &a.id);
    let ring = || row().gap(8.0).center_y().child(Spinner::new().size(18.0).build(k)).child(faint(k, "Querying"));
    if ringing && !drawn {
        card = card.child(ring());
    }
    match st.said.get(&a.id) {
        Some(Ok(t)) => card = card.child(faint(k, t.clone())),
        Some(Err(e)) => card = card.child(needs_you(k, e.clone())),
        None => {}
    }
    // ran out: the gateway's card says the same words for the same state (proxy `creds::Login`); a temporary limit
    // is the faint `Out until …` above
    if expired {
        card = card.child(needs_you(k, "expired · sign in again"));
    } else if let Some(t) = a.login_expires.filter(|t| *t - now < 7 * 86400) {
        card = card.child(needs_you(k, format!("Login runs out {}", crate::when::secs(t))));
    }
    // Claude's 5-hour and weekly windows, and Go's three; another opencode provider's limits are not seen (its calls
    // pass no gateway of ours)
    if drawn {
        // `Limits not seen yet` has the press beside it, as on the gateway's card
        card = if ringing {
            card.child(ring())
        } else if go_windows && !a.usage_key && windows.is_empty() {
            // a Go account whose key yaaihh does not keep (signed in before it read Go's page): its key once more, in
            // the card's field, and Go's page is read with it
            let i = id.clone();
            let add = small("Replace key").help("Replace the key its usage is read with").on_press(move |s: &mut App| drop(s.ext.get_mut::<Accounts>().keys.insert(i.clone(), TextEdit::new(""))));
            let add = (field.is_none() && setting.is_none()).then(|| add.build(k).key(format!("usage-key-{}", a.id)));
            card.child(row().w_full().gap(8.0).center_y().justify_between().child(faint(k, "Limits need its key once more").grow(1.0).shrink().w(0.0)).children(add))
        } else if windows.is_empty() && a.model_weeks.is_empty() {
            let i = id.clone();
            let press = checks.then(|| small("Check limits").help(check_help(s, a)).on_press(move |s: &mut App| check(s, &i)).build(k).key(format!("check-limits-{}", a.id)));
            card.child(row().w_full().gap(8.0).center_y().justify_between().child(faint(k, "Limits not seen yet")).children(press))
        } else {
            // its weeks of one model after its own two, as `/usage` names them, each row with when it was read
            let own = (a.limits_seen, a.limits_from.as_str());
            let weeks_read = (a.model_weeks_seen.unwrap_or(a.limits_seen), if agy { a.limits_from.as_str() } else { "usage" });
            let rows = windows.into_iter().map(|(n, w)| window(k, (&n, &n), w, Span::named(&n), own, now));
            // an agy account's under the limits t3code names, each limit's name once over its windows
            let weeks: Vec<El> = if agy {
                agy_groups(&a.model_weeks)
                    .into_iter()
                    .flat_map(|(limit, ws)| (!limit.is_empty()).then(|| faint(k, limit)).into_iter().chain(ws.into_iter().map(|(n, m)| window(k, (n, &m.model), &m.window, Span::named(n), weeks_read, now))))
                    .collect()
            } else {
                a.model_weeks.iter().map(|m| { let n = week_name(m, false); window(k, (&n, &n), &m.window, Some(Span::WEEK), weeks_read, now) }).collect()
            };
            let card = card.child(col().w_full().gap(10.0).children(rows.chain(weeks)));
            // not live: the last reading, with when it was read and the refresh beside
            let live = live(a, now);
            let card = if live {
                card
            } else {
                let i = id.clone();
                let press = checks.then(|| small("Check limits").help(check_help(s, a)).on_press(move |s: &mut App| check(s, &i)).build(k).key(format!("check-limits-{}", a.id)));
                card.child(as_of(k, a.limits_seen, press))
            };
            // the weeks of one model come from `/usage` alone (a status line has none): said when, where that is not
            // the time the card already says (E5's `model_weeks_seen`)
            match a.model_weeks_seen.filter(|t| !a.model_weeks.is_empty() && (live || (t - a.limits_seen).abs() > 60)) {
                Some(t) => {
                    let names: Vec<String> = a.model_weeks.iter().map(|m| week_name(m, agy)).collect();
                    card.child(faint(k, format!("{} as of {}", names.join(", "), crate::when::secs(t))))
                }
                None => card,
            }
        };
        if claude_windows {
            card = card.children(reset_row(s, k, a));
        }
    }
    // no agent and no use this month: the card says nothing of them
    if a.agents.is_empty() && a.usage.is_empty() && a.on_gateway.is_empty() {
        return card.key(format!("native-account-{}-{}", a.harness, a.id));
    }
    let agents: Vec<Listed> = a.agents.iter().map(|x| listed(s, x)).collect();
    card = card.child(agents_fold(s, k, &a.id, "Agents", false, &agents));
    // agents that left it at its limit and go on through the gateway until it is back
    if !a.on_gateway.is_empty() {
        card = card.child(faint(k, format!("On the gateway until it is back: {}", a.on_gateway.iter().map(|x| format!("{}.{}", x.coord, x.agent)).collect::<Vec<_>>().join(" · "))));
    }
    let requests: u64 = a.usage.iter().map(|u| u.requests).sum();
    let all = a.usage.iter().map(tokens).fold(Spend::default(), |mut s, t| {
        (s.input, s.cache_read, s.cache_write, s.output, s.reasoning) = (s.input + t.input, s.cache_read + t.cache_read, s.cache_write + t.cache_write, s.output + t.output, s.reasoning + t.reasoning);
        s
    });
    let month_name = chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").map_or(month.to_string(), |d| d.format("%B").to_string());
    let id = a.id.clone();
    let toggle = Button::new(if open { "▾ details" } else { "▸ details" })
        .control_size(ControlSize::Regular)
        .variant(ButtonVariant::Ghost)
        .disabled(a.usage.is_empty())
        .on_press(move |s: &mut App| {
            let open = &mut s.ext.get_mut::<Accounts>().open;
            if !open.remove(&id) {
                open.insert(id.clone());
            }
        })
        .build(k);
    card = card.child(
        row()
            .w_full()
            .gap(8.0)
            .center_y()
            .child(text(month_name).font(k.ty.small).color(k.c.text_muted).w(64.0))
            .child(text(format!("{} requests ·", count(requests as f64))).font(k.ty.small).color(k.c.text_base))
            .child(spend(k, &all))
            .child(el().grow(1.0))
            .child(toggle),
    );
    if open {
        let mut list = col().w_full().gap(6.0).padding(8.0).radius(k.radius.sm).fill(k.c.layer_02);
        for u in &a.usage {
            let mut said = format!("{} requests", count(u.requests as f64));
            if u.failed > 0 {
                said += &format!(", {} failed", count(u.failed as f64));
            }
            if u.timed > 0 {
                said += &format!(", {:.1} s each", u.duration_ms as f64 / u.timed as f64 / 1000.0);
            }
            if u.cache_write_1h > 0 {
                said += &format!(", {} of the cache write for 1 h", compact(u.cache_write_1h as f64));
            }
            list = list.child(col().w_full().gap(2.0).child(row().w_full().gap(8.0).justify_between().child(text(u.model.clone()).font(k.ty.small_medium).color(k.c.text_base).truncate()).child(spend(k, &tokens(u)))).child(faint(k, said)));
        }
        card = card.child(list);
    }
    card.key(format!("native-account-{}-{}", a.harness, a.id))
}

/// Sign-in account A's login has passed its end at NOW (`loginExpiresAt`, as Claude Code said it): only a new sign-in
/// brings it back, though coordd's last `auth status` may still have said signed in.
/// Whether sign-in account A's login was refused (0.2 D22 D): coordd found two turns refused (`authentication_failed`,
/// 401 or 403) with none answered between, and it takes no agent until a new sign-in or his `Try again`. One refused
/// turn alone only restarts that agent, and the card says nothing of it. A key's refusal, a key account's or an
/// opencode account's on a key ([`keyed`]), is `Key refused`; an opencode account whose provider signs in only by its
/// browser way is refused here, with its own `Sign in`.
fn login_refused(a: &NativeAccount, now: i64) -> bool {
    !keyed(a) && a.out_for.as_deref() == Some("authentication_failed") && a.out_until.is_some_and(|t| t > now)
}

/// Where an Owner of a Claude organisation turns channels on (Organization settings › Claude Code › Channels).
const CHANNELS: &str = "https://claude.ai/admin-settings/claude-code";

/// What a deaf account's press says before the link: what coordd saw, and the cause only where it is one. coordd judges
/// deaf from two deliveries never seen, and nothing on the card tells a Team seat from his own account.
const DEAF_WHY: &str = "Messages not seen. An Owner enables channels in";

/// Account ID's line when it is deaf (0.2 D22 D: two deliveries handed to sessions on it were never seen there; on
/// 20 Nov a Team seat whose organisation has channels off): `Deaf: messages do not reach agents here ▸`, in the one
/// accent. A press opens what was seen ([`DEAF_WHY`]), where an Owner turns channels on, and `Try again` (the clear:
/// coordd tries it again with its next agent). Its agents have moved already; their rows say why.
fn deaf_line(s: &App, k: &Kit, id: &str) -> El {
    let open = s.ext.get::<Accounts>().deaf_open.contains(id);
    let i = id.to_string();
    let line = k.interactive(needs_you(k, format!("Deaf: messages do not reach agents here {}", if open { "▾" } else { "▸" }))).key(format!("deaf-{id}")).on_press(move |s: &mut App| {
        let o = &mut s.ext.get_mut::<Accounts>().deaf_open;
        if !o.remove(&i) {
            o.insert(i.clone());
        }
    });
    if !open {
        return line;
    }
    let i = id.to_string();
    let link = k.interactive(text("Organization settings › Claude Code › Channels ↗").font(k.ty.small).color(k.c.text_accent)).help(CHANNELS).on_press(|_: &mut App| open_link(CHANNELS.to_string()));
    let again = Button::new("Try again").control_size(ControlSize::Regular).variant(ButtonVariant::Ghost).help("Try it with the next agent").on_press(move |s: &mut App| clear(s, &i)).build(k);
    let why = col().w_full().gap(6.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_02).child(text(DEAF_WHY).font(k.ty.small).color(k.c.text_base).wrap()).child(link).child(row().child(again));
    col().w_full().gap(6.0).child(line).child(why)
}

fn login_ran_out(a: &NativeAccount, now: i64) -> bool {
    !a.key && a.signed_in && a.login_expires.is_some_and(|t| t <= now)
}

/// Why account A on a key ([`keyed`]) is out at NOW, when coordd said (`out_for`): `authentication_failed`,
/// `billing_error` or `rate_limit`; None when it is not out or not on a key.
fn key_out(a: &NativeAccount, now: i64) -> Option<&str> {
    a.out_until.filter(|t| *t > now).and(a.out_for.as_deref()).filter(|_| keyed(a))
}

/// CARD with why account A on a key is out at NOW: `Key refused`, `No credit` with `It can take requests` on the hover,
/// coordd's words under each, or `At its limit, taken again at T` when coordd takes it again by itself; a limit with its
/// own end is `Out until …` further down. A key account's card and an opencode account's on a key alike.
fn key_out_lines(k: &Kit, a: &NativeAccount, now: i64, card: El) -> El {
    let Some(why) = key_out(a, now) else { return card };
    let words = a.out_words.clone().filter(|w| !w.trim().is_empty());
    match why {
        "authentication_failed" => card.child(needs_you(k, "Key refused")).children(words.map(|w| faint(k, w))),
        "billing_error" => {
            let i = a.id.clone();
            let clear = Button::new("It can take requests")
                .control_size(ControlSize::Regular)
                .variant(ButtonVariant::Ghost)
                .on_press(move |s: &mut App| clear(s, &i))
                .build(k);
            card.child(row().w_full().gap(8.0).center_y().justify_between().child(needs_you(k, "No credit")).child(on_hover(clear))).children(words.map(|w| faint(k, w)))
        }
        _ => match a.again_at {
            Some(t) => card.child(faint(k, format!("At its limit, taken again at {}", crate::when::secs(t)))),
            None => card,
        },
    }
}

/// A sign-in under way on account ID, on its card or (SHEET) in the Add account sheet: its link with `Copy link`, for
/// him to open in the browser he means, the code field for when the browser shows one, and cancel (on a card under the
/// pointer). Nothing here opens his browser. The box goes when coordd says the account is signed in.
fn signing_in(k: &Kit, id: &str, si: &SignIn, sheet: bool) -> El {
    let mut c = col().w_full().gap(8.0).padding(10.0).radius(k.radius.sm).fill(k.c.layer_02);
    let (oc, agy) = (matches!(si.whose, Whose::Opencode(_)), si.whose == Whose::Agy);
    let Some(url) = &si.url else { return c.child(faint(k, format!("Starting {}'s sign-in…", si.program()))) };
    let (u1, i1, i2, i3) = (url.clone(), id.to_string(), id.to_string(), id.to_string());
    let lead = if agy { "Copy this link; sign in with Google there" } else { "Copy this link; sign in there" };
    c = c
        .child(text(lead).font(k.ty.small_medium).color(k.c.text_base).wrap())
        .child(text(url.clone()).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere().help(url.clone()))
        // opencode's and agy's links are known on their sign-in hosts alone; any other is said to be foreign
        .children((!si.known(url)).then(|| needs_you(k, if agy { "Not Google's sign-in page." } else { "Not opencode's sign-in page." })))
        .children(si.said.as_ref().map(|t| text(t.clone()).font(k.ty.mono).color(k.c.text_base).wrap()))
        .child(
            row()
                .gap(6.0)
                .child(Button::new("Copy link").icon(k.glyph.copy).control_size(ControlSize::Regular).on_press_tx(move |_: &mut App, tx: &mut Tx| tx.copy(u1.clone())).build(k))
                .child({
                    let cancel = Button::new(if sheet { "Cancel sign-in" } else { "Cancel" }).variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).on_press(move |s: &mut App| act(s, &i1, What::Cancel, Request::AccountCancel { id: i1.clone() })).build(k);
                    if sheet { cancel } else { on_hover(cancel) }
                }),
        );
    // opencode's way that says what to enter on the page waits for the page by itself: no code comes back to paste
    if oc && si.said.is_some() {
        return c;
    }
    // agy's Google sign-in always ends with the code Google shows
    c = c.child(faint(k, if agy { "Then paste the code Google shows here:" } else { "Paste a code, if one is shown:" }));
    let whose = si.whose;
    let field = TextInput::new(&si.code).placeholder("Code").width(None).on_edit(move |s: &mut App| &mut s.ext.get_mut::<Accounts>().signin.entry(i2.clone()).or_insert_with(|| SignIn::new(whose)).code).build(k);
    let send = Button::new(if si.sending { "Waiting…" } else { "Send code" })
        .control_size(ControlSize::Regular)
        .disabled(si.sending || si.code.text.trim().is_empty())
        .on_press(move |s: &mut App| send_code(s, &i3))
        .build(k);
    c.child(row().w_full().gap(6.0).center_y().child(field.grow(1.0)).child(send))
}

/// The tiers the overview shows under HARNESS, highest first ([`tiers_of`] over his accounts): for Claude Code's, also
/// those an extension's Claude accounts stand in ([`crate::ext::AccountsExt::tiers`]). `Tier N` is a tier's place in
/// this list, on the overview and in the sheet.
pub fn tiers(s: &App, harness: &str) -> Vec<i64> {
    let more: Vec<i64> = if harness == CLAUDE { crate::ext::all().iter().filter_map(|e| e.accounts.as_ref()).flat_map(|a| (a.tiers)(s)).collect() } else { Vec::new() };
    tiers_of(&shown(s).0, harness, more)
}

/// The tiers of ACCOUNTS of HARNESS and MORE, highest first, each once: a harness's own, never another's (an agent
/// takes accounts of its harness alone).
fn tiers_of(accounts: &[NativeAccount], harness: &str, more: Vec<i64>) -> Vec<i64> {
    let mut t: Vec<i64> = accounts.iter().filter(|a| a.harness == harness).map(|a| a.tier).chain(more).collect();
    t.sort_by(|a, b| b.cmp(a));
    t.dedup();
    t
}

/// The page's one `Add account`: it opens the sheet. There where the sheet has a tile: the project's coordd answers
/// the account requests, or an extension brings tiles of its own. Public for the gateway's Accounts tab, which stands
/// it in its toolbar in a window built with it.
pub fn add_button(s: &App, k: &Kit) -> Option<El> {
    (s.proj().is_some_and(served) || crate::ext::all().iter().any(|e| e.accounts.is_some())).then(|| Button::new("Add account").variant(ButtonVariant::Contrast).icon(&PLUS).help("Add an account").on_press(open_sheet).build(k))
}

static PLUS: rune::Icon = rune::Icon { name: "accounts-plus", view: 24.0, body: r#"<path d="M5 12h14M12 5v14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"# };

/// A family's heading on the overview: the provider's logo and name.
pub fn family(k: &Kit, logo: Option<&'static rune::Icon>, name: &str) -> El {
    row().gap(8.0).center_y().children(logo.map(|l| icon(l, 18.0))).child(text(name.to_string()).font(k.ty.ui_medium).color(k.c.text_base))
}

/// A tier's box on the overview, headed `Tier N` (its number behind the hover), with its cards.
pub fn tier_box(k: &Kit, n: usize, tier: Option<i64>, cards: Vec<El>) -> El {
    let head = text(format!("Tier {n}")).font(k.ty.small_medium).color(k.c.text_muted);
    let head = match tier {
        Some(t) => head.help(format!("Priority {t}; higher goes first")),
        None => head,
    };
    col().w_full().gap(8.0).padding(10.0).radius(k.radius.md).border(1.0, k.c.border_weak).fill(k.c.layer_02).child(head).child(row().w_full().gap(12.0).flex_wrap().items_start().children(cards))
}

/// The agents that run through the gateway, where a coordd answered that it has one, as `coord.agent`: those not
/// stopped that are on none of his accounts, and those an account's card says are on the gateway until it is back
/// (the same agent, said from the account's side: the two must agree on one page).
fn on_gateway(s: &App) -> Option<Vec<Listed>> {
    let st = s.ext.get::<Accounts>();
    let with: Vec<(usize, &NativeAccounts)> = st.got.iter().filter_map(|(i, r)| Some((*i, r.as_ref().ok().filter(|n| n.gateway)?))).collect();
    if with.is_empty() {
        return None;
    }
    let mut agents: Vec<Listed> = Vec::new();
    for (i, n) in &with {
        if let Some(p) = s.projects.get(*i) {
            // an opencode or agy agent runs on an account of its harness (its own sign-in or key), never through the
            // gateway
            agents.extend(p.live.agents.iter().filter(|a| a.account.is_none() && a.state != "stopped" && a.kind != "opencode" && a.kind != "agy").map(|a| Listed { name: format!("{}.{}", n.coord, a.name), id: Some(a.id.clone()), detail: moved(s, a).or(a.account_note.clone()), stopped: false }));
        }
        agents.extend(n.accounts.iter().flat_map(|a| &a.on_gateway).map(|x| listed(s, x)));
    }
    agents.sort_by(|a, b| a.name.cmp(&b.name));
    agents.dedup_by(|a, b| a.name == b.name);
    Some(agents)
}

/// The gateway as one card, in a window without the gateway's own pages (the Windows window, where its accounts are
/// not read): `Through the gateway  17 agents ▸`, the agents on none of his accounts. No controls.
fn gateway_card(s: &App, k: &Kit, agents: &[Listed]) -> El {
    col()
        .w(340.0)
        .gap(10.0)
        .padding(14.0)
        .radius(k.radius.md)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_weak)
        .child(col().w_full().gap(2.0).child(text("Gateway").font(k.ty.ui_medium).color(k.c.text_base)).child(text("your gateway's accounts").font(k.ty.small).color(k.c.text_muted)))
        .child(agents_fold(s, k, GATEWAY, "Through the gateway", true, agents))
        .key("native-account-gateway")
}

/// The page: one line with the count, the switch and the one `Add account`; then his accounts by tier, highest first,
/// each tier a box of cards headed `Tier N`; the removed ones folded at the foot; the sheet over it once opened.
fn page(s: &App, k: &Kit, _: &str) -> (String, El) {
    let st = s.ext.get::<Accounts>();
    let mut layer: Vec<El> = Vec::new();
    let mut c = col().w_full().gap(14.0).px(24.0).py(16.0);
    let mut errors: Vec<(&usize, &String)> = st.got.iter().filter_map(|(i, r)| Some((i, r.as_ref().err()?))).collect();
    errors.sort();
    let accounts = merged(&st.got);
    let month = st.got.values().find_map(|r| Some(r.as_ref().ok()?.month.clone())).unwrap_or_default();
    // the gateway's own cards come from its extension; without it the gateway is one card
    let gateway = if crate::ext::all().iter().any(|e| e.accounts.is_some()) { None } else { on_gateway(s) };
    if st.got.is_empty() {
        c = c.child(faint(k, "asking the engine…"));
    } else if errors.len() < st.got.len() {
        let said = match accounts.len() {
            0 => "No accounts yet".to_string(),
            1 => "1 account".to_string(),
            n => format!("{n} accounts"),
        };
        let mut bar = row().w_full().gap(12.0).center_y().flex_wrap().child(text(said).font(k.ty.ui_medium).color(k.c.text_base).grow(1.0));
        if !accounts.is_empty() {
            bar = bar.children(all_switch(s, k)).children(check_all_button(s, k));
        }
        c = c.child(bar.children(add_button(s, k)));
    }
    // no way for yaaihh's own calls (the message box's cleaning, titles): no gateway
    if s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNTS_ALL) && no_own_calls(&st.got) {
        // they run through the gateway alone, on Codex, never a Claude model, so a key account gives them no way
        c = c.child(needs_you(k, "Cleaning and titles need the gateway."));
    }
    let now = now_s();
    // each harness under its own heading, with its own tiers: an agent takes accounts of its harness alone, by their
    // tiers (coordd's router.rs); the gateway's agents are Claude Code's, after Claude's tiers
    for (harness, logo, name) in FAMILIES {
        let all = tiers(s, harness);
        let here = gateway.as_ref().filter(|_| harness == CLAUDE);
        if !accounts.iter().any(|a| a.harness == harness) && here.is_none() {
            continue;
        }
        c = c.child(family(k, Some(logo), name));
        for (n, tier) in all.iter().enumerate() {
            let mut list: Vec<&NativeAccount> = accounts.iter().filter(|a| a.harness == harness && a.tier == *tier).collect();
            if list.is_empty() {
                continue;
            }
            list.sort_by(|a, b| a.label.cmp(&b.label));
            let cards: Vec<El> = list.iter().map(|a| native_card(s, k, a, &month, now, None, &mut layer)).collect();
            c = c.child(tier_box(k, n + 1, Some(*tier), cards).key(format!("native-tier-{harness}-{tier}")));
        }
        if let Some(agents) = here {
            c = c.child(tier_box(k, all.len() + 1, None, vec![gateway_card(s, k, agents)]).key("native-tier-gateway"));
        }
    }
    c = c.children(removed_fold(s, k));
    for (i, e) in errors {
        let name = s.projects.get(*i).map_or("?", |p| p.name.as_str());
        c = c.child(faint(k, format!("{name}: {e}")));
    }
    sheet(s, k, &mut layer);
    let page = col().key("accounts-page").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(c);
    ("Accounts".into(), el().key("accounts-root").w_full().h_full().child(page).children(layer))
}

/// The page's foot: what coordd said of the last account removed (its card is gone), and the removed accounts, folded,
/// each with `Put back`; nothing where there is neither. Public for the gateway's Accounts tab.
pub fn removed_fold(s: &App, k: &Kit) -> Option<El> {
    if !s.proj().is_some_and(served) {
        return None;
    }
    let st = s.ext.get::<Accounts>();
    let last = st.removed.as_ref().map(|r| faint(k, r.clone()));
    // the removed accounts stay in sight, folded
    let gone = removed_all(&st.got);
    // Remove, the list and Put back go together, behind one gate
    if gone.is_empty() || !s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNT_REMOVE) {
        return last;
    }
    let open = st.show_removed;
    let fold = Button::new(format!("{} Removed accounts ({})", if open { "▾" } else { "▸" }, gone.len()))
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Regular)
        .on_press(|s: &mut App| {
            let st = s.ext.get_mut::<Accounts>();
            st.show_removed = !st.show_removed;
        })
        .build(k);
    let mut list = col().gap(4.0).children(last).child(row().child(fold));
    if open {
        list = list.children(gone.iter().map(|r| {
            let name = r.name.clone();
            let back = Button::new("Put back").variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).on_press(move |s: &mut App| act(s, &name, What::PutBack, Request::AccountPutBack { name: name.clone() })).build(k);
            let line = faint(k, removed_row(r)).grow(1.0);
            let line = match removed_help(r) {
                Some(h) => line.help(h),
                None => line,
            };
            row().w_full().gap(8.0).center_y().hover_group().child(line).child(on_hover(back))
        }));
    }
    Some(list)
}

/// The scene `accounts`: the page as the first project's coordd answers it, or, with YAAIHH_ACCOUNTS_DATA=FILE, as
/// FILE says (a `NativeAccounts`, or a list of them, one per coord: made-up accounts for a still).
fn scene(s: &mut App) {
    scene_data(s);
    s.sel = crate::Sel::Ext(NAME.into(), String::new());
}

/// The accounts a scene shows, read once ([`scene`]); also the proxy's scene of its Accounts tab with them.
pub fn scene_data(s: &mut App) {
    let got: Vec<Result<NativeAccounts, String>> = match std::env::var("YAAIHH_ACCOUNTS_DATA") {
        Ok(f) => {
            let v: serde_json::Value = std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            let list = if v.is_array() { v } else { serde_json::Value::Array(vec![v]) };
            serde_json::from_value::<Vec<NativeAccounts>>(list).map_or_else(|e| vec![Err(format!("{f}: {e}"))], |l| l.into_iter().map(Ok).collect())
        }
        // the coordd's hello and its agents too, as the live link takes them (lib.rs `read_once`): a still shows what
        // the window would
        Err(_) => s
            .projects
            .iter_mut()
            .map(|p| {
                let mut answer = Err(format!("{}: no reply", p.name));
                for m in crate::link::once(&p.cmd, &[Request::Accounts { json: true }]) {
                    match m {
                        crate::link::Incoming::Event(ev) => _ = p.apply(ev),
                        crate::link::Incoming::Hello(h) => p.protocol = h.protocol,
                        crate::link::Incoming::Reply(_, r) => answer = if r.ok { Ok(r.out) } else { Err(r.out) },
                        _ => {}
                    }
                }
                parse(answer)
            })
            .collect(),
    };
    // made-up accounts are answered as a coordd of this build answers: what the page shows from a protocol on, shows
    if std::env::var_os("YAAIHH_ACCOUNTS_DATA").is_some() {
        for p in &mut s.projects {
            p.protocol = coord_types::PROTOCOL;
        }
    }
    // the agents of a snapshot (YAAIHH_SNAPSHOT_FILE), as the first project's: their rows on the cards and the gateway's
    crate::snapshot_scene(s);
    let st = s.ext.get_mut::<Accounts>();
    st.got = got.into_iter().enumerate().collect();
    // YAAIHH_ACCOUNTS_OPEN=ID,…: those accounts' details open
    st.open = std::env::var("YAAIHH_ACCOUNTS_OPEN").unwrap_or_default().split(',').filter(|x| !x.is_empty()).map(str::to_string).collect();
    // YAAIHH_ACCOUNTS_AGENTS=KEY,…: those `Agents ▸` lists open, as a press opens them (an account's id, or `*gateway`)
    st.agents_open = std::env::var("YAAIHH_ACCOUNTS_AGENTS").unwrap_or_default().split(',').filter(|x| !x.is_empty()).map(str::to_string).collect();
    // YAAIHH_ACCOUNTS_MENU=KEY: that card's `⋯` menu open ([`card_menu`]'s key: `claude-ID` for one of his accounts)
    st.menu = std::env::var("YAAIHH_ACCOUNTS_MENU").ok().filter(|m| !m.is_empty());
    // YAAIHH_ACCOUNTS_SIGNIN=ID=URL: a sign-in of ID under way, its URL answered, as the Sign in button leaves it
    // (by the account's harness: an opencode account's by its provider's browser way, none where it has no such way;
    // an agy account's agy's own)
    if let Some((id, url)) = std::env::var("YAAIHH_ACCOUNTS_SIGNIN").ok().as_deref().and_then(|v| v.split_once('=')) {
        let whose = match merged(&st.got).into_iter().find(|a| a.id == id) {
            Some(a) if is_oc(&a) => oc_provider(&a).and_then(|p| p.way).map(|(way, _)| Whose::Opencode(way)),
            Some(a) if is_agy(&a) => Some(Whose::Agy),
            _ => Some(Whose::Claude),
        };
        if let Some(whose) = whose {
            st.signin.insert(id.to_string(), SignIn { url: Some(url.to_string()), ..SignIn::new(whose) });
        }
    }
    // YAAIHH_ACCOUNTS_SHEET=tiles | claude | anthropic | opencode | agy | agy-key, or one of those five :ID: the sheet
    // opened on its tiles, on a tile's form (YAAIHH_ACCOUNTS_ADD=LABEL typed in it; YAAIHH_ACCOUNTS_PROVIDER=N the
    // `opencode` tile's provider, by its place in OPENCODE_PROVIDERS), or on account ID's step after it was made
    if let Ok(v) = std::env::var("YAAIHH_ACCOUNTS_SHEET") {
        let (pick, id) = v.split_once(':').map_or((v.as_str(), None), |(p, i)| (p, Some(i)));
        st.add.open = true;
        st.add.pick = match pick {
            "claude" => Some(Pick::Claude),
            "anthropic" => Some(Pick::Anthropic),
            "opencode" => Some(Pick::Opencode),
            "agy" => Some(Pick::Agy),
            "agy-key" => Some(Pick::AgyKey),
            _ => None,
        };
        st.add.provider = std::env::var("YAAIHH_ACCOUNTS_PROVIDER").ok().and_then(|t| t.parse().ok()).unwrap_or(0);
        st.add.label = TextEdit::new(std::env::var("YAAIHH_ACCOUNTS_ADD").unwrap_or_default());
        st.add.tier = std::env::var("YAAIHH_ACCOUNTS_TIER").ok().and_then(|t| t.parse().ok()).unwrap_or(0);
        if let Some(id) = id {
            let label = merged(&st.got).into_iter().find(|a| a.id == id).map_or(st.add.label.text.clone(), |a| a.label);
            st.add.made = Some((id.to_string(), label));
        }
        if matches!(st.add.pick, Some(Pick::Anthropic | Pick::AgyKey)) && id.is_none() {
            st.add.secret = TextEdit::new("a-made-up-key-for-a-still-only");
        }
    }
    // YAAIHH_ACCOUNTS_SETTING=ID: account ID's key on its way into opencode's sign-in, with its Cancel
    if let Ok(id) = std::env::var("YAAIHH_ACCOUNTS_SETTING") {
        st.setting.insert(id, false);
    }
    st.asked = Some(Instant::now());
    // YAAIHH_ACCOUNTS_TILE=TILE: the sheet opened on an extension's tile TILE (its id), its steps as the extension's
    // own scene sets them; the extension's pick is not called
    if let Ok(tile) = std::env::var("YAAIHH_ACCOUNTS_TILE") {
        let found = crate::ext::all().iter().find_map(|e| e.accounts.as_ref().and_then(|a| (a.tiles)(s).into_iter().find(|t| t.id == tile)).map(|t| (e.name, t)));
        if let Some((name, t)) = found {
            s.ext.get_mut::<Accounts>().add = Add { open: true, pick: Some(Pick::Ext(name, t.id, t.label)), ..Add::default() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coord_types::Tokens;

    /// The question in plain English for one agent and for several (N-1934 p12: "stop it is first").
    #[test]
    fn stop_first_says_it_or_them() {
        assert_eq!(stop_first("research.orch", true, "Work"), "research.orch runs on Work; stop it first");
        assert_eq!(stop_first("2 agents", false, "Work"), "2 agents run on Work; stop them first");
    }

    fn answer(coord: &str, agent: &str, requests: u64) -> NativeAccounts {
        let usage = vec![AccountUsage { model: "claude-opus-5-5".into(), requests, tokens: Tokens { output: requests * 10, ..Default::default() }, ..Default::default() }];
        let agents = vec![NativeAgent { coord: coord.into(), agent: agent.into(), since: "x".into(), note: None }];
        NativeAccounts { coord: coord.into(), month: "2026-11".into(), accounts: vec![NativeAccount { harness: "claude".into(), id: "work".into(), label: "Work".into(), tier: 20, agents, usage, ..Default::default() }], ..Default::default() }
    }

    #[test]
    fn an_account_seen_from_two_coords_is_one_card_with_both_agents_and_the_usage_added() {
        let got: HashMap<usize, Result<NativeAccounts, String>> =
            [(0, Ok(answer("research", "orch", 3))), (1, Ok(answer("docket", "lead", 4))), (2, Ok(answer("research", "orch", 3))), (3, Err("old".into()))].into_iter().collect();
        let m = merged(&got);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].agents.iter().map(|a| format!("{}.{}", a.coord, a.agent)).collect::<Vec<_>>(), ["research.orch", "docket.lead"]);
        assert_eq!((m[0].usage[0].requests, m[0].usage[0].tokens.output), (7, 70), "the coord named twice counts once");
    }

    /// N-1933.p1: reading an agent's chat, or the Agents page, asks no project for its accounts, even with
    /// its agents on his accounts; the Accounts page asks each project once per round.
    #[test]
    fn the_accounts_are_asked_only_while_their_page_shows() {
        let (_held, mut s, _cat) = crate::view_cost::linked_app(&["dev", "qa"]);
        s.ext.insert(Accounts::default());
        s.projects[0].protocol = ACCOUNTS;
        s.projects[0].live.agents[0].account = Some("work".into());
        let asks = |s: &App| s.projects[0].pending.values().filter(|w| matches!(w, crate::Pending::Ext { ext, .. } if *ext == NAME)).count();
        for sel in [crate::Sel::Agent("dev".into()), crate::Sel::Agents] {
            s.sel = sel;
            for _ in 0..3 {
                poll(&mut s, &mut rune::Tx::default());
            }
            assert_eq!(asks(&s), 0, "nothing asked on {:?}", s.sel);
        }
        s.sel = crate::Sel::Ext(NAME.into(), String::new());
        poll(&mut s, &mut rune::Tx::default());
        poll(&mut s, &mut rune::Tx::default());
        assert_eq!(asks(&s), 1, "the page asks once per round");
        // N-1933.p4: an answer slower than EVERY; three rounds past EVERY leave the one request, never a pile
        for _ in 0..3 {
            s.ext.get_mut::<Accounts>().asked = Some(Instant::now() - EVERY - std::time::Duration::from_secs(1));
            poll(&mut s, &mut rune::Tx::default());
        }
        assert_eq!(asks(&s), 1, "still the one request, unanswered");
        // its answer comes: the next round is EVERY after it, then asked again
        let id = *s.projects[0].pending.iter().find(|(_, w)| matches!(w, crate::Pending::Ext { ext, .. } if *ext == NAME)).unwrap().0;
        s.projects[0].pending.remove(&id);
        reply(&mut s, 0, Box::new(()), Ok(serde_json::to_string(&answer("research", "dev", 1)).unwrap()));
        assert_eq!(asks(&s), 0, "answered");
        poll(&mut s, &mut rune::Tx::default());
        assert_eq!(asks(&s), 0, "not again within EVERY of the answer");
        s.ext.get_mut::<Accounts>().asked = Some(Instant::now() - EVERY - std::time::Duration::from_secs(1));
        poll(&mut s, &mut rune::Tx::default());
        assert_eq!(asks(&s), 1, "the next round");
    }

    #[test]
    fn a_reply_that_is_not_the_json_says_the_coordd_is_older() {
        assert!(parse(Ok("work\tWork\ttier 20\tsigned in".into())).unwrap_err().contains("from before"));
        assert_eq!(parse(Err("coordd: unknown request".into())).unwrap_err(), "coordd: unknown request");
    }

    /// An account's id is made from its label: lower case, `-` between words, a taken one numbered.
    #[test]
    fn an_accounts_id_is_made_from_its_label() {
        assert_eq!(id_for("Work (Max)", &[]), "work-max");
        assert_eq!(id_for("  Ünï  Pro  ", &[]), "n-pro");
        assert_eq!(id_for("work", &["work", "work-2"]), "work-3");
        assert_eq!(id_for("(  )", &[]), "account", "no ASCII letter or digit");
        assert_eq!(id_for("Ünï", &["account"]), "n");
        assert_eq!(id_for("   ", &[]), "", "nothing typed");
        assert!(id_for(&"x".repeat(60), &[]).len() <= 32);
    }

    /// The check (`Check whether it is signed in`, `Check limits`) turns its ring while the ask is out, and while the
    /// accounts asked after its answer are: until coordd's answer, or the pipe's end, which brings none. The accounts'
    /// answer is taken as the poll's.
    #[test]
    fn a_check_turns_its_ring_until_coordd_answers_or_the_pipe_ends() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let i = s.projects.len() - 1;
        let ask = |s: &mut App, n: u64, what: What| drop(s.projects[i].pending.insert(n, crate::Pending::Ext { ext: NAME, data: Box::new(Act("work".into(), what)) }));
        ask(&mut s, 1, What::Tier);
        assert!(!asking(&s, "work", What::Check) && !querying(&s, "work"), "another action turns no ring");
        ask(&mut s, 2, What::Check);
        assert!(asking(&s, "work", What::Check) && !asking(&s, "home", What::Check) && querying(&s, "work") && !querying(&s, "home"));
        s.take(i, crate::link::Incoming::Reply(2, coord_types::Reply { ok: true, out: "signed in".into(), post: None }));
        assert!(!asking(&s, "work", What::Check), "answered");
        ask(&mut s, 3, What::Check);
        s.take(i, crate::link::Incoming::Down("ended".into()));
        assert!(!asking(&s, "work", What::Check), "the pipe ended: no answer comes, and no ring turns");
        // the accounts asked after a check's answer: taken as the poll's
        ask(&mut s, 4, What::Limits);
        assert!(querying(&s, "work"));
        let answer = serde_json::to_string(&NativeAccounts { coord: "r".into(), accounts: vec![NativeAccount { id: "work".into(), label: "Work".into(), ..Default::default() }], ..Default::default() }).unwrap();
        reply(&mut s, i, Box::new(Act("work".into(), What::Limits)), Ok(answer));
        assert!(s.ext.get::<Accounts>().got.get(&i).is_some_and(|g| g.as_ref().is_ok_and(|n| n.accounts[0].id == "work")), "taken as the poll's");
    }

    /// The reset row on made-up answers only: offers he may use, a press under way, one refused with its kind's `Use`
    /// gone; no offer and no press, no row.
    #[test]
    fn the_reset_row_offers_says_applying_and_says_a_refusal() {
        let offer = |kind: &str| ResetOffer { kind: kind.into(), left: Some(1), use_by: Some(1_800_000_000), ..Default::default() };
        let a = NativeAccount { signed_in: true, resets: vec![offer("Fable reset"), offer("full reset")], ..Default::default() };
        let r = resets(&a, false);
        assert_eq!((r.usable.len(), r.applying, r.refused), (2, false, None));
        assert!(resets(&a, true).applying, "pressed, its request not answered yet");
        let tried = |refused: Option<&str>| NativeAccount { reset: Some(coord_types::ResetTry { kind: "Fable reset".into(), since: 1, refused: refused.map(str::to_string), done: None, ..Default::default() }), ..a.clone() };
        assert!(resets(&tried(None), false).applying, "coordd says it applies");
        // refused: coordd leaves that kind out of the offers, and the row says why
        let refused = NativeAccount { resets: vec![offer("full reset")], ..tried(Some("not offered on this plan")) };
        let r = resets(&refused, false);
        assert_eq!(r.usable.iter().map(|o| o.kind.as_str()).collect::<Vec<_>>(), ["full reset"]);
        assert_eq!((r.applying, r.refused, r.done), (false, Some(("Fable reset", "not offered on this plan")), None));
        // a later notice names it again: coordd offers it, and the card shows it though the last press was refused
        let named = NativeAccount { resets: a.resets.clone(), ..refused.clone() };
        assert_eq!(resets(&named, false).usable.len(), 2, "coordd's offers, as given");
        let used = "Fable reset used · your weekly reset day stays Monday · 1 Fable reset left";
        let applied = NativeAccount { resets: Vec::new(), reset: Some(coord_types::ResetTry { done: Some(used.into()), ..tried(None).reset.unwrap() }), ..a.clone() };
        let r = resets(&applied, false);
        assert_eq!((r.usable.len(), r.applying, r.refused, r.done), (0, false, None, Some(used)), "applied: Claude Code's words, nothing under way");
        assert!(!r.is_empty(), "its words are a row");
        assert!(resets(&NativeAccount::default(), false).is_empty());
        // on the card: `Use` only from a coordd that applies it (`ACCOUNT_RESET`), and the words once applied
        let (_held, mut rt) = card_window(NativeAccount { id: "work".into(), label: "Work".into(), ..a.clone() });
        rt.update(|s, _| s.projects[0].protocol = coord_types::ACCOUNT_RESET - 1);
        rt.frame(0.016);
        assert!(!texts(&rt).iter().any(|x| x.starts_with("Use ")), "a coordd before ACCOUNT_RESET: no `Use`: {:?}", texts(&rt));
        rt.update(|s, _| s.projects[0].protocol = coord_types::ACCOUNT_RESET);
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Use Fable reset") && t.iter().any(|x| x == "Use full reset"), "{t:?}");
        drop((_held, rt));
        let (_held, mut rt) = card_window(NativeAccount { id: "work".into(), label: "Work".into(), ..applied });
        rt.update(|s, _| s.projects[0].protocol = coord_types::ACCOUNT_RESET);
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == used) && !t.iter().any(|x| x == "applying…" || x.starts_with("Use ")), "{t:?}");
    }

    /// `Check limits` on a built-in card, on a made-up pipe and answers: beside `Limits not seen yet`; pressed, one check
    /// (`AccountCheck`) is asked and `Querying` stands where the windows are; its answer has the accounts asked again, the
    /// ring on until they come; windows not read are said in the gateway card's words, any other end whole, a refusal
    /// plainly. A coordd whose check reads no windows is offered no press; one not signed in, `Check whether it is signed
    /// in`, the same request.
    #[test]
    fn check_limits_is_the_one_check_and_rings_until_its_answer_and_the_accounts_come() {
        let work = NativeAccount { id: "work".into(), label: "Work".into(), signed_in: true, ..Default::default() };
        let (_held, mut rt) = card_window(work.clone());
        rt.update(|s, _| s.projects[0].protocol = coord_types::ACCOUNT_RESET - 1);
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Limits not seen yet") && !t.iter().any(|x| x == "Check limits"), "a coordd before ACCOUNT_RESET: {t:?}");
        drop((_held, rt));
        let out = std::env::temp_dir().join(format!("yaaihh-accounts-check-{}", std::process::id()));
        let (_held, mut rt) = card_window(work.clone());
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        rt.update(move |s, _| {
            s.projects[0].protocol = coord_types::ACCOUNT_RESET;
            s.projects[0].link = Some(crate::link::Link::to_child(stdin));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "Check limits"), "{:?}", texts(&rt));
        press(&mut rt, "check-limits-work");
        let t = texts(&rt);
        assert_eq!(t.iter().filter(|x| *x == "Querying").count(), 1, "one ring: {t:?}");
        assert!(!t.iter().any(|x| x == "Limits not seen yet" || x == "asking the engine…"), "{t:?}");
        // checked: the ring turns on while the accounts are asked again
        let read = "account work: signed in · 5-hour 17% until 2026-11-20T15:00:00Z · weekly not said";
        rt.update(move |s, _| {
            s.take(0, crate::link::Incoming::Reply(1, coord_types::Reply { ok: true, out: read.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Check)), Ok(read.into()));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "Querying"), "until the accounts come: {:?}", texts(&rt));
        let seen = NativeAccount { five_hour: Some(LimitWindow { used_percent: 17.0, resets_at: now_s() + 3600 }), limits_from: "usage".into(), ..work.clone() };
        let answer = serde_json::to_string(&NativeAccounts { coord: "r".into(), accounts: vec![seen], ..Default::default() }).unwrap();
        rt.update(move |s, _| {
            s.take(0, crate::link::Incoming::Reply(2, coord_types::Reply { ok: true, out: answer.clone(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Limits)), Ok(answer));
        });
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "5-hour") && t.iter().any(|x| x == "83% remaining") && !t.iter().any(|x| x == "Querying"), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("account work")), "the windows are drawn, not said again in coordd's words: {t:?}");
        // windows not read, then refused: each said on the card
        rt.update(|s, _| {
            check(s, "work");
            let unread = "account work: signed in · its windows not read: Claude Code's /usage said neither window";
            s.take(0, crate::link::Incoming::Reply(3, coord_types::Reply { ok: true, out: unread.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Check)), Ok(unread.into()));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "Could not read its limits: Claude Code's /usage said neither window"), "{:?}", texts(&rt));
        rt.update(|s, _| {
            check(s, "work");
            s.take(0, crate::link::Incoming::Reply(5, coord_types::Reply { ok: false, out: String::new(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Check)), Err("coord account check: Claude Code did not answer".into()));
        });
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Claude Code did not answer") && !t.iter().any(|x| x.starts_with("Could not read")), "{t:?}");
        // an end that is neither the windows nor why they were not read: said whole, never dropped
        rt.update(|s, _| {
            let other = "account work: signed in · its usage page moved";
            check(s, "work");
            s.take(0, crate::link::Incoming::Reply(6, coord_types::Reply { ok: true, out: other.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Check)), Ok(other.into()));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "its usage page moved"), "{:?}", texts(&rt));
        // not signed in: the check by its other name, the same request, its words said as before
        rt.update(|s, _| {
            let none = "account work: not signed in";
            check(s, "work");
            s.take(0, crate::link::Incoming::Reply(8, coord_types::Reply { ok: true, out: none.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Check)), Ok(none.into()));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "account work: not signed in"), "{:?}", texts(&rt));
        rt.update(|s, _| s.projects[0].link = None);
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let ops: Vec<(&str, &str)> = sent.iter().map(|v| (v["req"]["op"].as_str().unwrap_or_default(), v["req"]["id"].as_str().unwrap_or_default())).collect();
        let (checked, asked) = (("accountcheck", "work"), ("accounts", ""));
        assert_eq!(ops, [checked, asked, checked, asked, checked, checked, asked, checked, asked], "each check, and the accounts after each answer it took");
    }

    /// A made-up agent of the registry: NAME, id ID, in STATE.
    fn agent(name: &str, id: &str, state: &str) -> AgentView {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "parent": "", "kind": "", "model": "", "state": state, "since": "2026-11-20T08:00:00Z",
            "last_tool": null, "tools": [], "items": [], "transcript": null, "mem_mb": null, "cpu_s": null
        }))
        .unwrap()
    }

    /// A made-up agent moved at 08:40 from `team-seat` (0.2 D21), on the Agents table and the lists.
    fn moved_agent() -> AgentView {
        AgentView { native: true, account_since: Some("2026-11-20T08:40:00Z".into()), account_from: Some("team-seat".into()), account_why: Some("its account does not take messages".into()), ..agent("alpha", "a1", "idle") }
    }

    /// The Account cell (0.2 D21) on made-up agents: where it runs, `waiting` with until when from `account_until`
    /// (never from the note's words), `Gateway`, nothing; the hover its last move, each part left out when coordd said
    /// none, else coordd's note. No extension is registered in a test, so an account reads by its id, as it does
    /// before the accounts are asked.
    #[test]
    fn an_agents_account_cell_says_where_it_runs_and_why() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        let since = crate::when::at("2026-11-20T08:40:00Z");
        let on = AgentView { account: Some("work".into()), ..moved_agent() };
        assert_eq!(account_cell(&s, &on), ("work".into(), Some(format!("since {since} · from team-seat: its account does not take messages"))));
        let wait = |note: &str, until: Option<i64>| AgentView { native: true, account_note: Some(note.into()), account_until: until, account_waiting: true, ..agent("beta", "b2", "stopped") };
        let until = "not running; it is started when an account can take it (until 14:00)";
        let back = 1_800_000_000;
        assert_eq!(account_cell(&s, &wait(until, Some(back))), (format!("waiting · until {}", crate::when::secs(back)), Some(until.into())));
        assert_eq!(account_cell(&s, &wait(until, None)).0, "waiting", "no time given: the note's words are not read for one");
        let unknown = "not running; it is started when an account can take it (with no reset time known)";
        assert_eq!(account_cell(&s, &wait(unknown, None)).0, "waiting");
        assert_eq!(account_cell(&s, &wait("not running: no account can take it; it is started when you sign in to one", None)).0, "waiting");
        let unsaid = AgentView { account_waiting: false, ..wait(until, Some(back)) };
        assert_eq!(account_cell(&s, &unsaid), (String::new(), Some(until.into())), "waiting is coordd's field, not read out of its note");
        let through = "through the gateway while no account can take it (until 14:00)";
        let gw = AgentView { native: true, account_note: Some(through.into()), ..agent("gamma", "c3", "working") };
        assert_eq!(account_cell(&s, &gw), ("Gateway".into(), Some(through.into())));
        assert_eq!(account_cell(&s, &AgentView { native: true, ..agent("delta", "d4", "stopped") }), (String::new(), None), "stopped");
        assert_eq!(account_cell(&s, &agent("eps", "e5", "working")), (String::new(), None), "the project's model route");
        // a part coordd did not give is left out
        let why_only = AgentView { account_since: None, account_from: None, ..moved_agent() };
        assert_eq!(moved(&s, &why_only).as_deref(), Some("its account does not take messages"));
        let from_only = AgentView { account_since: None, account_why: None, ..moved_agent() };
        assert_eq!(moved(&s, &from_only).as_deref(), Some("from team-seat"));
        assert_eq!(moved(&s, &agent("zeta", "f6", "idle")), None, "never moved");
    }

    /// The gateway's line and an account's line, for the window the list tests draw.
    fn lists(s: &App, _: &mut rune::Cx<App>) -> El {
        let k = Kit::neutral(false);
        let st = s.ext.get::<Accounts>();
        let a = st.got.values().find_map(|r| r.as_ref().ok()?.accounts.first()).expect("a made-up account");
        let agents: Vec<Listed> = a.agents.iter().map(|x| listed(s, x)).collect();
        col().w_full().children(gateway_agents(s, &k)).child(agents_fold(s, &k, &a.id, "Agents", false, &agents))
    }

    /// The texts the window RT shows; not those of a list closing, which stay listed while their exit plays.
    fn texts(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text.or(e.label)).collect()
    }

    /// Press the element keyed KEY, as his pointer does.
    fn press(rt: &mut rune::runtime::Runtime<App>, key: &str) {
        let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is drawn: {:?}", texts(rt)));
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        rt.frame(0.016);
    }

    /// A sign-in's box, in the sheet and on a card, for Claude Code's sign-in and agy's Google one: its link and
    /// `Copy link`, nothing that opens his browser; nor does its link coming open it: a [`SignIn`] has no way to.
    #[test]
    fn a_sign_in_shows_its_link_to_copy_and_opens_nothing() {
        fn one(s: &App, _: &mut rune::Cx<App>) -> El {
            let st = s.ext.get::<Accounts>();
            signing_in(&Kit::neutral(false), "work", &st.signin["work"], st.add.open)
        }
        for (whose, url) in [(Whose::Claude, "https://claude.ai/oauth/authorize?x=1"), (Whose::Agy, "https://accounts.google.com/o/oauth2/v2/auth?client_id=x")] {
            for sheet in [true, false] {
                let (_held, mut s) = crate::view_cost::test_app();
                s.ext.insert(Accounts::default());
                s.ext.get_mut::<Accounts>().add.open = sheet;
                s.ext.get_mut::<Accounts>().signin.insert("work".into(), SignIn { url: Some(url.into()), ..SignIn::new(whose) });
                let mut rt = rune::runtime::Runtime::new(s, one, (700.0, 500.0), rune::Env::new(false, 1.0));
                rt.frame(0.016);
                let t = texts(&rt);
                assert!(t.iter().any(|x| x == "Copy link") && t.iter().any(|x| x == url), "{t:?}");
                assert!(!t.iter().any(|x| x.contains("Open") || x.contains("opens")), "{t:?}");
            }
        }
    }

    /// `Through the gateway  2 agents ▸` and an account's `Agents  1 ▸` (0.2 D21; the seat listed 35 names joined), on
    /// made-up names: closed, the count alone; pressed, each by name and id with since when and why, the other list
    /// still closed. A stopped agent is on neither; an agent of a coord not open here has no id.
    #[test]
    fn the_agents_lists_open_on_a_press_with_each_by_name_and_id() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let i = s.projects.len() - 1;
        s.projects[i].live.agents = vec![AgentView { account: Some("work".into()), ..moved_agent() }, agent("beta", "b2", "working"), agent("gamma", "c3", "stopped")];
        let bound = |coord: &str, agent: &str| NativeAgent { coord: coord.into(), agent: agent.into(), since: "2026-11-20T08:40:00Z".into(), note: None };
        let work = NativeAccount { id: "work".into(), label: "Work".into(), agents: vec![bound("r", "alpha")], on_gateway: vec![bound("other", "omega")], ..Default::default() };
        s.ext.get_mut::<Accounts>().got.insert(i, Ok(NativeAccounts { coord: "r".into(), gateway: true, accounts: vec![work], ..Default::default() }));
        let mut rt = rune::runtime::Runtime::new(s, lists, (900.0, 700.0), rune::Env::new(false, 1.0));
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Through the gateway") && t.iter().any(|x| x == "2 agents ▸") && t.iter().any(|x| x == "1 ▸"), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("beta") || x.contains("alpha")), "closed: the count alone: {t:?}");
        press(&mut rt, &format!("agents-fold-{GATEWAY}"));
        let t = texts(&rt);
        let since = crate::when::at("2026-11-20T08:40:00Z");
        assert!(t.iter().any(|x| x == "2 agents ▾") && t.iter().any(|x| x == "r.beta (b2)") && t.iter().any(|x| x == "other.omega"), "{t:?}");
        assert!(t.iter().any(|x| *x == format!("since {since}")), "the coord not open here: since when it is there: {t:?}");
        assert!(!t.iter().any(|x| x.contains("gamma")), "stopped: {t:?}");
        assert!(t.iter().any(|x| x == "1 ▸") && !t.iter().any(|x| x == "r.alpha (a1)"), "the account's list stays closed: {t:?}");
        press(&mut rt, "agents-fold-work");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "r.alpha (a1)") && t.iter().any(|x| *x == format!("since {since} · from team-seat: its account does not take messages")), "{t:?}");
        press(&mut rt, &format!("agents-fold-{GATEWAY}"));
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "2 agents ▸") && !t.iter().any(|x| x == "r.beta (b2)"), "closed again: {t:?}");
    }

    /// On made-up names: an account's `Agents` holding a stopped agent says how many run (`2 · 1 running`); opened, the
    /// stopped one is under the running one. The opencode agent that runs is not counted through the gateway.
    #[test]
    fn an_accounts_agents_say_how_many_run_and_list_a_stopped_one_last() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        s.projects.push(crate::Proj::new(&crate::projects::ProjectConfig { name: "r".into(), id: "r".into(), boxed: false }, Vec::new()));
        let i = s.projects.len() - 1;
        let oc = |name: &str, id: &str, state: &str| AgentView { kind: "opencode".into(), ..agent(name, id, state) };
        s.projects[i].live.agents = vec![oc("oc1", "a1", "stopped"), oc("oc2", "a2", "idle")];
        let bound = |agent: &str| NativeAgent { coord: "r".into(), agent: agent.into(), since: "2026-11-22T15:32:00Z".into(), note: None };
        let go = NativeAccount { id: "go".into(), label: "Go".into(), agents: vec![bound("oc1"), bound("oc2")], ..Default::default() };
        s.ext.get_mut::<Accounts>().got.insert(i, Ok(NativeAccounts { coord: "r".into(), gateway: true, accounts: vec![go], ..Default::default() }));
        let mut rt = rune::runtime::Runtime::new(s, lists, (900.0, 700.0), rune::Env::new(false, 1.0));
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "2 · 1 running ▸"), "{t:?}");
        assert!(t.iter().any(|x| x == "Through the gateway") && t.iter().any(|x| x == "no agent"), "an opencode agent is not through the gateway: {t:?}");
        press(&mut rt, "agents-fold-go");
        let t = texts(&rt);
        let at = |name: &str| t.iter().position(|x| x == name).unwrap_or_else(|| panic!("{name}: {t:?}"));
        assert!(at("r.oc2 (a2)") < at("r.oc1 (a1)"), "the stopped one under the running one: {t:?}");
        let since = crate::when::at("2026-11-22T15:32:00Z");
        assert_eq!(t[at("r.oc1 (a1)") + 1], format!("stopped · since {since}"), "{t:?}");
        assert_eq!(t[at("r.oc2 (a2)") + 1], format!("since {since}"), "{t:?}");
    }

    /// The first account of the made-up answer, as its card, with its `⋯` menu's entries when it is open.
    fn one_card(s: &App, _: &mut rune::Cx<App>) -> El {
        let a = s.ext.get::<Accounts>().got.values().find_map(|r| r.as_ref().ok()?.accounts.first()).expect("a made-up account");
        let mut layer = Vec::new();
        let card = native_card(s, &Kit::neutral(false), a, "", now_s(), None, &mut layer);
        col().child(card).children(layer)
    }

    /// Press the words WORDS where they are drawn (a menu's entry, keyed by none of ours), as his pointer does.
    fn tap(rt: &mut rune::runtime::Runtime<App>, words: &str) {
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.text.as_deref() == Some(words)).unwrap_or_else(|| panic!("{words} is drawn: {:?}", texts(rt))).rect;
        let (x, y) = (r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
        rt.pointer_move(x, y);
        rt.frame(0.016);
        rt.pointer_down(x, y);
        rt.frame(0.016);
        rt.pointer_up(x, y);
        rt.frame(0.016);
        rt.frame(0.016);
    }

    /// `Rename` and `Turn off` · `Turn on` on a card of his own, on a made-up pipe and answers: none from a coordd before
    /// `ACCOUNT_EDIT`; from it, in the `⋯`. Rename opens a field with its name and Save sends one `AccountEdit` with the
    /// label alone, the field closing on coordd's yes. Turn off sends `off`; an account that is off says so once, with
    /// `Turn on` beside; a refusal is said on the card. Its weeks of one model stand under its two windows.
    #[test]
    fn rename_and_turn_off_are_one_edit_and_an_off_card_says_so_with_turn_on() {
        let week = |used: f64, secs: i64| LimitWindow { used_percent: used, resets_at: now_s() + secs };
        let fable = coord_types::ModelWeek { model: "Fable".into(), window: week(65.0, 86_400) };
        let work = NativeAccount { harness: "claude".into(), id: "work".into(), label: "Work".into(), signed_in: true, five_hour: Some(week(62.0, 3600)), seven_day: Some(week(31.0, 90_000)), model_weeks: vec![fable], ..Default::default() };
        let (_held, mut rt) = card_window(work.clone());
        let t = texts(&rt);
        assert!(["5-hour", "Weekly", "Weekly (Fable)", "35% remaining"].iter().all(|w| t.iter().any(|x| x == w)), "its own two, then its Fable week: {t:?}");
        // a coordd before ACCOUNT_EDIT: the ⋯ has no Rename and no Turn off
        rt.update(|s, _| {
            s.projects[0].protocol = coord_types::ACCOUNT_EDIT - 1;
            s.ext.get_mut::<Accounts>().menu = Some("claude-work".into());
        });
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Check limits") && !t.iter().any(|x| x == "Rename" || x == "Turn off"), "{t:?}");
        let out = std::env::temp_dir().join(format!("yaaihh-accounts-edit-{}", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        rt.update(move |s, _| {
            s.projects[0].protocol = coord_types::ACCOUNT_EDIT;
            s.projects[0].link = Some(crate::link::Link::to_child(stdin));
        });
        rt.frame(0.016);
        // Rename: a field with its name; the name typed is sent, and the field goes on coordd's yes
        tap(&mut rt, "Rename");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Save") && t.iter().any(|x| x == "Cancel"), "{t:?}");
        let mut holds = None;
        rt.update(|s, _| holds = s.ext.get::<Accounts>().naming.get("work").map(|f| f.text.clone()));
        assert_eq!(holds.as_deref(), Some("Work"), "it holds its name");
        rt.update(|s, _| drop(s.ext.get_mut::<Accounts>().naming.insert("work".into(), TextEdit::new("Work (Max)"))));
        rt.frame(0.016);
        tap(&mut rt, "Save");
        rt.update(|s, _| {
            let yes = "account work: label “Work (Max)”";
            s.take(0, crate::link::Incoming::Reply(1, coord_types::Reply { ok: true, out: yes.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Edit)), Ok(yes.into()));
        });
        rt.frame(0.016);
        assert!(!texts(&rt).iter().any(|x| x == "Save"), "closed on its yes: {:?}", texts(&rt));
        // Turn off: `off` alone; the card that is off says so once, with Turn on
        rt.update(|s, _| s.ext.get_mut::<Accounts>().menu = Some("claude-work".into()));
        rt.frame(0.016);
        tap(&mut rt, "Turn off");
        let off = NativeAccount { off: Some(now_s()), ..work.clone() };
        rt.update(move |s, _| {
            let yes = "account work: off: it takes no agent; its agents move at the end of their turn";
            s.take(0, crate::link::Incoming::Reply(2, coord_types::Reply { ok: true, out: yes.into(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Edit)), Ok(yes.into()));
            s.ext.get_mut::<Accounts>().got.insert(0, Ok(NativeAccounts { coord: "r".into(), accounts: vec![off], ..Default::default() }));
        });
        rt.frame(0.016);
        let t = texts(&rt);
        assert_eq!(t.iter().filter(|x| *x == "Off · takes no agents").count(), 1, "{t:?}");
        assert!(t.iter().any(|x| x == "Turn on") && !t.iter().any(|x| x.contains("account work")), "its state said, not coordd's words: {t:?}");
        // Turn on, refused: said on the card
        tap(&mut rt, "Turn on");
        rt.update(|s, _| {
            s.take(0, crate::link::Incoming::Reply(3, coord_types::Reply { ok: false, out: String::new(), post: None }));
            reply(s, 0, Box::new(Act("work".into(), What::Edit)), Err("coord account edit: no account work".into()));
        });
        rt.frame(0.016);
        assert!(texts(&rt).iter().any(|x| x == "no account work"), "{:?}", texts(&rt));
        rt.update(|s, _| s.projects[0].link = None);
        let _ = child.wait();
        let sent: Vec<serde_json::Value> = std::fs::read_to_string(&out).unwrap().lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let _ = std::fs::remove_file(&out);
        let edits: Vec<(&str, &str, Option<&str>, Option<bool>)> = sent.iter().map(|v| (v["req"]["op"].as_str().unwrap_or_default(), v["req"]["id"].as_str().unwrap_or_default(), v["req"]["label"].as_str(), v["req"]["off"].as_bool())).collect();
        assert_eq!(edits, [("accountedit", "work", Some("Work (Max)"), None), ("accountedit", "work", None, Some(true)), ("accountedit", "work", None, Some(false))], "one edit each, the rest left as it was");
        assert!(sent.iter().all(|v| v["req"].get("tier").is_none()), "no tier sent: {sent:?}");
    }

    /// On made-up accounts: windows from a session's status line in the last ten minutes are live, with no time; any others
    /// say when they were read, with `Check limits` beside. A window past its reset with nothing read since is said reset.
    #[test]
    fn windows_not_live_say_when_they_were_read_and_one_past_its_reset_says_so() {
        let w = |used: f64, secs: i64| LimitWindow { used_percent: used, resets_at: now_s() + secs };
        let work = NativeAccount { harness: "claude".into(), id: "work".into(), label: "Work".into(), signed_in: true, five_hour: Some(w(40.0, 3600)), seven_day: Some(w(20.0, 90_000)), limits_from: "statusline".into(), limits_seen: now_s() - 60, ..Default::default() };
        let (_held, rt) = card_window(work.clone());
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "60% remaining") && !t.iter().any(|x| x.starts_with("as of")), "live: {t:?}");
        drop((_held, rt));
        let read = now_s() - 7200;
        let (_held, rt) = card_window(NativeAccount { limits_from: "usage".into(), limits_seen: read, five_hour: Some(w(90.0, -60)), ..work });
        let t = texts(&rt);
        assert!(t.iter().any(|x| *x == format!("as of {}", crate::when::secs(read))) && t.iter().any(|x| x == "Check limits"), "{t:?}");
        assert!(t.iter().any(|x| x == "reset, not seen since") && !t.iter().any(|x| x == "10% remaining"), "{t:?}");
        assert!(t.iter().any(|x| x == "80% remaining"), "the week still runs: {t:?}");
    }

    /// The even pace on each running window's meter, on made-up accounts: a red mark at what would remain had the window
    /// been used evenly. A 5-hour window 2 h before its reset would have 40% left: with 20% left it is ahead of the pace; a
    /// week 5 days before its reset would have 71%: with 90% it is behind. An agy window not begun has no mark, nor one
    /// past its reset; Go's month runs a calendar month back from its reset.
    #[test]
    fn each_running_window_marks_where_an_even_pace_would_stand() {
        let now = now_s();
        let w = |used: f64, secs: i64| LimitWindow { used_percent: used, resets_at: now + secs };
        let work = NativeAccount {
            harness: "claude".into(),
            id: "work".into(),
            label: "Work".into(),
            signed_in: true,
            five_hour: Some(w(80.0, 2 * 3600)),
            seven_day: Some(w(10.0, 5 * 86_400)),
            model_weeks: vec![coord_types::ModelWeek { model: "Fable".into(), window: w(50.0, 5 * 86_400) }],
            limits_from: "statusline".into(),
            limits_seen: now - 60,
            ..Default::default()
        };
        let (held, rt) = card_window(work);
        let els = rt.elements();
        let rect = |label: &str| els.iter().find(|e| !e.exiting && e.label.as_deref() == Some(label)).map(|e| e.rect);
        let labels: Vec<String> = els.iter().filter_map(|e| e.label.clone()).collect();
        let (bar, mark) = (rect("5-hour").expect("the 5-hour meter"), rect("5-hour at an even pace: 40% by now").unwrap_or_else(|| panic!("{labels:?}")));
        assert!(((mark[0] - bar[0]) / bar[2] - 0.4).abs() < 0.02, "the mark at 40% of the meter: {mark:?} on {bar:?}");
        let fill = els.iter().find(|e| e.parent.is_some_and(|p| els[p].label.as_deref() == Some("5-hour")) && e.label.is_none()).map(|e| e.rect).expect("its fill");
        assert!(fill[0] + fill[2] < mark[0], "20% left, short of the mark: ahead of the pace: {fill:?} {mark:?}");
        let week = rect("Weekly").expect("the week's meter");
        let week_mark = rect("Weekly at an even pace: 71% by now").unwrap_or_else(|| panic!("{labels:?}"));
        assert!(((week_mark[0] - week[0]) / week[2] - 5.0 / 7.0).abs() < 0.02, "{week_mark:?} on {week:?}");
        assert!(labels.iter().any(|l| l == "Weekly (Fable) at an even pace: 71% by now"), "a model's week too: {labels:?}");
        assert!(mark[3] > bar[3], "the mark stands out of the meter: {mark:?} {bar:?}");
        drop((held, rt));
        let none = coord_types::NOT_STARTED;
        let week = |label: &str, used: f64, resets_at: i64| coord_types::ModelWeek { model: label.into(), window: LimitWindow { used_percent: used, resets_at } };
        let agy = NativeAccount {
            id: "personal".into(),
            label: "Personal".into(),
            harness: "agy".into(),
            signed_in: true,
            model_weeks: vec![week("Gemini · 5-hour", 25.0, now + 3600), week("Gemini · Weekly", 0.0, none), week("Claude & GPT (shared) · 5-hour", 60.0, now - 60)],
            model_weeks_seen: Some(now - 60),
            limits_seen: now - 60,
            limits_from: "t3code".into(),
            ..Default::default()
        };
        let (held, rt) = card_window(agy);
        let paced: Vec<String> = rt.elements().into_iter().filter_map(|e| e.label).filter(|l| l.contains("even pace")).collect();
        assert_eq!(paced, ["Gemini · 5-hour at an even pace: 20% by now"], "not begun, and past its reset: no mark");
        drop((held, rt));
        let at = |s: &str| chrono::DateTime::parse_from_rfc3339(s).unwrap().timestamp();
        assert_eq!((Span::named("5-hour"), Span::named("Weekly"), Span::named("Monthly"), Span::named("Weekly (Fable)")), (Some(Span::Secs(5 * 3600)), Some(Span::WEEK), Some(Span::Month), None));
        assert_eq!(Span::Month.began(at("2026-12-01T00:00:00Z")), Some(at("2026-11-01T00:00:00Z")));
        let month = LimitWindow { used_percent: 70.0, resets_at: at("2026-12-01T00:00:00Z") };
        assert_eq!(pace(&month, Span::Month, at("2026-11-16T00:00:00Z")), Some(50.0), "half of November left");
    }

    /// On made-up answers: the resets Claude Code lists at rest are drawn with their counts and when they were listed, with
    /// no `Use`; at a limit, `Use` beside the same lines. On a live card the weeks of one model say when they were read.
    #[test]
    fn held_resets_are_listed_at_rest_with_their_time_and_no_use() {
        let w = |used: f64, secs: i64| LimitWindow { used_percent: used, resets_at: now_s() + secs };
        let fable = ResetOffer { kind: "Fable reset".into(), left: Some(2), use_by: Some(now_s() + 7 * 86_400), ..Default::default() };
        let (listed, weeks_read) = (now_s() - 3600, now_s() - 7200);
        let work = NativeAccount {
            harness: "claude".into(),
            id: "work".into(),
            label: "Work".into(),
            signed_in: true,
            five_hour: Some(w(40.0, 3600)),
            seven_day: Some(w(20.0, 90_000)),
            limits_from: "statusline".into(),
            limits_seen: now_s() - 60,
            model_weeks: vec![coord_types::ModelWeek { model: "Fable".into(), window: w(50.0, 90_000) }],
            model_weeks_seen: Some(weeks_read),
            held: vec![fable.clone()],
            held_seen: Some(listed),
            ..Default::default()
        };
        let (_held, rt) = card_window(work.clone());
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Fable reset") && t.iter().any(|x| x.starts_with("2\u{a0}left")), "{t:?}");
        assert!(t.iter().any(|x| *x == format!("as of {}", crate::when::secs(listed))) && !t.iter().any(|x| x.starts_with("Use ") && x.contains("reset")), "listed at rest, no `Use` of a reset: {t:?}");
        assert!(t.iter().any(|x| x == "Offered at a limit"), "where its Use comes: {t:?}");
        assert!(t.iter().any(|x| *x == format!("Weekly (Fable) as of {}", crate::when::secs(weeks_read))), "live, its Fable week read before: {t:?}");
        drop((_held, rt));
        let (_held, rt) = card_window(NativeAccount { resets: vec![fable], ..work });
        assert!(texts(&rt).iter().any(|x| x == "Use Fable reset"), "at a limit: {:?}", texts(&rt));
    }

    /// On made-up answers: the plan in the kind line, extra usage while `/usage` says it, and a limit with its time known
    /// can be cleared from the `⋯`; one with none known keeps `It can take requests` in sight.
    #[test]
    fn the_card_says_its_plan_and_extra_usage_and_a_timed_limit_can_be_cleared() {
        let now = now_s();
        let work = NativeAccount { harness: "claude".into(), id: "work".into(), label: "Work".into(), signed_in: true, plan: Some("max".into()), ..Default::default() };
        let (_held, mut rt) = card_window(NativeAccount { out_until: Some(now + 3600), out_for: Some("rate_limit".into()), extra_usage_until: Some(now + 3600), ..work.clone() });
        rt.update(|s, _| s.ext.get_mut::<Accounts>().menu = Some("claude-work".into()));
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "subscription · Plan Max"), "{t:?}");
        assert!(t.iter().any(|x| x.starts_with("On extra usage until ")) && t.iter().any(|x| x.starts_with("Out until ")), "{t:?}");
        assert!(t.iter().any(|x| x == "It can take requests"), "a timed limit, cleared from the ⋯: {t:?}");
        drop((_held, rt));
        // no plan said, extra usage past, at no limit: none of these
        let (_held, mut rt) = card_window(NativeAccount { plan: None, extra_usage_until: Some(now - 1), ..work });
        rt.update(|s, _| s.ext.get_mut::<Accounts>().menu = Some("claude-work".into()));
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "subscription") && !t.iter().any(|x| x.starts_with("On extra usage") || x == "It can take requests"), "{t:?}");
    }

    /// On made-up answers only: each kind's grant line says what it gives back and why it cannot be used at this limit; that
    /// kind's `Use` is dimmed and opens no confirm; a press Claude Code could not confirm is said in its words and offers
    /// `Try … again`; one refused for its login puts the card's `Sign in` in sight.
    #[test]
    fn a_reset_says_what_it_gives_back_and_why_not_and_one_not_confirmed_can_be_tried_again() {
        let by = now_s() + 7 * 86_400;
        let fable = ResetOffer { kind: "Fable reset".into(), left: Some(2), use_by: Some(by), gives: Some("Refills your Fable limit now · returns what Fable used to your weekly limit".into()), ..Default::default() };
        let full = ResetOffer { kind: "full reset".into(), why: Some("can't be used while you're at your 5-hour limit".into()), ..Default::default() };
        let a = NativeAccount { harness: "claude".into(), id: "work".into(), label: "Work".into(), signed_in: true, resets: vec![fable.clone(), full.clone()], held: vec![fable.clone(), full.clone()], held_seen: Some(now_s()), ..Default::default() };
        let (_held, mut rt) = card_window(a.clone());
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.contains("use\u{a0}by") && x.ends_with("— Refills your Fable limit now · returns what Fable used to your weekly limit")), "{t:?}");
        assert!(t.iter().any(|x| x == "can't be used while you're at your 5-hour limit") && t.iter().any(|x| x == "Use full reset") && t.iter().any(|x| x == "Use Fable reset"), "{t:?}");
        assert!(!t.iter().any(|x| x == "Offered at a limit"), "at a limit: {t:?}");
        tap(&mut rt, "Use full reset");
        assert!(rt.state.ext.get::<Accounts>().confirm_reset.is_none(), "dimmed: no confirm");
        tap(&mut rt, "Use Fable reset");
        assert_eq!(rt.state.ext.get::<Accounts>().confirm_reset, Some(("work".into(), "Fable reset".into())));
        drop((_held, rt));
        let unconfirmed = "Couldn't confirm the reset went through · if you're still at the limit in a moment, try again";
        let tried = coord_types::ResetTry { kind: "Fable reset".into(), since: 1, refused: Some(unconfirmed.into()), again: true, ..Default::default() };
        let (_held, rt) = card_window(NativeAccount { reset: Some(tried.clone()), ..a.clone() });
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Try Fable reset again") && t.iter().any(|x| *x == format!("Fable reset: {unconfirmed}")), "{t:?}");
        assert!(!t.iter().any(|x| x.contains("refused")), "{t:?}");
        drop((_held, rt));
        // refused by Claude Code: said so, and coordd no longer offers that kind
        let refused = coord_types::ResetTry { refused: Some("No Fable reset to use".into()), again: false, ..tried.clone() };
        let (_held, rt) = card_window(NativeAccount { reset: Some(refused), resets: vec![full.clone()], ..a.clone() });
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Fable reset refused: No Fable reset to use") && !t.iter().any(|x| x.starts_with("Try ")), "{t:?}");
        assert!(!t.iter().any(|x| x == "Sign in"), "a refusal not for its login asks for no sign-in: {t:?}");
        drop((_held, rt));
        // refused for its login: no `Try again`, which would meet the same; the card's own Sign in in sight
        let login = "Couldn't reset your limits with this login · run /login, then try again";
        let relogin = coord_types::ResetTry { refused: Some(login.into()), again: false, login: true, ..tried };
        let (_held, rt) = card_window(NativeAccount { reset: Some(relogin), resets: vec![full], ..a });
        let t = texts(&rt);
        assert!(t.iter().any(|x| *x == format!("Fable reset refused: {login}")) && !t.iter().any(|x| x.starts_with("Try ")), "{t:?}");
        assert!(t.iter().any(|x| x == "Sign in"), "{t:?}");
    }

    /// The page's `Check limits` checks every card that offers it, four at a time, the rest as answers come, on a made-up
    /// pipe; a card not signed in, or a key's, is not checked. The pipe's end mid-run ends the run: the bar does not stay
    /// on `Checking…`, and nothing waiting goes out later.
    #[test]
    fn check_limits_on_the_page_checks_every_signed_in_account_four_at_a_time() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.projects[0].protocol = coord_types::PROTOCOL;
        let out = std::env::temp_dir().join(format!("yaaihh-accounts-check-all-{}", std::process::id()));
        let mut child = std::process::Command::new("sh").arg("-c").arg(format!("cat > '{}'", out.display())).stdin(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        let account = |id: &str| NativeAccount { harness: "claude".into(), id: id.into(), label: id.into(), signed_in: true, ..Default::default() };
        let mut accounts: Vec<NativeAccount> = (1..=6).map(|i| account(&format!("a{i}"))).collect();
        accounts.push(NativeAccount { signed_in: false, ..account("out") });
        accounts.push(NativeAccount { key: true, key_since: Some(1), ..account("key") });
        s.ext.insert(Accounts::default());
        s.ext.get_mut::<Accounts>().got.insert(0, Ok(NativeAccounts { coord: "r".into(), accounts, ..Default::default() }));
        let checks = |s: &App| -> Vec<(u64, String)> {
            let mut v: Vec<(u64, String)> = s.projects[0]
                .pending
                .iter()
                .filter_map(|(id, w)| match w {
                    crate::Pending::Ext { data, .. } => data.downcast_ref::<Act>().filter(|a| a.1 == What::Check).map(|a| (*id, a.0.clone())),
                    _ => None,
                })
                .collect();
            v.sort();
            v
        };
        check_all(&mut s);
        let out_now = checks(&s);
        assert_eq!(out_now.iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(), ["a1", "a2", "a3", "a4"]);
        assert_eq!(s.ext.get::<Accounts>().checking.iter().map(String::as_str).collect::<Vec<_>>(), ["a5", "a6"]);
        // one answered: the next goes
        let (id, first) = out_now[0].clone();
        s.take(0, crate::link::Incoming::Reply(id, coord_types::Reply { ok: true, out: format!("account {first}: signed in"), post: None }));
        reply(&mut s, 0, Box::new(Act(first.clone(), What::Check)), Ok(format!("account {first}: signed in")));
        assert_eq!(checks(&s).iter().map(|(_, id)| id.as_str()).collect::<Vec<_>>(), ["a2", "a3", "a4", "a5"]);
        assert_eq!(s.ext.get::<Accounts>().checking.len(), 1);
        assert!(!checks_lost(&mut s), "checks out: the run goes on");
        // the pipe ends with four out (an install restarts coordd): none is answered, the one waiting is dropped, and the bar
        // says `Check limits` again
        s.take(0, crate::link::Incoming::Down("made-up end".into()));
        assert!(checks(&s).is_empty());
        assert!(checks_lost(&mut s));
        assert!(s.ext.get::<Accounts>().checking.is_empty() && (1..=6).all(|i| !querying(&s, &format!("a{i}"))));
        // a later answer, as his press on one card brings: nothing queued goes out with it
        reply(&mut s, 0, Box::new(Act("a2".into(), What::Check)), Ok("account a2: signed in".into()));
        assert!(checks(&s).is_empty() && !checks_lost(&mut s));
        drop(child.kill());
        let _ = std::fs::remove_file(&out);
    }

    /// A window drawing account A's card alone, on a made-up answer from a coordd of this build (`PROTOCOL`), with no
    /// pipe.
    fn card_window(a: NativeAccount) -> (std::sync::MutexGuard<'static, ()>, rune::runtime::Runtime<App>) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.projects[0].protocol = coord_types::PROTOCOL;
        s.ext.insert(Accounts::default());
        s.ext.get_mut::<Accounts>().got.insert(0, Ok(NativeAccounts { coord: "r".into(), accounts: vec![a], ..Default::default() }));
        let mut rt = rune::runtime::Runtime::new(s, one_card, (900.0, 900.0), rune::Env::new(false, 1.0));
        rt.frame(0.016);
        (held, rt)
    }

    /// A deaf account's card (0.2 D22 D), on a made-up answer: the one line; a press opens why, where an Owner turns
    /// channels on, and `Try again`; nothing says it is out. A card that hears says nothing of it.
    #[test]
    fn a_deaf_card_says_so_and_opens_where_channels_are_turned_on() {
        let seat = NativeAccount { id: "seat".into(), label: "Team seat".into(), signed_in: true, deaf: Some("w0".into()), ..Default::default() };
        let (_held, mut rt) = card_window(seat.clone());
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Deaf: messages do not reach agents here ▸"), "{t:?}");
        assert!(!t.iter().any(|x| x == DEAF_WHY || x.starts_with("Out")), "closed, and not out: {t:?}");
        press(&mut rt, "deaf-seat");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Deaf: messages do not reach agents here ▾") && t.iter().any(|x| x == "Messages not seen. An Owner enables channels in"), "{t:?}");
        assert!(t.iter().any(|x| x == "Organization settings › Claude Code › Channels ↗") && t.iter().any(|x| x == "Try again"), "{t:?}");
        drop((_held, rt));
        let (_held, rt) = card_window(NativeAccount { deaf: None, ..seat });
        assert!(!texts(&rt).iter().any(|x| x.starts_with("Deaf")), "it hears");
    }

    /// Every word the window RT shows, or says on a hover.
    fn words(rt: &rune::runtime::Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).flat_map(|e| [e.text, e.label, e.help]).flatten().collect()
    }

    /// The page as the window draws it, for [`the_page_names_the_engine_never_coordd`].
    fn whole_page(s: &App, _: &mut rune::Cx<App>) -> El {
        page(s, &Kit::neutral(false), "").1
    }

    /// Toward him the page names the engine, never its file `coordd`, on made-up answers: the hovers of a refused login's
    /// and an out account's buttons, an agy sign-in's terms, an open deaf line's `Try again`, an action the engine did not
    /// get, and the page before its first answer.
    #[test]
    fn the_page_names_the_engine_never_coordd() {
        let mut all = Vec::new();
        let refused = NativeAccount { id: "work".into(), label: "Work".into(), signed_in: true, out_for: Some("authentication_failed".into()), out_until: Some(UNKNOWN_RESET), ..Default::default() };
        let out = NativeAccount { id: "spare".into(), label: "Spare".into(), signed_in: true, out_for: Some("overloaded".into()), out_until: Some(UNKNOWN_RESET), ..Default::default() };
        let agy = NativeAccount { id: "personal".into(), label: "Personal".into(), harness: "agy".into(), signed_in: true, signed_in_by: Some("t3code".into()), ..Default::default() };
        let own = NativeAccount { id: "own".into(), label: "Own".into(), harness: "agy".into(), signed_in: true, signed_in_by: Some("agy".into()), ..Default::default() };
        for a in [refused, out, agy, own] {
            let (_held, rt) = card_window(a);
            all.extend(words(&rt));
        }
        let seat = NativeAccount { id: "seat".into(), label: "Team seat".into(), signed_in: true, deaf: Some("w0".into()), ..Default::default() };
        let (held, mut rt) = card_window(seat);
        press(&mut rt, "deaf-seat");
        // pressed with no pipe to the engine
        rt.update(|s, _| act(s, "seat", What::Keep, Request::AccountKeep { id: "seat".into() }));
        rt.frame(0.016);
        all.extend(words(&rt));
        drop((rt, held));
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.ext.insert(Accounts::default());
        let mut rt = rune::runtime::Runtime::new(s, whole_page, (900.0, 700.0), rune::Env::new(false, 1.0));
        rt.frame(0.016);
        all.extend(words(&rt));
        let named: Vec<&String> = all.iter().filter(|w| w.contains("coordd")).collect();
        assert!(named.is_empty(), "{named:?}");
        for w in [
            "Mark its login as working",
            "Mark this account as back",
            AGY_T3CODE_TERMS,
            AGY_OWN_TERMS,
            "Try it with the next agent",
            "The engine is not reached",
            "asking the engine…",
        ] {
            assert!(all.iter().any(|x| x == w), "{w}: {all:?}");
        }
        assert!(all.iter().any(|x| x == "Agents run inside t3code") && all.iter().any(|x| x == "Yolo agents run tools without asking"), "{all:?}");
    }

    /// A sign-in account whose login was refused twice (0.2 D22 D), on made-up answers: `Login refused: sign in again`
    /// with `Sign in` and `Try again`, not `Out, reset unknown`; a key account's refusal, a past one and another
    /// kind of out are not it.
    #[test]
    fn a_refused_login_says_sign_in_again_not_out() {
        let now = 1_000;
        let a = NativeAccount { id: "work".into(), label: "Work".into(), signed_in: true, out_for: Some("authentication_failed".into()), out_until: Some(UNKNOWN_RESET), out_words: Some("Please run /login · API Error: 401".into()), ..Default::default() };
        assert!(login_refused(&a, now));
        assert!(!login_refused(&NativeAccount { key: true, ..a.clone() }, now), "a key account's is `Key refused`");
        assert!(!login_refused(&NativeAccount { out_until: Some(now), ..a.clone() }, now), "back already");
        assert!(!login_refused(&NativeAccount { out_for: Some("rate_limit".into()), ..a.clone() }, now), "at its limit");
        let (_held, rt) = card_window(a);
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "Login refused: sign in again") && t.iter().any(|x| x == "Sign in") && t.iter().any(|x| x == "Try again"), "{t:?}");
        assert!(!t.iter().any(|x| x == "Out, reset unknown"), "said once: {t:?}");
    }

    /// The key typed is moved out of its field when sent: no field holds it after, and nothing typed is in what the
    /// card says when it could not be sent.
    #[test]
    fn a_key_sent_leaves_no_field_holding_it() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        s.ext.get_mut::<Accounts>().keys.insert("work-api".into(), TextEdit::new("sk-made-up-0000"));
        send_key(&mut s, "work-api");
        let st = s.ext.get::<Accounts>();
        assert!(st.keys.get("work-api").is_none_or(|f| f.text.is_empty()));
        let said = format!("{:?}", st.said.get("work-api"));
        assert!(!said.contains("sk-made-up"), "{said}");
        assert!(!crate::when::day(0).is_empty());
    }

    /// A key account says why it is out from what coordd gave: refused, no credit, at its limit; not out, nothing.
    #[test]
    fn a_key_account_says_why_it_is_out() {
        let a = |out_for: Option<&str>, until: Option<i64>| NativeAccount { key: true, out_for: out_for.map(String::from), out_until: until, ..Default::default() };
        assert_eq!(key_out(&a(Some("authentication_failed"), Some(UNKNOWN_RESET)), 100), Some("authentication_failed"));
        assert_eq!(key_out(&a(Some("billing_error"), Some(200)), 100), Some("billing_error"));
        assert_eq!(key_out(&a(Some("rate_limit"), Some(50)), 100), None, "back already");
        assert_eq!(key_out(&NativeAccount { out_for: Some("rate_limit".into()), out_until: Some(200), ..Default::default() }, 100), None, "a sign-in account");
    }

    /// An opencode account on a key says why it is out as a key card does, on made-up answers: a refused key is
    /// `Key refused` with opencode's words and the field open, not `Login refused`; no credit is `No credit`; a limit
    /// coordd takes again by itself is `At its limit, taken again at T`; one with its own end is `Out until T`. One whose
    /// provider signs in only by its browser way (OpenAI) keeps `Login refused: sign in again` with `Sign in`.
    #[test]
    fn an_opencode_key_card_says_why_it_is_out_as_a_key_card_does() {
        let now = now_s();
        let go = |out_for: &str, until: i64, words: Option<&str>| NativeAccount {
            id: "go".into(),
            label: "Go plan".into(),
            harness: "opencode".into(),
            provider: Some("opencode-go".into()),
            signed_in: true,
            out_for: Some(out_for.into()),
            out_until: Some(until),
            out_words: words.map(String::from),
            ..Default::default()
        };
        let refused = go("authentication_failed", UNKNOWN_RESET, Some("ProviderAuthError: Invalid API key"));
        assert!(keyed(&refused) && !login_refused(&refused, now) && key_out(&refused, now) == Some("authentication_failed"));
        let card = |a: NativeAccount| {
            let (held, rt) = card_window(a);
            let t = texts(&rt);
            drop((held, rt));
            t
        };
        let t = card(refused);
        assert!(t.iter().any(|x| x == "Key refused") && t.iter().any(|x| x == "ProviderAuthError: Invalid API key"), "{t:?}");
        assert!(t.iter().any(|x| x == "From opencode.ai → API keys"), "the field open at once: {t:?}");
        assert!(!t.iter().any(|x| x.starts_with("Login refused") || x.starts_with("Out")), "{t:?}");
        let t = card(go("billing_error", UNKNOWN_RESET, Some("Quota exceeded. Check your plan and billing details.")));
        assert!(t.iter().any(|x| x == "No credit") && t.iter().any(|x| x == "Quota exceeded. Check your plan and billing details."), "{t:?}");
        assert!(!t.iter().any(|x| x.starts_with("Out") || x.starts_with("From opencode")), "no field, no `Out`: {t:?}");
        let back = now + 600;
        let t = card(NativeAccount { again_at: Some(back), ..go("rate_limit", UNKNOWN_RESET, None) });
        assert!(t.iter().any(|x| *x == format!("At its limit, taken again at {}", crate::when::secs(back))), "{t:?}");
        assert!(!t.iter().any(|x| x.starts_with("Out")), "{t:?}");
        let t = card(go("rate_limit", now + 3600, None));
        assert!(t.iter().any(|x| x.starts_with(&format!("Out until {}", crate::when::secs(now + 3600)))), "{t:?}");
        // OpenAI signs in only by its browser way: its refusal is a login's
        let openai = NativeAccount { provider: Some("openai".into()), ..go("authentication_failed", UNKNOWN_RESET, Some("401 Unauthorized")) };
        assert!(!keyed(&openai) && login_refused(&openai, now) && key_out(&openai, now).is_none());
        let t = card(openai);
        assert!(t.iter().any(|x| x == "Login refused: sign in again") && t.iter().any(|x| x == "Sign in"), "{t:?}");
        assert!(!t.iter().any(|x| x == "Key refused"), "{t:?}");
    }

    /// On made-up answers: an OpenCode Go card draws Go's 5-hour, weekly and monthly windows, each with what is left, as
    /// of its reading, with `Check limits` beside. One whose key is not kept says its limits need the key once more, and
    /// `Replace key` opens the card's field. A coordd before `GO_USAGE_PROTOCOL` draws no windows on it.
    #[test]
    fn a_go_card_draws_its_five_hours_week_and_month_from_gos_page() {
        let now = now_s();
        let w = |used: f64, secs: i64| LimitWindow { used_percent: used, resets_at: now + secs };
        let go = NativeAccount {
            id: "go".into(),
            label: "Go plan".into(),
            harness: "opencode".into(),
            provider: Some("opencode-go".into()),
            signed_in: true,
            usage_key: true,
            five_hour: Some(w(30.0, 3600)),
            seven_day: Some(w(45.0, 86_400)),
            month: Some(w(12.0, 20 * 86_400)),
            limits_from: "go usage".into(),
            limits_seen: now - 120,
            ..Default::default()
        };
        let (held, rt) = card_window(go.clone());
        let t = texts(&rt);
        for x in ["5-hour", "Weekly", "Monthly", "70% remaining", "55% remaining", "88% remaining", "Check limits"] {
            assert!(t.iter().any(|y| y == x), "{x}: {t:?}");
        }
        assert!(t.iter().any(|y| *y == format!("as of {}", crate::when::secs(now - 120))), "{t:?}");
        drop((held, rt));
        let (held, mut rt) = card_window(NativeAccount { usage_key: false, five_hour: None, seven_day: None, month: None, limits_seen: 0, limits_from: String::new(), ..go.clone() });
        let t = texts(&rt);
        assert!(t.iter().any(|y| y == "Limits need its key once more") && !t.iter().any(|y| y == "Check limits"), "{t:?}");
        press(&mut rt, "usage-key-go");
        let t = texts(&rt);
        assert!(t.iter().any(|y| y == "From opencode.ai → API keys") && rt.find("usage-key-go").is_none(), "the field, and the row's press gone: {t:?}");
        drop((held, rt));
        let (held, mut rt) = card_window(go);
        rt.update(|s, _| s.projects[0].protocol = coord_types::GO_USAGE_PROTOCOL - 1);
        rt.frame(0.016);
        let t = texts(&rt);
        assert!(!t.iter().any(|y| y == "Monthly" || y == "5-hour" || y == "Check limits"), "{t:?}");
        drop((held, rt));
    }

    /// An agy account as t3code read it, on made-up times: t3code's four windows by its own labels, each limit's name once
    /// over its windows, those not used yet as not started, one in use with its countdown, as of its reading with
    /// `Check limits` beside; its kind line names the personal Google account, its hover that t3code names no plan.
    #[test]
    fn an_agy_card_draws_t3codes_windows_by_their_labels_and_says_its_kind_of_sign_in() {
        let now = now_s();
        let week = |label: &str, used: f64, resets_at: i64| coord_types::ModelWeek { model: label.into(), window: LimitWindow { used_percent: used, resets_at } };
        let none = coord_types::NOT_STARTED;
        let agy = NativeAccount {
            id: "personal".into(),
            label: "Personal".into(),
            harness: "agy".into(),
            signed_in: true,
            auth_method: Some("oauth-personal".into()),
            model_weeks: vec![week("Claude & GPT (shared) · 5-hour", 0.0, none), week("Gemini · 5-hour", 25.0, now + 3600), week("Claude & GPT (shared) · Weekly", 0.0, none), week("Gemini · Weekly", 0.0, none)],
            model_weeks_seen: Some(now - 60),
            limits_seen: now - 60,
            limits_from: "t3code".into(),
            signed_in_by: Some("t3code".into()),
            ..Default::default()
        };
        let (held, rt) = card_window(agy);
        let t = texts(&rt);
        for x in ["Claude & GPT (shared) · 5-hour", "Gemini · 5-hour", "Claude & GPT (shared) · Weekly", "Gemini · Weekly", "not started", "100% remaining", "75% remaining", "Check limits", "Antigravity · personal Google account, through t3code"] {
            assert!(t.iter().any(|y| y == x), "{x}: {t:?}");
        }
        assert_eq!(t.iter().filter(|y| *y == "not started").count(), 3, "{t:?}");
        // each limit's name once over its windows, in the order t3code first gives it; each meter by the whole label
        let rows: Vec<&str> = t.iter().map(String::as_str).filter(|y| ["Claude & GPT (shared)", "Gemini", "5-hour", "Weekly"].contains(y)).collect();
        assert_eq!(rows, ["Claude & GPT (shared)", "5-hour", "Weekly", "Gemini", "5-hour", "Weekly"], "{t:?}");
        assert!(!t.iter().any(|y| y == "Limits not seen yet" || y.starts_with("Weekly (")), "{t:?}");
        assert!(t.iter().any(|y| *y == format!("as of {}", crate::when::secs(now - 60))), "{t:?}");
        drop((held, rt));
    }

    /// N-1956 (consult-lead-c p3, point 2): an agy sign-in account's lines by its way in, as coordd says it. One agy
    /// signed in itself names agy's own sign-in and its tool calls asked of the engine, and no word of its card, a
    /// hover's included, says t3code; one t3code signed in names t3code on both lines; one from a coordd that says
    /// neither names no program. Removing it names the program that signs it out, and a failed sign-out of agy's is
    /// read as any other's.
    #[test]
    fn an_agy_card_names_the_program_that_signed_it_in() {
        let agy = |by: Option<&str>| NativeAccount { id: "personal".into(), label: "Personal".into(), harness: "agy".into(), signed_in: true,
            auth_method: Some("oauth-personal".into()), signed_in_by: by.map(String::from), models: vec!["agy/gemini-3.8-flash-high".into()], ..Default::default() };
        let (held, rt) = card_window(agy(Some("agy")));
        let w = words(&rt);
        assert!(w.iter().any(|y| y == "Antigravity · personal Google account, agy's own sign-in") && w.iter().any(|y| y == AGY_OWN_TERMS), "{w:?}");
        assert!(!w.iter().any(|y| y.contains("t3code")), "agy signed it in: {w:?}");
        drop((held, rt));
        let (held, rt) = card_window(agy(Some("t3code")));
        let w = words(&rt);
        assert!(w.iter().any(|y| y == "Antigravity · personal Google account, through t3code") && w.iter().any(|y| y == AGY_T3CODE_TERMS), "{w:?}");
        assert!(!w.iter().any(|y| y == AGY_OWN_TERMS), "{w:?}");
        drop((held, rt));
        let (held, rt) = card_window(agy(None));
        let w = words(&rt);
        assert!(w.iter().any(|y| y == "Antigravity · personal Google account") && !w.iter().any(|y| y.contains("t3code") || y == AGY_OWN_TERMS), "{w:?}");
        drop((held, rt));
        assert_eq!((signs(&agy(Some("agy"))), signs(&agy(Some("t3code")))), ("agy", "t3code"));
        assert_eq!(removed_line("account personal: removed, but agy's sign-out failed: agy-rpc logout exit 1: agy-rpc: agy did not sign the account out: it ended. Its folder, with what agy left in it, is set aside at /x", "Personal"),
                   "Personal removed; agy's sign-out failed");
    }

    /// An agy account out for some of its models, on made-up names: each said under its windows by agy's name, until its
    /// time, or with none known, the refusal's words on the hover; the account itself is not out, and a model whose time
    /// has passed is not said.
    #[test]
    fn an_agy_card_says_each_model_it_is_out_for_and_the_account_stays_in() {
        let now = now_s();
        let out = |model: &str, until: i64| ModelOut { model: model.into(), until, since: now - 60, words: Some("You have reached your Antigravity quota for this period.".into()) };
        let agy = NativeAccount {
            id: "personal".into(),
            label: "Personal".into(),
            harness: "agy".into(),
            signed_in: true,
            models: vec!["agy/claude-sonnet-4-6".into(), "agy/gpt-oss-120b".into(), "agy/gemini-3.8-flash-high".into()],
            models_out: vec![out("agy/claude-sonnet-4-6", now + 7200), out("agy/gpt-oss-120b", UNKNOWN_RESET), out("agy/gemini-3.8-flash-high", now - 1)],
            model_weeks: vec![coord_types::ModelWeek { model: "Claude & GPT (shared) · 5-hour".into(), window: LimitWindow { used_percent: 100.0, resets_at: now + 7200 } }],
            limits_seen: now - 60,
            limits_from: "t3code".into(),
            ..Default::default()
        };
        let (held, rt) = card_window(agy);
        let t = texts(&rt);
        let until = format!("Out for claude-sonnet-4-6 until {} ({})", crate::when::secs(now + 7200), countdown(now + 7200, now));
        assert!(t.iter().any(|y| *y == until), "{until}: {t:?}");
        assert!(t.iter().any(|y| y == "Out for gpt-oss-120b, reset unknown"), "{t:?}");
        assert!(!t.iter().any(|y| y.contains("gemini-3.8-flash-high") && y.starts_with("Out")), "its time passed: {t:?}");
        assert!(!t.iter().any(|y| y.starts_with("Out until") || y == "Out, reset unknown"), "the account is in: {t:?}");
        drop((held, rt));
    }

    /// Accounts on made-up names: Claude's two tiers, a Go plan and an agy account, both at Claude's top number. Each
    /// harness has its own tiers, numbered from its own first; the sheet offers a new agy account the agy tier and one
    /// under it, never Claude's. (A unit test registers no extension, so the still shows no headings.)
    #[test]
    fn each_harness_has_its_own_tiers_and_the_sheet_offers_its_own() {
        let acct = |id: &str, harness: &str, tier: i64| NativeAccount { id: id.into(), label: id.into(), harness: harness.into(), tier, signed_in: true, ..Default::default() };
        let his = [acct("seat", CLAUDE, 40), acct("max", CLAUDE, 30), acct("go", "opencode", 40), acct("gmail", "agy", 40)];
        assert_eq!(tiers_of(&his, CLAUDE, vec![]), [40, 30]);
        assert_eq!(tiers_of(&his, CLAUDE, vec![35, 40]), [40, 35, 30], "a gateway's Claude tiers among Claude's");
        assert_eq!(tiers_of(&his, "opencode", vec![]), [40]);
        assert_eq!(tiers_of(&his, "agy", vec![]), [40]);
        assert_eq!(sheet::offered_of(&tiers_of(&his, "agy", vec![])), vec![("Tier 1".to_string(), 40), ("Tier 2 · new".to_string(), 30)]);
        assert_eq!(sheet::offered_of(&tiers_of(&his, CLAUDE, vec![])), vec![("Tier 1".to_string(), 40), ("Tier 2".to_string(), 30), ("Tier 3 · new".to_string(), 20)]);
        assert_eq!((sheet::harness_of(&Pick::Agy), sheet::harness_of(&Pick::AgyKey), sheet::harness_of(&Pick::Opencode), sheet::harness_of(&Pick::Claude), sheet::harness_of(&Pick::Anthropic)), ("agy", "agy", "opencode", CLAUDE, CLAUDE));
    }

    /// A sign-in account whose login passed its end needs a new sign-in, though coordd last heard it signed in; one out at
    /// its limit, one not signed in, or a key account is not this.
    #[test]
    fn a_login_past_its_end_needs_a_new_sign_in() {
        let a = |signed_in: bool, ends: Option<i64>| NativeAccount { signed_in, login_expires: ends, ..Default::default() };
        assert!(login_ran_out(&a(true, Some(100)), 100));
        assert!(!login_ran_out(&a(true, Some(101)), 100), "still runs");
        assert!(!login_ran_out(&a(true, None), 100), "no end said");
        assert!(!login_ran_out(&a(false, Some(50)), 100), "not signed in: its own line");
        assert!(!login_ran_out(&NativeAccount { key: true, ..a(true, Some(50)) }, 100), "a key account");
        assert!(!login_ran_out(&NativeAccount { out_until: Some(200), out_for: Some("rate_limit".into()), ..a(true, Some(300)) }, 100), "at its limit, its login fine");
    }

    /// A removal is said in one line: removed, or removed with the sign-out's failure and why; no path.
    #[test]
    fn a_removed_accounts_id_is_not_made_again_and_it_is_listed_once() {
        let gone = |name: &str, at: i64| RemovedAccount { harness: "claude".into(), id: name.split('.').next().unwrap().into(), label: "Work".into(), name: name.into(), at, not_signed_out: false };
        let answer = |r: Vec<RemovedAccount>| Ok(NativeAccounts { removed: r, ..Default::default() });
        let got: HashMap<usize, Result<NativeAccounts, String>> = [(0, answer(vec![gone("work.20261118-200000", 10)])), (1, answer(vec![gone("work.20261118-200000", 10), gone("work-2.20261118-210000", 20)]))].into();
        let all = removed_all(&got);
        assert_eq!(all.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["work-2.20261118-210000", "work.20261118-200000"], "once each, newest first");
        assert_eq!(id_for("Work", &taken(&[], &all)), "work-3");
        assert_eq!(removed_row(&gone("work.x", 0)), "Work · its usage is kept");
        assert_eq!(removed_row(&RemovedAccount { not_signed_out: true, ..gone("work.x", 0) }), "Work · not signed out");
        assert_eq!(removed_help(&RemovedAccount { not_signed_out: true, ..gone("work.x", 0) }), Some("Still signed in; Put back, then Remove"));
        assert_eq!(removed_help(&gone("work.x", 0)), None, "a signed-out line has no hover");
        let half: HashMap<usize, Result<NativeAccounts, String>> = [(0, answer(vec![gone("work.y", 5)])), (1, answer(vec![RemovedAccount { not_signed_out: true, ..gone("work.y", 5) }]))].into();
        assert!(removed_all(&half)[0].not_signed_out, "either coordd saying not signed out is enough");
    }

    #[test]
    fn a_put_back_says_what_he_does_next() {
        assert_eq!(put_back_line("account work: put back; not signed in", "Work (Max)"), "Work (Max) put back; sign in again");
        assert_eq!(put_back_line("account k: put back; key set", "Key"), "Key put back.");
        assert_eq!(made_id("account work-max-2: made, not signed in yet (/x) (a removed account had the id work-max)").as_deref(), Some("work-max-2"));
        assert_eq!(made_id("something else"), None);
        let alone: HashMap<usize, Result<NativeAccounts, String>> = [(0, Ok(NativeAccounts::default()))].into();
        assert!(no_own_calls(&alone), "a key account gives them no way: not asked");
        assert!(moves_on_sign_in(&alone));
        let set: HashMap<usize, Result<NativeAccounts, String>> = [(0, Ok(NativeAccounts { all: Some(Default::default()), ..Default::default() }))].into();
        assert!(!moves_on_sign_in(&set), "after the switch was set a sign-in moves nobody");
        assert_eq!(by_label("account work: research.orch runs on it; put it on another account or the gateway first", "work", "Work (Max)"), "Work (Max): research.orch runs on it; put it on another account or the gateway first");
        let gw: HashMap<usize, Result<NativeAccounts, String>> = [(0, Ok(NativeAccounts { gateway: true, ..Default::default() }))].into();
        assert!(!no_own_calls(&gw));
    }

    #[test]
    fn removing_an_account_in_use_says_who_moves_where() {
        let ag = |n: &str| coord_types::NativeAgent { coord: "research".into(), agent: n.into(), ..Default::default() };
        let acc = |id: &str, signed: bool| NativeAccount { id: id.into(), label: id.to_uppercase(), signed_in: signed, ..Default::default() };
        let mut work = acc("work", true);
        assert_eq!(moves_to(&work, &[], false, 0), None, "no agent: removed at once");
        work.agents = vec![ag("orch"), ag("w2"), ag("w3")];
        assert_eq!(who(&work), "research.orch and 2 others");
        assert_eq!(moves_to(&work, &[work.clone(), acc("home", true)], false, 0), Some(Ok("to HOME".into())));
        assert_eq!(moves_to(&work, &[work.clone(), acc("home", false)], true, 0), Some(Ok("to the gateway".into())));
        assert_eq!(moves_to(&work, &[work.clone(), acc("home", false)], false, 0), Some(Err(())));
        let mut out = acc("home", true);
        out.out_until = Some(100);
        assert_eq!(moves_to(&work, &[out], false, 50), Some(Err(())), "an account out now takes no agent");
        let go = NativeAccount { harness: "opencode".into(), ..acc("go", true) };
        assert_eq!(moves_to(&work, &[work.clone(), go], false, 0), Some(Err(())), "an opencode account takes no Claude Code agent");
    }

    #[test]
    fn a_removal_is_said_in_one_line() {
        assert_eq!(removed_line("account work: removed (signed out; its folder is set aside at /home/t/agent-coord/accounts/removed/claude/work.20261118-211500)", "Work (Max)"), "Work (Max) removed");
        assert_eq!(removed_line("account work: removed, but Claude Code's sign-out failed: no network. Its folder, with what Claude Code left in it, is set aside at /x", "Work (Max)"), "Work (Max) removed; sign-out failed: no network");
        assert_eq!(removed_line("account go: removed, but opencode's sign-out failed: opencode auth logout: no such provider. Its folder, with what opencode left in it, is set aside at /x", "Go plan"), "Go plan removed; sign-out failed: opencode auth logout: no such provider");
        assert_eq!(removed_line("account g: removed, but t3code's sign-out failed: its server did not start. Its folder, with what t3code left in it, is set aside at /x", "Google AI Pro"), "Google AI Pro removed; sign-out failed: its server did not start");
    }

    /// coordd's refusals lose the command they begin with and the one they tell to type.
    #[test]
    fn a_refusal_names_no_coord_command() {
        assert_eq!(plain("coord account new: account work: there is one already"), "account work: there is one already");
        assert_eq!(plain("coord account: no account work (coord account new work)"), "no account work");
        assert_eq!(plain("coord account code: no sign-in runs for work (coord account signin work)"), "no sign-in runs for work");
        assert_eq!(plain("coord native: no account is signed in (coord accounts)"), "no account is signed in");
        assert_eq!(plain("coord account signin: its command could not be put in a cgroup of its own (EPERM): x"), "its command could not be put in a cgroup of its own (EPERM): x");
        assert_eq!(plain("coordd: N-1104.p31 is no ask of his"), "N-1104.p31 is no ask of his", "the engine's name is no word of his");
        assert_eq!(plain("The engine is not reached"), "The engine is not reached");
    }

    #[test]
    fn a_countdown_reads_as_the_proxy_cards_say_it() {
        assert_eq!((countdown(1000 + 2 * 3600 + 18 * 60, 1000), countdown(1000 + 90_000, 1000), countdown(900, 1000)), ("in 2 h 18 m".into(), "in 1 d 01 h".into(), "reset time reached".into()));
        assert_eq!(countdown(1000 + 114 * 3600, 1000), "in 4 d 18 h", "N-1104 p678");
    }

    /// opencode's sign-in link is known only when it is `https` to a host of `OPENCODE_SIGN_IN_HOSTS`; any other is shown
    /// with a warning. Claude Code's is any. The answer's second line is opencode's instructions (`Enter code: …`).
    #[test]
    fn an_opencode_link_is_known_only_on_its_hosts_and_its_instructions_are_kept() {
        let oc = SignIn::new(Whose::Opencode("ChatGPT Pro/Plus (headless)"));
        assert!(oc.known("https://auth.openai.com/codex/device"));
        for foreign in ["https://auth.openai.com.evil.example/codex/device", "https://evil.example/codex/device", "http://auth.openai.com/codex/device", "https://user@evil.example/?auth.openai.com"] {
            assert!(!oc.known(foreign), "{foreign}");
        }
        assert!(SignIn::new(Whose::Claude).known("https://claude.ai/oauth/authorize?x=1"));
        // t3code's Google sign-in: Google's own address alone
        let agy = SignIn::new(Whose::Agy);
        assert!(agy.known("https://accounts.google.com/o/oauth2/v2/auth?client_id=x"));
        for foreign in ["https://accounts.google.com.evil.example/o/oauth2", "http://accounts.google.com/o/oauth2", "https://user@accounts.google.com/", "https://accounts.google.com:8443/", "https://evil.example/?accounts.google.com"] {
            assert!(!agy.known(foreign), "{foreign}");
        }
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        s.ext.get_mut::<Accounts>().signin.insert("chatgpt".into(), oc);
        reply(&mut s, 0, Box::new(Act("chatgpt".into(), What::Signin)), Ok("https://evil.example/codex/device\nEnter code: ABCD-1234".into()));
        let si = &s.ext.get::<Accounts>().signin["chatgpt"];
        assert_eq!((si.url.as_deref(), si.said.as_deref()), (Some("https://evil.example/codex/device"), Some("Enter code: ABCD-1234")));
        assert!(!si.known(si.url.as_deref().unwrap()), "shown with a warning");
        s.ext.get_mut::<Accounts>().signin.insert("work".into(), SignIn::new(Whose::Claude));
        reply(&mut s, 0, Box::new(Act("work".into(), What::Signin)), Ok("https://claude.ai/oauth/authorize?x=1\nOpen it in a browser".into()));
        assert_eq!(s.ext.get::<Accounts>().signin["work"].said, None, "Claude Code's words for a terminal are not shown");
    }
}
