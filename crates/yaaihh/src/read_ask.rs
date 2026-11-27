//! An agent's read outside its folders, waiting on the owner (coordd's read_ask.rs, `AgentView.read_asks`): one card at
//! the top of its page, in every view of it, Allow read-only for this folder, Allow once or Deny, and one line under Waits on you that opens
//! that chat. His press is the waiting call's answer.

use crate::{App, Proj};
use coord_types::{AgentView, ReadAnswer, ReadAsk, Request};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};

/// Every read waiting on him in P, with its agent, oldest first.
pub fn asks(p: &Proj) -> Vec<(&str, &ReadAsk)> {
    let mut all: Vec<(&str, &ReadAsk)> = p.live.agents.iter().flat_map(|a| a.read_asks.iter().map(move |k| (a.name.as_str(), k))).collect();
    all.sort_by(|a, b| a.1.at.cmp(&b.1.at));
    all
}

/// PATH with a home's top as `~`, as he names his own folders.
pub fn short(path: &str) -> String {
    let mut parts = path.splitn(4, '/');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(""), Some("home"), Some(_), rest) => format!("~/{}", rest.unwrap_or("")).trim_end_matches('/').to_string(),
        (Some(""), Some("root"), rest, more) => format!("~/{}{}", rest.unwrap_or(""), more.map(|m| format!("/{m}")).unwrap_or_default()).trim_end_matches('/').to_string(),
        _ => path.to_string(),
    }
}

/// What agent NAME's ask K asks, in his words.
pub fn words(name: &str, k: &ReadAsk) -> String {
    format!("{name} wants read-only access to {}", short(k.folder.as_deref().unwrap_or(&k.path)))
}

fn answer(pi: usize, name: &str, id: u64, a: ReadAnswer, label: &str, variant: ButtonVariant, k: &Kit) -> El {
    let req = Request::ReadAnswer { name: name.to_string(), id, answer: a };
    let key = match a {
        ReadAnswer::Folder => "folder",
        ReadAnswer::Once => "once",
        ReadAnswer::Deny => "deny",
    };
    Button::new(label).variant(variant).on_press(move |s: &mut App| {
        s.request_in(pi, req.clone(), crate::Pending::Sent);
    })
    .build(k)
    .key(format!("read-{key}-{id}"))
}

/// The cards of agent A's reads that wait on him, under its page's head; none when none waits.
pub fn cards(pi: usize, a: &AgentView, k: &Kit) -> Option<El> {
    if a.read_asks.is_empty() {
        return None;
    }
    let card = |r: &ReadAsk| {
        let mut buttons = row().w_full().gap(6.0).flex_wrap();
        if r.folder.is_some() {
            buttons = buttons.child(answer(pi, &a.name, r.id, ReadAnswer::Folder, "Allow read-only for this folder", ButtonVariant::Contrast, k));
        }
        let once = if r.folder.is_some() { ButtonVariant::Outline } else { ButtonVariant::Contrast };
        buttons = buttons.child(answer(pi, &a.name, r.id, ReadAnswer::Once, "Allow once", once, k)).child(answer(pi, &a.name, r.id, ReadAnswer::Deny, "Deny", ButtonVariant::Ghost, k));
        let reads = if r.folder.is_some() { format!("It is reading {}", short(&r.path)) } else { "Only this file can be allowed: its folder is the top of a home or the machine".into() };
        col()
            .key(format!("read-ask-{}", r.id))
            .w_full()
            .gap(6.0)
            .padding(10.0)
            .radius(k.radius.md)
            .border(1.0, k.c.danger.fg)
            .fill(k.c.layer_01)
            .child(text(words(&a.name, r)).font(k.ty.body).color(k.c.text_base).wrap())
            .child(text(reads).font(k.ty.small).color(k.c.text_muted).wrap())
            .child(buttons)
    };
    Some(col().key(format!("read-asks-{pi}-{}", a.name)).w_full().px(24.0).pt(8.0).gap(6.0).children(a.read_asks.iter().map(card)))
}

/// The lines under Waits on you on Threads for P's reads, each opening the chat its card is in.
pub fn lines(p: &Proj, k: &Kit) -> Vec<El> {
    asks(p)
        .into_iter()
        .map(|(n, r)| {
            let name = n.to_string();
            k.interactive(
                row()
                    .key(format!("need-read-{}", r.id))
                    .w_full()
                    .gap(10.0)
                    .center_y()
                    .py(2.0)
                    .child(text(words(n, r)).font(k.ty.small).color(k.c.text_base).wrap().max_lines(3).grow(1.0).shrink().w(0.0))
                    .child(text("read access").font(k.ty.small).color(k.c.text_faint)),
            )
            .on_press(move |s: &mut App| s.select_agent(&name))
        })
        .collect()
}

#[cfg(test)]
#[path = "read_ask_tests.rs"]
mod tests;
