//! The Add account sheet: the page's one `Add account` opens it; first the provider, by its logo, in rows
//! (subscriptions, API keys); then that provider's own steps inside the sheet; then the overview, the new account in its
//! tier.
//!
//! The core's tiles: two named `Claude Code`, a Claude subscription added by Claude Code's own sign-in (coordd runs it
//! in the account's folder and hands on its link; nothing of the login is read, written or copied by us) and an
//! Anthropic API key (the masked field, one request, never shown again); `opencode`, one of [`OPENCODE_PROVIDERS`], his
//! Go plan first, its key typed by coordd into opencode's own sign-in and kept nowhere, or that provider's browser way;
//! `Antigravity`, Google's agent, by his Google sign-in through agy's own or a Gemini API key. An extension adds its
//! own tiles and steps ([`crate::ext::AccountsExt`]).
//!
//! `Built-in subscriptions` holds the core's own alone; each extension's subscriptions stand in a row under its own
//! heading ([`subscription_rows`]); `API keys` is one row, the core's first. There is no other way in, and no sentence
//! among the tiles. A way not in this window stays in sight, dimmed, with one line saying where it is.

use super::*;
use crate::ext::{self, AccountTile, TileGroup};
use rune_kit::{Dialog, SegmentedControl};

static BACK: rune::Icon = rune::Icon { name: "accounts-back", view: 24.0, body: r#"<path d="m12 19-7-7 7-7M19 12H5" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"# };

/// Open the sheet on its tiles; each extension reads what its tiles need.
pub fn open_sheet(s: &mut App) {
    s.ext.get_mut::<Accounts>().add = Add { open: true, ..Add::default() };
    for a in ext::all().iter().filter_map(|e| e.accounts.as_ref()) {
        (a.open)(s);
    }
}

/// Open the sheet on extension NAME's tile TILE (its id), its flow started as a press of that tile: a card's `Sign in`
/// on a login that must be signed in again. The tiles alone when it has no such tile.
pub fn open_on(s: &mut App, name: &'static str, tile: &str) {
    open_sheet(s);
    let found = ext::get(name).and_then(|e| e.accounts.as_ref()).and_then(|a| (a.tiles)(s).into_iter().find(|t| t.id == tile));
    if let Some(t) = found {
        pick_ext(s, name, &t);
    }
}

/// Open the sheet on the core's Claude subscription tile with LABEL as its name and TIER chosen when offered
/// ([`offered`]; else the first): a gateway login's `Move to my accounts`, at the login's priority, with EMAIL filled
/// in on Claude's sign-in page (`claude auth login --email`). The same person signs in again by Claude Code's own
/// sign-in in a new account's folder: nothing of the gateway's login is read or copied.
pub fn open_claude(s: &mut App, label: &str, tier: i64, email: Option<&str>) {
    open_sheet(s);
    let at = offered(s, CLAUDE).iter().position(|(_, t)| *t == tier).unwrap_or(0);
    let add = &mut s.ext.get_mut::<Accounts>().add;
    (add.label, add.tier, add.email) = (TextEdit::new(label), at, email.map(str::to_string));
    pick(s, Pick::Claude);
}

/// The account the open sheet made (its id), until the sheet closes: an extension's `leave` reads it there (the
/// gateway's Move, which follows that account's sign-in on its own card). None with no account made, or the sheet
/// closed.
pub fn sheet_made(s: &App) -> Option<String> {
    s.ext.try_get::<Accounts>()?.add.made.as_ref().map(|(id, _)| id.clone())
}

/// Whether the Add account sheet is open.
pub fn sheet_open(s: &App) -> bool {
    s.ext.try_get::<Accounts>().is_some_and(|a| a.add.open)
}

/// Close the sheet: what was typed goes (a key with it), the extensions are told, and the accounts are asked again.
/// An account already made keeps its sign-in: its card carries it.
pub fn close_sheet(s: &mut App) {
    for a in ext::all().iter().filter_map(|e| e.accounts.as_ref()) {
        (a.leave)(s, true);
    }
    let st = s.ext.get_mut::<Accounts>();
    st.add = Add::default();
    st.asked = None;
}

/// Leave the flow of the tile picked, if it is an extension's.
fn leave(s: &mut App) {
    if let Some(Pick::Ext(name, ..)) = s.ext.get::<Accounts>().add.pick.clone()
        && let Some(a) = ext::get(name).and_then(|e| e.accounts.as_ref())
    {
        (a.leave)(s, false);
    }
}

/// Back to the tiles: the name typed stays, a key typed does not; an account already made keeps its sign-in.
fn back(s: &mut App) {
    leave(s);
    let st = s.ext.get_mut::<Accounts>();
    st.add = Add { open: true, label: std::mem::take(&mut st.add.label), ..Add::default() };
    st.asked = None;
}

/// Pick one of the core's tiles.
fn pick(s: &mut App, p: Pick) {
    let st = s.ext.get_mut::<Accounts>();
    st.add.pick = Some(p);
    st.add.error = None;
}

/// Pick the `opencode` tile's provider I: a key typed for another provider is emptied out of the field, so it is never
/// handed to this one's sign-in.
fn pick_provider(s: &mut App, i: usize) {
    let add = &mut s.ext.get_mut::<Accounts>().add;
    if add.provider != i {
        add.secret = TextEdit::new("");
    }
    add.provider = i;
}

/// Pick extension NAME's tile T: its flow starts.
fn pick_ext(s: &mut App, name: &'static str, t: &AccountTile) {
    let st = s.ext.get_mut::<Accounts>();
    st.add = Add { open: true, pick: Some(Pick::Ext(name, t.id.clone(), t.label.clone())), ..Add::default() };
    if let Some(a) = ext::get(name).and_then(|e| e.accounts.as_ref()) {
        (a.pick)(s, &t.id);
    }
}

/// The tiers a new account of HARNESS is offered, by the words the sheet shows and their numbers: those the overview
/// shows under HARNESS (`Tier 1` the one taken first), then a new one under the last; one tier, the first, while it
/// has none. Another harness's tiers are not offered: an agent takes accounts of its own harness alone.
pub(super) fn offered(s: &App, harness: &str) -> Vec<(String, i64)> {
    offered_of(&tiers(s, harness))
}

/// The harness an account made from tile PICK is of: Claude Code's for a subscription, an Anthropic key or an
/// extension's tile; opencode's; agy's for a Google sign-in or a Gemini key.
pub(super) fn harness_of(pick: &Pick) -> &'static str {
    match pick {
        Pick::Claude | Pick::Anthropic | Pick::Ext(..) => CLAUDE,
        Pick::Opencode => "opencode",
        Pick::Agy | Pick::AgyKey => "agy",
    }
}

/// [`offered`] for the overview's tiers ALL, highest first.
pub(super) fn offered_of(all: &[i64]) -> Vec<(String, i64)> {
    let Some(last) = all.last().copied() else { return vec![("Tier 1".to_string(), FIRST_TIER)] };
    let mut o: Vec<(String, i64)> = all.iter().enumerate().map(|(i, t)| (format!("Tier {}", i + 1), *t)).collect();
    // under the last: ten lower, as the levels are numbered (30, 20, 10); under 10, half
    let under = if last > 10 { Some(last - 10) } else if last > 1 { Some(last / 2) } else { None };
    if let Some(t) = under {
        o.push((format!("Tier {} · new", all.len() + 1), t));
    }
    o
}

/// The tier of the first account ever added.
const FIRST_TIER: i64 = 30;

/// The extensions' tiles, each with its extension's name.
fn ext_tiles(s: &App) -> Vec<(&'static str, AccountTile)> {
    ext::all().iter().filter_map(|e| Some((e.name, e.accounts.as_ref()?))).flat_map(|(name, a)| (a.tiles)(s).into_iter().map(move |t| (name, t))).collect()
}

/// The heading of the core's own row of subscriptions.
const BUILT_IN: &str = "Built-in subscriptions";

/// A subscription tile of the sheet: the core's own (Claude Code's, opencode's, Antigravity's), or extension NAME's.
enum Sub<'a> {
    ClaudeCode,
    Opencode,
    Antigravity,
    Theirs(&'static str, &'a AccountTile),
}

/// The sheet's rows of subscriptions, each with its heading: the core's own first, `Claude Code` where the project's
/// coordd makes accounts (SERVED), then `opencode` (OPENCODE), then `Antigravity` (AGY); then a row per extension that
/// offers subscription tiles, under its HEADING, in its order. A row with no tile has no heading.
fn subscription_rows<'a>(served: bool, opencode: bool, agy: bool, theirs: &'a [(&'static str, AccountTile)], heading: impl Fn(&'static str) -> &'static str) -> Vec<(&'static str, Vec<Sub<'a>>)> {
    let subs = || theirs.iter().filter(|(_, t)| t.group == TileGroup::Subscriptions);
    let mut exts: Vec<&'static str> = subs().map(|(name, _)| *name).collect();
    exts.dedup();
    let mut rows = Vec::new();
    let built_in: Vec<Sub> = [(served, Sub::ClaudeCode), (opencode, Sub::Opencode), (agy, Sub::Antigravity)].into_iter().filter_map(|(on, sub)| on.then_some(sub)).collect();
    if !built_in.is_empty() {
        rows.push((BUILT_IN, built_in));
    }
    rows.extend(exts.into_iter().map(|ext| (heading(ext), subs().filter(|(name, _)| *name == ext).map(|(name, t)| Sub::Theirs(name, t)).collect())));
    rows
}

/// A tile: the provider's logo and name, a few faint words where the name does not say what is asked for.
/// ELSEWHERE: its steps are not in this window; it is drawn dimmed and takes no click.
fn tile(k: &Kit, logo: Option<&'static rune::Icon>, label: &str, note: Option<&str>, elsewhere: bool, on: impl Fn(&mut App) + 'static) -> El {
    let words = col().gap(1.0).grow(1.0).child(text(label.to_string()).font(k.ty.ui_medium).color(k.c.text_base).wrap()).children(note.map(|n| text(n.to_string()).font(k.ty.small).color(k.c.text_muted).wrap()));
    let mark = match logo {
        Some(l) => icon(l, 28.0).color(k.c.text_base),
        None => el().w(28.0).h(28.0).radius(k.radius.sm).fill(k.c.layer_02),
    };
    let face = row().gap(12.0).center_y().padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak).child(mark).child(words).w(170.0).min_h(54.0);
    if elsewhere { face.opacity(0.4).label(format!("{label}, not in this window")) } else { k.interactive(face).label(label.to_string()).on_press(on) }
}

/// A row of the sheet: its heading and its tiles.
fn group(k: &Kit, name: &str, tiles: Vec<El>) -> El {
    col().w_full().gap(8.0).child(text(name.to_string()).font(k.ty.small_medium).color(k.c.text_muted)).child(row().w_full().gap(10.0).flex_wrap().label(name.to_string()).children(tiles))
}

/// The sheet, on the layer, while it is open.
pub fn sheet(s: &App, k: &Kit, layer: &mut Vec<El>) {
    if ext::get(NAME).is_none() {
        return;
    }
    let st = s.ext.get::<Accounts>();
    if !st.add.open {
        return;
    }
    let mut body = col().w_full().gap(14.0).px(16.0).pb(16.0);
    // the tiles and an extension's steps take the sheet's full width; the core's two steps are a short form
    let mut width = 760.0;
    let title = match &st.add.pick {
        None => {
            let theirs = ext_tiles(s);
            let their = |name: &'static str, t: &AccountTile| -> El {
                let t2 = t.clone();
                tile(k, t.logo, &t.label, t.note, t.elsewhere, move |s: &mut App| pick_ext(s, name, &t2))
            };
            // the subscriptions: the core's own, `Claude Code`, `opencode` and `Antigravity`, in a row by themselves,
            // then each extension's in its row
            let heading = |name: &'static str| ext::get(name).and_then(|e| e.accounts.as_ref()).map_or("Subscriptions", |a| a.subscriptions);
            for (name, subs) in subscription_rows(s.proj().is_some_and(served), opencode_served(s), agy_served(s), &theirs, heading) {
                let tiles = subs
                    .into_iter()
                    .map(|sub| match sub {
                        Sub::ClaudeCode => tile(k, Some(&logos::CLAUDE), "Claude Code", None, false, |s: &mut App| pick(s, Pick::Claude)),
                        Sub::Opencode => tile(k, Some(&logos::OPENCODE), "opencode", None, false, |s: &mut App| pick(s, Pick::Opencode)),
                        Sub::Antigravity => tile(k, Some(&logos::ANTIGRAVITY), "Antigravity", Some("Google sign-in, agy's own"), false, |s: &mut App| pick(s, Pick::Agy)),
                        Sub::Theirs(ext, t) => their(ext, t),
                    })
                    .collect();
                body = body.child(group(k, name, tiles));
            }
            // the API keys stay one row, the core's `Claude Code` (on his API key) first in it, then `Antigravity` (on
            // a Gemini API key)
            let mut keys = Vec::new();
            if keys_served(s) {
                keys.push(tile(k, Some(&logos::CLAUDE), "Claude Code", None, false, |s: &mut App| pick(s, Pick::Anthropic)));
            }
            if agy_served(s) {
                keys.push(tile(k, Some(&logos::ANTIGRAVITY), "Antigravity", Some("Gemini API key"), false, |s: &mut App| pick(s, Pick::AgyKey)));
            }
            keys.extend(theirs.iter().filter(|(_, t)| t.group == TileGroup::ApiKeys).map(|(name, t)| their(name, t)));
            if !keys.is_empty() {
                body = body.child(group(k, "API keys", keys));
            }
            "Add account".to_string()
        }
        Some(p) => {
            // back to the tiles, until the account is in: then the step's own two buttons are the ways on
            let core = !matches!(p, Pick::Ext(..));
            // `is_in` by its key for an Anthropic or a Gemini key; an opencode account is in when opencode keeps a
            // sign-in for it
            if !(core && st.add.made.as_ref().is_some_and(|(id, _)| is_in(s, id, keyed_pick(p)))) {
                body = body.child(row().child(Button::new("All providers").icon(&BACK).variant(ButtonVariant::Ghost).control_size(ControlSize::Regular).on_press(back).build(k)));
            }
            if core {
                width = 520.0;
            }
            match p {
                Pick::Ext(name, id, label) => {
                    if let Some(a) = ext::get(name).and_then(|e| e.accounts.as_ref()) {
                        body = body.child((a.flow)(s, k, id, layer));
                    }
                    label.clone()
                }
                core => {
                    body = body.child(match &st.add.made {
                        Some((id, label)) => made(s, k, id, label, core),
                        None => form(s, k, core),
                    });
                    let name = match core {
                        Pick::Opencode => "opencode",
                        Pick::Agy | Pick::AgyKey => "Antigravity",
                        _ => "Claude Code",
                    };
                    st.add.made.as_ref().map_or(name.to_string(), |(_, label)| label.clone())
                }
            }
        }
    };
    if st.add.pick.is_none() {
        body = body.children(ext::all().iter().filter_map(|e| (e.accounts.as_ref()?.note)(s)).map(|n| faint(k, n)));
    }
    // a click outside closes it only on the tiles: a step under way is left by its own buttons
    Dialog::new("accounts-sheet", title).open(true).fit().width(width).click_outside(st.add.pick.is_none()).on_close(close_sheet).child(body).build(k, layer);
}

/// Whether tile P's account goes on a key typed in the sheet: an Anthropic or a Gemini API key.
fn keyed_pick(p: &Pick) -> bool {
    matches!(p, Pick::Anthropic | Pick::AgyKey)
}

/// The step of one of the core's tiles before the account is made: its name, its tier, for an opencode account its
/// provider, for a key its key; then the one button that makes it and goes on (for an opencode provider with a browser
/// way, a second one: that way instead of a key; for one that takes no key, OpenAI, that way alone).
fn form(s: &App, k: &Kit, p: &Pick) -> El {
    let st = s.ext.get::<Accounts>();
    let (accounts, _) = shown(s);
    let gone = removed_all(&st.got);
    let (key, oc) = (keyed_pick(p), *p == Pick::Opencode);
    let provider = &OPENCODE_PROVIDERS[st.add.provider.min(OPENCODE_PROVIDERS.len() - 1)];
    let mut c = col().w_full().gap(12.0);
    let label = |t: &str| text(t.to_string()).font(k.ty.small).color(k.c.text_muted).w(64.0);
    let placeholder = match p {
        Pick::Anthropic => "Work key",
        Pick::AgyKey => "Gemini key",
        Pick::Opencode => "Go plan",
        Pick::Agy => "Google AI Pro",
        _ => "Work (Max)",
    };
    let name = TextInput::new(&st.add.label).placeholder(placeholder).label("Name").width(None).on_edit(|s: &mut App| &mut s.ext.get_mut::<Accounts>().add.label).on_submit(|s: &mut App, _| add(s)).build(k);
    c = c.child(row().w_full().gap(8.0).center_y().child(label("Name")).child(name.grow(1.0)));
    // the tier, where there is one to choose
    let offered = offered(s, harness_of(p));
    if offered.len() > 1 {
        let names: Vec<&str> = offered.iter().map(|t| t.0.as_str()).collect();
        let tier = SegmentedControl::new(&names, Some(st.add.tier.min(offered.len() - 1))).label("Tier").fit().on_pick(|s: &mut App, i, _| s.ext.get_mut::<Accounts>().add.tier = i).build(k);
        c = c.child(row().w_full().gap(8.0).center_y().child(label("Tier")).child(tier));
    }
    if oc {
        let names: Vec<&str> = OPENCODE_PROVIDERS.iter().map(|p| p.name).collect();
        let pick = SegmentedControl::new(&names, Some(st.add.provider.min(names.len() - 1))).label("Provider").fit().on_pick(|s: &mut App, i, _| pick_provider(s, i)).build(k);
        c = c.child(row().w_full().gap(8.0).center_y().child(label("Provider")).child(pick));
    }
    // opencode's provider that takes no key (OpenAI) has no field for one
    let takes_key = key || (oc && provider.key_from.is_some());
    if takes_key {
        let field = TextInput::new(&st.add.secret).secure().placeholder("API key").label("API key").width(None).on_edit(|s: &mut App| &mut s.ext.get_mut::<Accounts>().add.secret).on_submit(|s: &mut App, _| add(s)).build(k);
        c = c.child(row().w_full().gap(8.0).center_y().child(label("API key")).child(field.grow(1.0)));
    }
    c = c.child(faint(
        k,
        if *p == Pick::AgyKey {
            "From aistudio.google.com → API keys".to_string()
        } else if key {
            "From console.anthropic.com → API keys".to_string()
        } else if *p == Pick::Agy {
            "Sign in with Google in your browser".to_string()
        } else if oc {
            // OpenCode Go's key is kept by yaaihh too, for Go's usage page alone, by a coordd that reads it
            match provider.key_from {
                Some(from) if provider.id == "opencode-go" && s.proj().is_some_and(|p| p.protocol >= coord_types::GO_USAGE_PROTOCOL) => {
                    format!("{from} · also kept to read usage")
                }
                Some(from) => from.to_string(),
                None => "Sign in with ChatGPT in your browser".to_string(),
            }
        } else {
            "Sign in with Claude Code".to_string()
        },
    ));
    // what signing in does, only against a coordd that does it and only while the switch was never set; an opencode
    // account takes no Claude Code agent
    if *p == Pick::Claude && s.proj().is_some_and(|p| p.protocol >= coord_types::ACCOUNTS_ALL) && moves_on_sign_in(&st.got) {
        c = c.child(faint(k, "Agents move here after their turn."));
    }
    if let Some(e) = &st.add.error {
        c = c.child(needs_you(k, e.clone()));
    }
    let named = !id_for(st.add.label.text.trim(), &taken(&accounts, &gone)).is_empty();
    let typed = !st.add.secret.text.trim().is_empty();
    let ok = named && (!takes_key || typed);
    let mut buttons = row().w_full().gap(8.0);
    if !oc || takes_key {
        let words = match p {
            _ if takes_key => "Add",
            Pick::Agy => "Sign in with Google",
            _ => "Sign in with Claude",
        };
        buttons = buttons.child(Button::new(words).variant(ButtonVariant::Contrast).disabled(!ok).on_press(add).build(k));
    }
    // the provider's browser way: no key goes with it, so a key typed is emptied out of the field first
    if let Some((_, words)) = provider.way.filter(|_| oc) {
        let way = Button::new(words);
        buttons = buttons.child(
            (if takes_key { way } else { way.variant(ButtonVariant::Contrast) })
                .disabled(!named)
                .on_press(|s: &mut App| {
                    s.ext.get_mut::<Accounts>().add.secret = TextEdit::new("");
                    add(s);
                })
                .build(k),
        );
    }
    c.child(buttons)
}

/// Whether account ID, made in the sheet, is in: signed in, or (KEY) its key set.
fn is_in(s: &App, id: &str, key: bool) -> bool {
    shown(s).0.iter().any(|a| a.id == id && if key { a.key_since.is_some() } else { a.signed_in })
}

/// The step after the account is sent: its sign-in or its key under way, what went wrong, or that it is in.
fn made(s: &App, k: &Kit, id: &str, label: &str, p: &Pick) -> El {
    let st = s.ext.get::<Accounts>();
    let (accounts, _) = shown(s);
    let account = accounts.iter().find(|a| a.id == id);
    let (key, oc, agy) = (keyed_pick(p), *p == Pick::Opencode, *p == Pick::Agy);
    let mut c = col().w_full().gap(12.0);
    // it is in: where it stands, and out
    if let Some(a) = account.filter(|_| is_in(s, id, key)) {
        let place = tiers(s, &a.harness).iter().position(|t| *t == a.tier).map_or(String::new(), |i| format!(" {label} is in Tier {}.", i + 1));
        return c
            .child(row().gap(6.0).center_y().child(icon(k.glyph.check, 16.0).color(k.c.success.icon)).child(text(if key { "Key set" } else { "Signed in" }).font(k.ty.ui_medium).color(k.c.text_base)))
            .child(faint(k, place.trim().to_string()))
            .child(row().gap(8.0).child(Button::new("Done").variant(ButtonVariant::Contrast).on_press(close_sheet).build(k)).child(Button::new("Add another").on_press(open_sheet).build(k)));
    }
    let said = st.said.get(id).and_then(|r| r.as_ref().err());
    if let Some(e) = said {
        c = c.child(needs_you(k, e.clone()));
    }
    let i = id.to_string();
    let provider = account.and_then(oc_provider);
    // where its key is made; None for a provider that takes no key (before the account is listed: the field's words)
    let from = account.map_or(Some("Its API key:"), oc_key_from);
    match (st.signin.get(id).filter(|_| !key), st.keys.get(id).filter(|_| key || (oc && from.is_some()))) {
        (Some(si), _) => {
            c = c.child(signing_in(k, id, si, true));
            if si.url.is_some() {
                // t3code's waits for the code pasted first: its field says so
                if !agy || si.sending {
                    c = c.child(faint(k, format!("Waiting for {}…", si.program())));
                }
                c = c.child(faint(k, "Closing keeps the sign-in going."));
            }
        }
        // its key on its way into opencode's sign-in, which Cancel ends
        _ if oc && st.setting.contains_key(id) => c = c.child(setting_key(k, id, st.setting.get(id) == Some(&true))),
        // the key was refused, or cancelled: its field again
        (_, Some(field)) => c = c.child(faint(k, if oc { from.unwrap_or("Its API key:") } else if *p == Pick::AgyKey { GEMINI_KEY_HINT } else { KEY_HINT })).child(key_field(k, id, field, false)),
        // an opencode account whose key or sign-in ended without signing in (coordd said why above): its key field
        // where its provider takes a key, and its provider's browser way where there is one
        _ if oc && account.is_some() && st.said.get(id).is_some_and(|r| !matches!(r, Ok(t) if t == KEY_SET)) => {
            if let Some(from) = from {
                let empty = TextEdit::new("");
                c = c.child(faint(k, from)).child(key_field(k, id, &empty, false));
            }
            if let Some((way, words)) = provider.and_then(|p| p.way) {
                let again = Button::new(words);
                c = c.child(row().child((if from.is_some() { again } else { again.variant(ButtonVariant::Contrast) }).on_press(move |s: &mut App| sign_in(s, &i, Whose::Opencode(way))).build(k)));
            }
        }
        // the sign-in ended without signing in (cancelled, or coordd said why above), or one still runs that a window
        // swap left (the engine answers its link again): from here
        _ if !key && !oc && account.is_some() => {
            let (words, whose) = if agy { ("Sign in with Google", Whose::Agy) } else { ("Sign in with Claude", Whose::Claude) };
            c = c.child(row().child(Button::new(words).variant(ButtonVariant::Contrast).on_press(move |s: &mut App| sign_in(s, &i, whose)).build(k)));
        }
        _ if said.is_none() => c = c.child(faint(k, if (key || oc) && account.is_some() { "Setting the key…" } else { "Making the account…" })),
        _ => {}
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new account is offered the tiers the overview shows, by their place, and one new tier under the last; the
    /// first account ever has no choice to make.
    #[test]
    fn a_new_account_is_offered_the_tiers_shown_and_one_under_them() {
        let named = |o: &[(&str, i64)]| -> Vec<(String, i64)> { o.iter().map(|(n, t)| (n.to_string(), *t)).collect() };
        assert_eq!(offered_of(&[30, 20, 10]), named(&[("Tier 1", 30), ("Tier 2", 20), ("Tier 3", 10), ("Tier 4 · new", 5)]));
        assert_eq!(offered_of(&[30]), named(&[("Tier 1", 30), ("Tier 2 · new", 20)]));
        assert_eq!(offered_of(&[1]), named(&[("Tier 1", 1)]), "nothing under 1");
        assert_eq!(offered_of(&[]), named(&[("Tier 1", FIRST_TIER)]));
    }

    /// A gateway login's `Move to my accounts` opens the sheet on Claude Code's sign-in with its name, and its priority's
    /// tier when offered, else the first. A unit test registers no extension, so the sheet offers its first tier alone.
    #[test]
    fn a_gateway_logins_move_opens_claude_with_its_name_and_its_tier() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        open_claude(&mut s, "Team login", FIRST_TIER, None);
        let add = &s.ext.get::<Accounts>().add;
        assert_eq!((add.open, add.pick.clone(), add.label.text.as_str(), offered(&s, CLAUDE)[add.tier].1), (true, Some(Pick::Claude), "Team login", FIRST_TIER));
        s.ext.get_mut::<Accounts>().add.tier = 3;
        open_claude(&mut s, "", 7, None);
        assert_eq!(s.ext.get::<Accounts>().add.tier, 0, "a priority the sheet does not offer: its first");
    }

    /// A gateway login's move fills its address in on Claude's sign-in page: the sheet's `AccountSignin` for the account it
    /// made carries it as `email`, and the extension reads which account the sheet made until it closes. A card's sign-in of
    /// another account carries none.
    #[test]
    fn a_moved_logins_sign_in_fills_in_its_address() {
        let (_held, mut s, child) = wired();
        open_claude(&mut s, "Personal", FIRST_TIER, Some("pe@example.com"));
        add(&mut s);
        assert_eq!(sheet_made(&s).as_deref(), Some("personal"));
        reply(&mut s, 0, Box::new(Act("personal".into(), What::New { signs: Some(Whose::Claude) })), Ok("account personal: made, not signed in yet".into()));
        sign_in(&mut s, "other", Whose::Claude);
        let reqs = sent(&mut s, child);
        let signins: Vec<(Option<&str>, Option<&str>)> = reqs.iter().filter(|r| r["op"] == "accountsignin").map(|r| (r["id"].as_str(), r["email"].as_str())).collect();
        assert_eq!(signins, [(Some("personal"), Some("pe@example.com")), (Some("other"), None)], "{reqs:?}");
        assert!(sheet_open(&s));
        close_sheet(&mut s);
        assert_eq!((sheet_made(&s), sheet_open(&s)), (None, false));
    }

    /// A window swap while Claude Code's sign-in waits on his browser: the new window's sheet comes back on the account
    /// already made (`made`, whose `Sign in with Claude` asks for that account's running sign-in, so the code his browser
    /// shows still fits), not on its form, so no second account of the name is made. (The sheet is drawn only where the
    /// accounts extension is registered, which a unit test does not do.)
    #[test]
    fn a_swap_mid_sign_in_brings_the_sheet_back_on_the_account_it_made() {
        let (held, mut s, child) = wired();
        open_sheet(&mut s);
        pick(&mut s, Pick::Claude);
        s.ext.get_mut::<Accounts>().add.label = TextEdit::new("Work");
        add(&mut s);
        let kept = (add_kept(&s), add_key_kept(&s), add_made_kept(&s));
        assert_eq!(sent(&mut s, child)[0]["op"], "accountnew");
        drop((s, held));
        // the new window, in state.rs `retake`'s order
        let (_held, mut s, child) = wired();
        let (Some((open, label, tier)), key, made) = kept else { panic!("the sheet is kept: {kept:?}") };
        add_put_back(&mut s, open, &label, tier);
        add_key_put_back(&mut s, key);
        add_made_put_back(&mut s, made);
        let back = &s.ext.get::<Accounts>().add;
        assert_eq!((back.open, back.pick.clone(), back.made.clone()), (true, Some(Pick::Claude), Some(("work".to_string(), "Work".to_string()))), "the account's step, not the form");
        add(&mut s);
        assert_eq!(sent(&mut s, child), Vec::<serde_json::Value>::new(), "no second account");
    }

    /// An extension's tile, made up.
    fn theirs(ext: &'static str, group: TileGroup, label: &str) -> (&'static str, AccountTile) {
        (ext, AccountTile { id: label.to_lowercase(), group, label: label.to_string(), note: None, logo: None, elsewhere: false })
    }

    /// The rows by their headings and their tiles' names.
    fn named<'a>(rows: &'a [(&'static str, Vec<Sub<'a>>)]) -> Vec<(&'static str, Vec<&'a str>)> {
        let name = |s: &'a Sub<'a>| match s {
            Sub::ClaudeCode => "Claude Code",
            Sub::Opencode => "opencode",
            Sub::Antigravity => "Antigravity",
            Sub::Theirs(_, t) => t.label.as_str(),
        };
        rows.iter().map(|(heading, subs)| (*heading, subs.iter().map(name).collect())).collect()
    }

    fn gateway(_: &'static str) -> &'static str {
        "Gateway subscriptions"
    }

    /// The subscription rows: the built-in one holds what the project's coordd makes (Claude Code, then opencode, then
    /// Antigravity), each extension's subscription tiles a row under its own heading, a key tile in none, and no row
    /// that would be empty.
    #[test]
    fn the_subscription_rows_hold_the_built_in_harnesses_served_then_each_extensions_tiles() {
        let claude = || theirs("proxy", TileGroup::Subscriptions, "Claude");
        let tiles = [claude(), theirs("proxy", TileGroup::Subscriptions, "Codex"), theirs("proxy", TileGroup::ApiKeys, "Codex API"), theirs("proxy", TileGroup::Subscriptions, "Kimi")];
        let rows = subscription_rows(true, false, false, &tiles, gateway);
        assert_eq!(named(&rows), vec![("Built-in subscriptions", vec!["Claude Code"]), ("Gateway subscriptions", vec!["Claude", "Codex", "Kimi"])]);
        assert!(matches!(rows[0].1[..], [Sub::ClaudeCode]) && rows[1].1.iter().all(|s| matches!(s, Sub::Theirs("proxy", _))));
        let keys_only = [theirs("proxy", TileGroup::ApiKeys, "Codex API")];
        assert_eq!(named(&subscription_rows(true, false, false, &keys_only, gateway)), vec![("Built-in subscriptions", vec!["Claude Code"])]);
        assert_eq!(named(&subscription_rows(true, true, true, &[claude()], gateway)), vec![("Built-in subscriptions", vec!["Claude Code", "opencode", "Antigravity"]), ("Gateway subscriptions", vec!["Claude"])]);
        assert_eq!(named(&subscription_rows(false, true, false, &[], gateway)), vec![("Built-in subscriptions", vec!["opencode"])]);
        assert_eq!(named(&subscription_rows(false, false, true, &[], gateway)), vec![("Built-in subscriptions", vec!["Antigravity"])]);
        assert_eq!(named(&subscription_rows(false, false, false, &[claude()], gateway)), vec![("Gateway subscriptions", vec!["Claude"])]);
        assert!(subscription_rows(false, false, false, &[], gateway).is_empty());
        let two = [claude(), theirs("other", TileGroup::Subscriptions, "Made-up")];
        let heading = |ext: &'static str| if ext == "proxy" { "Gateway subscriptions" } else { "Other subscriptions" };
        assert_eq!(named(&subscription_rows(true, false, false, &two, heading)), vec![("Built-in subscriptions", vec!["Claude Code"]), ("Gateway subscriptions", vec!["Claude"]), ("Other subscriptions", vec!["Made-up"])]);
    }

    /// A key typed in the sheet is in its field only: back to the tiles or the sheet closed, no field holds it; the
    /// name typed stays over a step back and goes with the sheet.
    #[test]
    fn a_key_typed_in_the_sheet_goes_when_its_step_is_left() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        open_sheet(&mut s);
        pick(&mut s, Pick::Anthropic);
        let type_in = |s: &mut App| {
            let st = s.ext.get_mut::<Accounts>();
            st.add.label = TextEdit::new("Work key");
            st.add.secret = TextEdit::new("sk-made-up-0000");
        };
        type_in(&mut s);
        back(&mut s);
        let st = s.ext.get::<Accounts>();
        assert!(st.add.open && st.add.pick.is_none());
        assert_eq!((st.add.label.text.as_str(), st.add.secret.text.as_str()), ("Work key", ""));
        pick(&mut s, Pick::Anthropic);
        type_in(&mut s);
        close_sheet(&mut s);
        let st = s.ext.get::<Accounts>();
        assert!(!st.add.open);
        assert_eq!((st.add.label.text.as_str(), st.add.secret.text.as_str()), ("", ""));
    }

    /// A new account's answer that comes after the sheet closed takes nothing from the sheet: a key account gets no
    /// sign-in, its card asks for the key; a sheet reopened on another key is not emptied into it.
    #[test]
    fn an_answer_after_the_sheet_closed_takes_nothing_from_it() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        open_sheet(&mut s);
        pick(&mut s, Pick::Anthropic);
        s.ext.get_mut::<Accounts>().add.secret = TextEdit::new("sk-made-up-1111");
        reply(&mut s, 0, Box::new(Act("work-key".into(), What::New { signs: None })), Ok("account work-key made".into()));
        let st = s.ext.get::<Accounts>();
        assert!(st.signin.is_empty(), "a key account is not signed in");
        assert!(st.keys.get("work-key").is_some_and(|f| f.text.is_empty()), "its card asks for the key");
        assert_eq!(st.add.secret.text, "sk-made-up-1111", "the key typed for another account stays in its field");
        assert!(!format!("{:?}", st.said).contains("sk-made-up"));
    }

    /// The sheet that waits gives its key to the account once, and holds none after.
    #[test]
    fn the_sheet_that_waits_gives_its_key_once() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        open_sheet(&mut s);
        pick(&mut s, Pick::Anthropic);
        let st = s.ext.get_mut::<Accounts>();
        st.add.secret = TextEdit::new("sk-made-up-2222");
        st.add.made = Some(("work-key".into(), "Work key".into()));
        reply(&mut s, 0, Box::new(Act("work-key".into(), What::New { signs: None })), Ok("account work-key made".into()));
        let st = s.ext.get::<Accounts>();
        assert_eq!(st.add.secret.text, "");
        assert!(st.keys.get("work-key").is_none_or(|f| f.text.is_empty()), "moved out to be sent, in no field after");
        assert!(st.signin.is_empty());
        assert!(!format!("{:?}", st.said).contains("sk-made-up"));
    }

    /// The window of these tests with a coordd behind it: one project at this protocol, its requests caught by `cat`
    /// (read back by [`sent`]), the page's state fresh.
    fn wired() -> (std::sync::MutexGuard<'static, ()>, App, std::process::Child) {
        let (held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.projects[0].protocol = coord_types::PROTOCOL;
        s.cur = 0;
        s.ext.insert(Accounts::default());
        let mut child = std::process::Command::new("cat").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
        s.projects[0].link = Some(crate::link::Link::to_child(child.stdin.take().unwrap()));
        (held, s, child)
    }

    /// The requests the window sent, in order, as written to coordd: the link is closed, so `cat` ends.
    fn sent(s: &mut App, child: std::process::Child) -> Vec<serde_json::Value> {
        s.projects[0].link = None;
        let out = child.wait_with_output().unwrap();
        String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap()["req"].clone()).collect()
    }

    /// The `opencode` tile makes an opencode account of his Go plan (`AccountNew` with its harness and provider, no key of
    /// Claude's), and the key typed goes once, in one `AccountKeyStore`, out of every field; nothing kept says it.
    #[test]
    fn an_opencode_account_is_made_for_its_provider_and_its_key_goes_once() {
        let (_held, mut s, child) = wired();
        assert!(opencode_served(&s));
        open_sheet(&mut s);
        pick(&mut s, Pick::Opencode);
        let st = s.ext.get_mut::<Accounts>();
        st.add.label = TextEdit::new("Go plan");
        st.add.secret = TextEdit::new("sk-made-up-go-3333");
        add(&mut s);
        assert_eq!(s.ext.get::<Accounts>().add.made, Some(("go-plan".into(), "Go plan".into())));
        reply(&mut s, 0, Box::new(Act("go-plan".into(), What::New { signs: None })), Ok("account go-plan: made, not signed in yet (opencode, opencode-go)".into()));
        let st = s.ext.get::<Accounts>();
        assert_eq!(st.add.secret.text, "");
        assert!(st.keys.get("go-plan").is_none_or(|f| f.text.is_empty()) && st.signin.is_empty());
        assert!(!format!("{:?}", st.said).contains("sk-made-up"));
        let reqs = sent(&mut s, child);
        assert_eq!(reqs.len(), 2, "{reqs:?}");
        assert_eq!((reqs[0]["op"].as_str(), reqs[0]["harness"].as_str(), reqs[0]["provider"].as_str(), reqs[0]["key"].as_bool()), (Some("accountnew"), Some("opencode"), Some("opencode-go"), Some(false)));
        assert_eq!((reqs[1]["op"].as_str(), reqs[1]["id"].as_str(), reqs[1]["key"].as_str()), (Some("accountkeystore"), Some("go-plan"), Some("sk-made-up-go-3333")));
        assert!(!reqs[0].to_string().contains("sk-made-up"), "the key is in its own request alone");
    }

    /// A provider with a browser way and no key typed signs in by that way (`AccountSignin` with its method); one with
    /// no such way is not made without its key; another provider picked empties the key typed for the last.
    #[test]
    fn an_opencode_account_without_a_key_signs_in_by_its_providers_way_or_is_not_made() {
        let (_held, mut s, child) = wired();
        open_sheet(&mut s);
        pick(&mut s, Pick::Opencode);
        s.ext.get_mut::<Accounts>().add.label = TextEdit::new("Go plan");
        add(&mut s);
        assert!(s.ext.get::<Accounts>().add.made.is_none(), "the Go plan signs in by its key alone");
        s.ext.get_mut::<Accounts>().add.secret = TextEdit::new("sk-made-up-zen-4444");
        pick_provider(&mut s, 2);
        let st = s.ext.get_mut::<Accounts>();
        assert_eq!((st.add.provider, st.add.secret.text.as_str()), (2, ""), "a key typed for another provider goes");
        st.add.label = TextEdit::new("ChatGPT");
        add(&mut s);
        reply(&mut s, 0, Box::new(Act("chatgpt".into(), What::New { signs: Some(Whose::Opencode("ChatGPT Pro/Plus (headless)")) })), Ok("account chatgpt: made, not signed in yet (opencode, openai)".into()));
        assert!(s.ext.get::<Accounts>().signin.get("chatgpt").is_some_and(|si| si.whose == Whose::Opencode("ChatGPT Pro/Plus (headless)")));
        let reqs = sent(&mut s, child);
        assert_eq!(reqs.len(), 2, "{reqs:?}");
        assert_eq!((reqs[0]["op"].as_str(), reqs[0]["provider"].as_str()), (Some("accountnew"), Some("openai")));
        assert_eq!((reqs[1]["op"].as_str(), reqs[1]["method"].as_str()), (Some("accountsignin"), Some("ChatGPT Pro/Plus (headless)")));
    }

    /// Below `OPENCODE_ACCOUNTS` a coordd would make a Claude Code account of the request: the tile is not offered.
    #[test]
    fn the_opencode_tile_needs_a_coordd_that_makes_opencode_accounts() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.cur = 0;
        s.projects[0].protocol = coord_types::OPENCODE_ACCOUNTS - 1;
        assert!(!opencode_served(&s));
        s.projects[0].protocol = coord_types::OPENCODE_ACCOUNTS;
        assert!(opencode_served(&s));
    }

    /// Cancel beside "Setting the key…" ends the key's run (`AccountCancel`, sent once however often pressed); the refusal
    /// that follows is the cancel's, so no fault is said, the account stays not signed in and its field is there again,
    /// empty; a second paste goes in its own `AccountKeyStore`.
    #[test]
    fn a_key_cancelled_while_opencode_starts_says_no_fault_and_takes_a_second_paste() {
        let (_held, mut s, child) = wired();
        open_sheet(&mut s);
        pick(&mut s, Pick::Opencode);
        let st = s.ext.get_mut::<Accounts>();
        st.add.label = TextEdit::new("Go plan");
        st.add.secret = TextEdit::new("sk-made-up-go-5555");
        add(&mut s);
        reply(&mut s, 0, Box::new(Act("go-plan".into(), What::New { signs: None })), Ok("account go-plan: made, not signed in yet (opencode, opencode-go)".into()));
        assert_eq!(s.ext.get::<Accounts>().setting.get("go-plan"), Some(&false), "setting the key, Cancel offered");
        cancel_key(&mut s, "go-plan");
        cancel_key(&mut s, "go-plan");
        assert_eq!(s.ext.get::<Accounts>().setting.get("go-plan"), Some(&true));
        reply(&mut s, 0, Box::new(Act("go-plan".into(), What::KeyCancel)), Ok("account go-plan: its sign-in is ended".into()));
        reply(&mut s, 0, Box::new(Act("go-plan".into(), What::Key)), Err("account go-plan: key not stored: cancelled before opencode asked for the key: nothing was typed into it".into()));
        let st = s.ext.get::<Accounts>();
        assert!(st.setting.is_empty() && !st.said.contains_key("go-plan"), "no fault said: {:?}", st.said);
        assert!(st.keys.get("go-plan").is_some_and(|f| f.text.is_empty()), "its field again, empty");
        s.ext.get_mut::<Accounts>().keys.insert("go-plan".into(), TextEdit::new("sk-made-up-go-6666"));
        send_key(&mut s, "go-plan");
        assert_eq!(s.ext.get::<Accounts>().setting.get("go-plan"), Some(&false), "the second paste is on its way");
        let reqs = sent(&mut s, child);
        let ops: Vec<&str> = reqs.iter().map(|r| r["op"].as_str().unwrap_or("")).collect();
        assert_eq!(ops, ["accountnew", "accountkeystore", "accountcancel", "accountkeystore"], "{reqs:?}");
        assert_eq!((reqs[2]["id"].as_str(), reqs[3]["key"].as_str()), (Some("go-plan"), Some("sk-made-up-go-6666")));
        assert!(!reqs[2].to_string().contains("sk-made-up"), "the cancel carries no key");
    }

    /// A key that coordd refuses with no Cancel pressed is said, as before; a Cancel that coordd refuses is said too,
    /// and the key's refusal after it is said as well (it was not the cancel's).
    #[test]
    fn a_keys_refusal_is_said_unless_its_cancel_went_through() {
        let (_held, mut s, child) = wired();
        for id in ["a", "b"] {
            s.ext.get_mut::<Accounts>().keys.insert(id.into(), TextEdit::new("sk-made-up-7777"));
            send_key(&mut s, id);
        }
        reply(&mut s, 0, Box::new(Act("a".into(), What::Key)), Err("account a: key not stored: opencode asks \"Login method\", not for an API key: nothing was typed into it".into()));
        cancel_key(&mut s, "b");
        reply(&mut s, 0, Box::new(Act("b".into(), What::KeyCancel)), Err("coord account cancel: coordd is busy".into()));
        assert_eq!(s.ext.get::<Accounts>().setting.get("b"), Some(&false));
        reply(&mut s, 0, Box::new(Act("b".into(), What::Key)), Err("account b: key not stored: opencode has ended".into()));
        let st = s.ext.get::<Accounts>();
        assert!(st.said.get("a").is_some_and(|r| r.as_ref().is_err_and(|e| e.contains("Login method"))), "{:?}", st.said);
        assert!(st.said.get("b").is_some_and(|r| r.as_ref().is_err_and(|e| e.contains("opencode has ended"))), "{:?}", st.said);
        assert!(st.setting.is_empty());
        drop(sent(&mut s, child));
    }

    /// A Cancel's refusal that comes after the key's own `key set` is not said in its place; the card keeps `key set`.
    #[test]
    fn a_cancels_refusal_after_the_key_is_set_leaves_key_set() {
        let (_held, mut s, child) = wired();
        s.ext.get_mut::<Accounts>().keys.insert("c".into(), TextEdit::new("sk-made-up-9999"));
        send_key(&mut s, "c");
        cancel_key(&mut s, "c");
        reply(&mut s, 0, Box::new(Act("c".into(), What::Key)), Ok("account c: key stored".into()));
        reply(&mut s, 0, Box::new(Act("c".into(), What::KeyCancel)), Err("coord account cancel: coordd is busy".into()));
        let st = s.ext.get::<Accounts>();
        assert_eq!(st.said.get("c"), Some(&Ok(KEY_SET.to_string())), "{:?}", st.said);
        assert!(st.setting.is_empty());
        drop(sent(&mut s, child));
    }

    /// opencode 1.18.34's `auth login -p openai` asks "Login method" first, not for a key, so OpenAI takes no key here:
    /// the sheet's secret goes into no request, and the account signs in by ChatGPT alone.
    #[test]
    fn openai_takes_no_key_and_signs_in_by_chatgpt_alone() {
        assert_eq!(OPENCODE_PROVIDERS.iter().map(|p| (p.id, p.key_from.is_some())).collect::<Vec<_>>(), [("opencode-go", true), ("opencode", true), ("openai", false)]);
        let (_held, mut s, child) = wired();
        open_sheet(&mut s);
        pick(&mut s, Pick::Opencode);
        pick_provider(&mut s, 2);
        let st = s.ext.get_mut::<Accounts>();
        st.add.label = TextEdit::new("ChatGPT");
        st.add.secret = TextEdit::new("sk-made-up-openai-8888");
        add(&mut s);
        assert_eq!(s.ext.get::<Accounts>().add.made, Some(("chatgpt".into(), "ChatGPT".into())));
        reply(&mut s, 0, Box::new(Act("chatgpt".into(), What::New { signs: Some(Whose::Opencode("ChatGPT Pro/Plus (headless)")) })), Ok("account chatgpt: made, not signed in yet (opencode, openai)".into()));
        assert!(s.ext.get::<Accounts>().keys.is_empty() && s.ext.get::<Accounts>().setting.is_empty());
        let reqs = sent(&mut s, child);
        assert_eq!(reqs.iter().map(|r| r["op"].as_str().unwrap_or("")).collect::<Vec<_>>(), ["accountnew", "accountsignin"], "{reqs:?}");
        assert_eq!((reqs[0]["provider"].as_str(), reqs[0]["key"].as_bool(), reqs[1]["method"].as_str()), (Some("openai"), Some(false), Some("ChatGPT Pro/Plus (headless)")));
        assert!(!format!("{reqs:?}").contains("sk-made-up"), "no request carries it");
    }

    /// Add with no key typed makes nothing: a key account is never made without its key (it would stand as a card
    /// that no agent can run on).
    #[test]
    fn a_key_account_is_not_made_without_its_key() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.ext.insert(Accounts::default());
        open_sheet(&mut s);
        pick(&mut s, Pick::Anthropic);
        s.ext.get_mut::<Accounts>().add.label = TextEdit::new("Work key");
        add(&mut s);
        let st = s.ext.get::<Accounts>();
        assert!(st.add.made.is_none() && st.said.is_empty());
        pick(&mut s, Pick::AgyKey);
        s.ext.get_mut::<Accounts>().add.label = TextEdit::new("Gemini key");
        add(&mut s);
        let st = s.ext.get::<Accounts>();
        assert!(st.add.made.is_none() && st.said.is_empty(), "nor a Gemini key account");
    }

    /// The `Antigravity` subscription tile makes an agy account (`AccountNew` with harness `agy`, no key, no provider) and
    /// goes on to agy's own Google sign-in (`AccountSignin` with no method); Google's address is shown to copy, never
    /// opened, and the code Google shows goes in by `AccountCode`. Nothing else of the sign-in passes the window.
    #[test]
    fn an_antigravity_account_signs_in_with_agys_own_google_sign_in() {
        let (_held, mut s, child) = wired();
        assert!(agy_served(&s));
        open_sheet(&mut s);
        pick(&mut s, Pick::Agy);
        s.ext.get_mut::<Accounts>().add.label = TextEdit::new("Google AI Pro");
        add(&mut s);
        assert_eq!(s.ext.get::<Accounts>().add.made, Some(("google-ai-pro".into(), "Google AI Pro".into())));
        reply(&mut s, 0, Box::new(Act("google-ai-pro".into(), What::New { signs: Some(Whose::Agy) })), Ok("account google-ai-pro: made, not signed in yet (agy)".into()));
        assert!(s.ext.get::<Accounts>().signin.get("google-ai-pro").is_some_and(|si| si.whose == Whose::Agy));
        reply(&mut s, 0, Box::new(Act("google-ai-pro".into(), What::Signin)), Ok("https://accounts.google.com/o/oauth2/v2/auth?client_id=x\nOpen it, sign in with Google, then paste the code Google shows.".into()));
        let si = &s.ext.get::<Accounts>().signin["google-ai-pro"];
        assert_eq!((si.url.as_deref(), si.said.as_deref()), (Some("https://accounts.google.com/o/oauth2/v2/auth?client_id=x"), None));
        assert!(si.known(si.url.as_deref().unwrap()));
        s.ext.get_mut::<Accounts>().signin.get_mut("google-ai-pro").unwrap().code = TextEdit::new("4/made-up-code");
        send_code(&mut s, "google-ai-pro");
        let reqs = sent(&mut s, child);
        assert_eq!(reqs.iter().map(|r| r["op"].as_str().unwrap_or("")).collect::<Vec<_>>(), ["accountnew", "accountsignin", "accountcode"], "{reqs:?}");
        assert_eq!((reqs[0]["harness"].as_str(), reqs[0]["key"].as_bool(), reqs[0].get("provider")), (Some("agy"), Some(false), None));
        assert_eq!((reqs[1]["id"].as_str(), reqs[1].get("method").filter(|m| !m.is_null())), (Some("google-ai-pro"), None));
        assert_eq!(reqs[2]["code"].as_str(), Some("4/made-up-code"));
    }

    /// The `Antigravity` key tile: an agy account on a Gemini API key, the key sent once in its own `AccountKeyStore` and
    /// kept in no field; a key whose models agy did not list is said, not taken for set.
    #[test]
    fn an_antigravity_key_account_takes_its_gemini_key_once() {
        let (_held, mut s, child) = wired();
        open_sheet(&mut s);
        pick(&mut s, Pick::AgyKey);
        let st = s.ext.get_mut::<Accounts>();
        st.add.label = TextEdit::new("Gemini key");
        st.add.secret = TextEdit::new("made-up-gemini-1234");
        add(&mut s);
        reply(&mut s, 0, Box::new(Act("gemini-key".into(), What::New { signs: None })), Ok("account gemini-key: made, no key yet (agy)".into()));
        let st = s.ext.get::<Accounts>();
        assert!(st.add.secret.text.is_empty() && st.keys.get("gemini-key").is_none_or(|f| f.text.is_empty()) && st.signin.is_empty());
        reply(&mut s, 0, Box::new(Act("gemini-key".into(), What::Key)), Ok("account gemini-key: key stored; key set; agy's models were not read: agy-rpc models: agy ended".into()));
        assert!(s.ext.get::<Accounts>().said.get("gemini-key").is_some_and(|r| r.as_ref().is_err_and(|e| e.starts_with("Key set; models not listed: "))));
        reply(&mut s, 0, Box::new(Act("gemini-key".into(), What::Key)), Ok("account gemini-key: key stored; key set; agy offers 3 models on it".into()));
        assert_eq!(s.ext.get::<Accounts>().said.get("gemini-key"), Some(&Ok(KEY_SET.to_string())));
        let reqs = sent(&mut s, child);
        assert_eq!(reqs.iter().map(|r| r["op"].as_str().unwrap_or("")).collect::<Vec<_>>(), ["accountnew", "accountkeystore"], "{reqs:?}");
        assert_eq!((reqs[0]["harness"].as_str(), reqs[0]["key"].as_bool()), (Some("agy"), Some(true)));
        assert_eq!(reqs[1]["key"].as_str(), Some("made-up-gemini-1234"));
        assert!(!reqs[0].to_string().contains("made-up-gemini"), "the key is in its own request alone");
    }

    /// Below `AGY_ACCOUNTS` a coordd would make a Claude Code account of the request: neither Antigravity tile is
    /// offered.
    #[test]
    fn the_antigravity_tiles_need_a_coordd_that_makes_agy_accounts() {
        let (_held, mut s) = crate::view_cost::test_app();
        s.projects = vec![crate::view_cost::project_for_tests()];
        s.cur = 0;
        s.projects[0].protocol = coord_types::AGY_ACCOUNTS - 1;
        assert!(!agy_served(&s));
        s.projects[0].protocol = coord_types::AGY_ACCOUNTS;
        assert!(agy_served(&s));
    }
}
