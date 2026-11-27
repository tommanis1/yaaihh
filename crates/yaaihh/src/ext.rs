//! Extensions: pages and state a variant of yaaihh adds to the core (an account proxy's pages, say). The core knows
//! none of them; a variant's binary passes its extensions to [`crate::run`], and the core then calls each at its
//! points: its state made at start ([`Extension::init`]), set up with the window ([`Extension::setup`]), its work
//! taken each update ([`Extension::poll`]), its section in the sidebar, its pages in a pane (`Sel::Ext`), the
//! replies to the coordd requests it made ([`crate::Proj::ask`]), the price table the agents' cost is read from, and
//! its scenes (`--snap`).
//!
//! An extension's state lives in the App's [`States`], one value per type, reached by the extension's own accessors
//! (`s.ext.get::<T>()`): a field of its own, so it borrows apart from the core's fields.

use crate::App;
use rune::{El, Tx};
use rune_kit::Kit;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::OnceLock;

/// An extension: its name and the functions the core calls.
pub struct Extension {
    /// Its name: the key of its pages (`Sel::Ext(name, page)`) and of its requests' replies.
    pub name: &'static str,
    /// Its state put into `s.ext`, before the window opens (and before a scene).
    pub init: fn(&mut App),
    /// The window opened: work that runs beside it starts, waking the window with the waker.
    pub setup: fn(&mut App, rune::Waker),
    /// Each update: take what arrived; true when the window should be drawn again.
    pub poll: fn(&mut App, &mut Tx) -> bool,
    /// Its pages as rows of the sidebar at DEPTH, beneath its own entry among the project's pages while it is open
    /// (the owner, N-358: one menu, on the left), and a row saying what is wrong with it when something is, named.
    pub rows: fn(&App, &Kit, u8) -> Vec<El>,
    /// What its rows show, as a text that changes when they do: the sidebar is built again only then.
    pub nav_key: fn(&App) -> String,
    /// Page PAGE (the second part of `Sel::Ext`): its title and content.
    pub page: fn(&App, &Kit, &str) -> (String, El),
    /// Page PAGE's title alone (an unfocused pane's label).
    pub title: fn(&str) -> String,
    /// The reply to a request it made of project I's coordd ([`crate::Proj::ask`]): the data it asked with, and the
    /// reply (Err: coordd's refusal).
    pub reply: fn(&mut App, usize, Box<dyn Any>, Result<String, String>),
    /// The model price table the agents' cost is read from, when the extension keeps one (the core's is the bundled
    /// catalog); asked at start and again at most once a minute while the agents page shows.
    pub prices: Option<fn(&App) -> Result<prices::PriceTable, String>>,
    /// Its scenes (`--snap F --scene NAME`), beside the core's.
    pub scenes: &'static [(&'static str, fn(&mut App))],
    /// The extensions whose tab it takes the place of: theirs is not shown while it is there, their state and requests
    /// still are (the proxy's Accounts page shows the native accounts as cards, and the core's page of them goes:
    /// paper-research N-412 p199, "one of each thing"). Its page of their name stands for theirs ([`tab_page`]).
    pub replaces: &'static [&'static str],
    /// The accounts it lets the owner add and how, in the one Add account sheet (accounts.rs, the owner N-541 post 32:
    /// "a single thing you click in the overview … where you select the logo and go through the flow").
    pub accounts: Option<AccountsExt>,
    /// Its pages that are the window's own of that name (the proxy's `usage`): the sidebar's foot row of that name
    /// opens its page in place of the core's ([`window_page`]), and its own rows leave them out.
    pub window_pages: &'static [&'static str],
}

/// What an extension gives the Add account sheet: its tiles, in the sheet's two rows after the core's `Claude Code`
/// (a Claude subscription on Claude Code's own sign-in; an Anthropic API key), and each tile's own steps, drawn inside
/// the sheet.
pub struct AccountsExt {
    /// The sheet opened: read what the tiles and flows need.
    pub open: fn(&mut App),
    /// The heading of its row of subscription tiles in the sheet, in its own words (the core's row is its own,
    /// `Built-in subscriptions`, and holds no extension's tile: the owner, N-1104 post 280).
    pub subscriptions: &'static str,
    /// Its tiles, in its order.
    pub tiles: fn(&App) -> Vec<AccountTile>,
    /// Tile ID picked: its flow starts.
    pub pick: fn(&mut App, &str),
    /// Tile ID's steps, as the sheet's body; a dialog of its own over the sheet goes on the layer.
    pub flow: fn(&App, &Kit, &str, &mut Vec<El>) -> El,
    /// The sheet went back to the tiles (false) or closed (true): what a flow left running ends; closed, what `open`
    /// read is dropped and its accounts are read again.
    pub leave: fn(&mut App, bool),
    /// The tiers its Claude accounts stand in, as the overview numbers them (their priorities, highest first): a new
    /// account of the core's is offered the same tiers.
    pub tiers: fn(&App) -> Vec<i64>,
    /// One faint line under the sheet's tiles, where some of its ways to add an account are not in this window
    /// ([`AccountTile::elsewhere`]): where they are (nothing he has in one window is gone in another without a line
    /// saying so). None: nothing to say.
    pub note: fn(&App) -> Option<&'static str>,
}

/// The sheet's two groups.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileGroup {
    Subscriptions,
    ApiKeys,
}

/// One way to add an account, as a tile of the sheet: a provider by its logo and name.
#[derive(Clone, Debug)]
pub struct AccountTile {
    /// The extension's own word for it, handed back to `pick` and `flow`.
    pub id: String,
    pub group: TileGroup,
    pub label: String,
    /// A few faint words under the label, where the name alone does not say what is asked for.
    pub note: Option<&'static str>,
    pub logo: Option<&'static rune::Icon>,
    /// Its steps are not in this window (the Windows window, for a way its server in WSL does not answer yet): the
    /// tile is in sight, dimmed, and takes no click; [`AccountsExt::note`] says where it is.
    pub elsewhere: bool,
}

static EXTENSIONS: OnceLock<Vec<Extension>> = OnceLock::new();

/// Register the extensions, once, before the app starts (`run`).
pub(crate) fn register(exts: Vec<Extension>) {
    let _ = EXTENSIONS.set(exts);
}

/// The extensions registered (none before `run`).
pub fn all() -> &'static [Extension] {
    EXTENSIONS.get().map_or(&[], Vec::as_slice)
}

/// Extension E's tab on the Project page (paper-research N-101 p901: where an extension's page goes is the app's
/// decision, the sidebar keeping the owner's three entries): the title of its page "" when that is a page of it
/// (Harnesses), else its name (the proxy: "Proxy", its section's title too).
pub fn tab_label(e: &Extension) -> String {
    let t = (e.title)("");
    if !t.is_empty() {
        return t;
    }
    let mut c = e.name.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// Whether extension E is the window's, not a project's: the accounts, one that takes their tab or gives one of the
/// window's own pages (the proxy), and Harnesses, which says what this machine has (under the foot's Settings).
pub fn global(e: &Extension) -> bool {
    e.name == crate::accounts::NAME || e.name == crate::harness::NAME || e.replaces.contains(&crate::accounts::NAME) || !e.window_pages.is_empty()
}

/// Whether extension E has pages of its own names, its page "" being none of them (the proxy's Home, Accounts…): its
/// tab then shows its section beside the page, to go between them.
pub fn has_pages(e: &Extension) -> bool {
    (e.title)("").is_empty()
}

/// Whether extension E's tab gives way to another's ([`Extension::replaces`]).
pub fn replaced(e: &Extension) -> bool {
    replaced_in(all(), e)
}

fn replaced_in(exts: &[Extension], e: &Extension) -> bool {
    exts.iter().any(|o| o.replaces.contains(&e.name))
}

/// The extensions' tabs on a project's Project page, after the core's: none that is the window's ([`global`]) or
/// gives way to another's.
pub fn project_tabs() -> Vec<(String, crate::Sel)> {
    project_tabs_in(all())
}

fn project_tabs_in(exts: &[Extension]) -> Vec<(String, crate::Sel)> {
    exts.iter().filter(|e| !replaced_in(exts, e) && !global(e)).map(|e| (tab_label(e), crate::Sel::Ext(e.name.to_string(), String::new()))).collect()
}

/// The page a press opens to show extension NAME: its own, or while another takes the place of its tab
/// ([`Extension::replaces`]) that one's page of NAME's name, which is in a tab (the proxy's `accounts`: his accounts
/// beside the gateway's card). Its own drawn there would be in no tab, and the core's Accounts page draws no gateway
/// card beside the proxy's.
pub fn tab_page(name: &str) -> crate::Sel {
    tab_page_in(all(), name)
}

fn tab_page_in(exts: &[Extension], name: &str) -> crate::Sel {
    match exts.iter().find(|o| o.replaces.contains(&name)) {
        Some(o) => crate::Sel::Ext(o.name.into(), name.into()),
        None => crate::Sel::Ext(name.into(), String::new()),
    }
}

/// The page the window's own row NAME opens when an extension gives one of that name ([`Extension::window_pages`]).
pub fn window_page(name: &str) -> Option<crate::Sel> {
    window_page_in(all(), name)
}

fn window_page_in(exts: &[Extension], name: &str) -> Option<crate::Sel> {
    exts.iter().find(|e| e.window_pages.contains(&name)).map(|e| crate::Sel::Ext(e.name.into(), name.into()))
}

/// The extension called NAME.
pub fn get(name: &str) -> Option<&'static Extension> {
    all().iter().find(|e| e.name == name)
}

/// The extensions' states, one value per type.
#[derive(Default)]
pub struct States(HashMap<TypeId, Box<dyn Any>>);

impl States {
    pub fn insert<T: 'static>(&mut self, v: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(v));
    }

    /// The state of type T; its extension's `init` put it there, so its absence is that extension's bug.
    pub fn get<T: 'static>(&self) -> &T {
        self.0.get(&TypeId::of::<T>()).and_then(|b| b.downcast_ref()).unwrap_or_else(|| panic!("no extension state {}", std::any::type_name::<T>()))
    }

    /// The state of type T when its extension is in this build (state.rs keeps some of it across a swap).
    pub fn try_get<T: 'static>(&self) -> Option<&T> {
        self.0.get(&TypeId::of::<T>()).and_then(|b| b.downcast_ref())
    }

    pub fn try_get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.0.get_mut(&TypeId::of::<T>()).and_then(|b| b.downcast_mut())
    }

    pub fn get_mut<T: 'static>(&mut self) -> &mut T {
        self.0.get_mut(&TypeId::of::<T>()).and_then(|b| b.downcast_mut()).unwrap_or_else(|| panic!("no extension state {}", std::any::type_name::<T>()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Sel;

    /// The press to the Accounts tab (the Map's gateway, the message box's Accounts: `tab_page`) with each list of
    /// extensions a build registers (proto-window N-1479 p66, consult-3 p77): the core's (yaaihh's main.rs) opens the
    /// core's page; the train's (flavor/app's main.rs: the gateway's extension first, then the core's two) the
    /// gateway's own Accounts page, in its tab. The gateway's extension is a stand-in of its name and `replaces`
    /// (proxy/'s yaaihh-proxy, which this crate does not build; at the train's pin b318f66 `replaces: &["accounts"]`
    /// holds on every platform).
    #[test]
    fn a_press_for_a_replaced_extensions_page_opens_the_page_that_took_its_tab() {
        let core = [crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(tab_page_in(&core, "accounts"), Sel::Ext("accounts".into(), String::new()), "the core's Accounts page");
        let gateway = Extension { name: "proxy", replaces: &["accounts"], ..crate::harness::extension() };
        let train = [gateway, crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(tab_page_in(&train, "accounts"), Sel::Ext("proxy".into(), "accounts".into()), "the gateway's Accounts page, in its tab");
        assert_eq!(tab_page_in(&train, crate::harness::extension().name), Sel::Ext(crate::harness::extension().name.into(), String::new()), "one not replaced is its own");
    }

    /// A project's Project page holds that project's own pages alone: no tab of the proxy's, which is the window's
    /// (under the foot's Accounts), nor of Harnesses, which is this machine's. An extension of a project's
    /// own still has its tab.
    #[test]
    fn no_page_of_the_window_or_the_machine_is_a_tab_of_a_projects_project_page() {
        let core = [crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(project_tabs_in(&core), vec![], "the core's");
        let gateway = Extension { name: "proxy", replaces: &["accounts"], window_pages: &["usage"], ..crate::harness::extension() };
        let train = [gateway, crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(project_tabs_in(&train), vec![], "the train's");
        let own = Extension { name: "notes", title: |_| "Notes".into(), ..crate::harness::extension() };
        assert_eq!(project_tabs_in(&[own]), vec![("Notes".to_string(), Sel::Ext("notes".into(), String::new()))]);
    }

    /// The foot's Usage opens the gateway's Usage page when it gives one (N-1934 p2), else the core's.
    #[test]
    fn the_windows_usage_is_the_page_an_extension_gives_of_its_name() {
        let core = [crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(window_page_in(&core, "usage"), None, "the core's own Usage page");
        let gateway = Extension { name: "proxy", replaces: &["accounts"], window_pages: &["usage"], ..crate::harness::extension() };
        let train = [gateway, crate::harness::extension(), crate::accounts::extension()];
        assert_eq!(window_page_in(&train, "usage"), Some(Sel::Ext("proxy".into(), "usage".into())), "the gateway's Usage page");
        assert_eq!(window_page_in(&train, "home"), None, "only the pages it names");
    }
}
