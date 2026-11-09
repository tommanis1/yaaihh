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
use rune_kit::shell::SidebarSection;
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
    /// Its section of the sidebar, under the project's.
    pub nav: fn(&App, &Kit) -> SidebarSection,
    /// What its section shows, as a text that changes when the section does: the sidebar is built again only then.
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

    pub fn get_mut<T: 'static>(&mut self) -> &mut T {
        self.0.get_mut(&TypeId::of::<T>()).and_then(|b| b.downcast_mut()).unwrap_or_else(|| panic!("no extension state {}", std::any::type_name::<T>()))
    }
}
