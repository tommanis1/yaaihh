//! The Policy page of a v2 engine: its plan from the engine's texts, its questions through `query`, its drawing.

pub mod ask;
#[cfg(all(test, unix))]
pub(crate) mod live;
pub mod page;
pub mod plan;

use crate::ext::Extension;
use crate::App;
use rune::{col, text, El};
use rune_kit::Kit;
use std::cell::OnceCell;

pub const NAME: &str = "policy";
const TITLE: &str = "Policy";

/// The page's plan, read once from the texts the engine's extension loaded, and the level it shows.
#[derive(Default)]
struct Policy {
    plan: OnceCell<plan::Plan>,
    at: Option<page::At>,
}

pub fn extension() -> Extension {
    Extension {
        name: NAME,
        init: |s| {
            if s.ext.try_get::<Policy>().is_none() {
                s.ext.insert(Policy::default());
            }
        },
        setup: |_, _| {},
        poll: |_, _| false,
        rows: |_, _, _| Vec::new(),
        nav_key: |_| String::new(),
        page,
        title: |_| TITLE.into(),
        reply: |_, _, _, _| {},
        prices: None,
        scenes: &[],
        replaces: &[],
        window_pages: &[],
        accounts: None,
    }
}

/// The page on the engine the projects file names, or why there is none.
fn page(s: &App, k: &Kit, _: &str) -> (String, El) {
    let say = |t: String| (TITLE.to_string(), col().key("policy-v2").w_full().padding(24.0).child(text(t).font(k.ty.body).color(k.c.text_muted)));
    let (Some(v), Some(p)) = (s.ext.try_get::<crate::v2::V2>(), s.ext.try_get::<Policy>()) else { return say("No engine: the projects file names none.".into()) };
    let Some(c) = &v.config else { return say("No engine: the projects file names none.".into()) };
    if c.top.is_empty() {
        return say("No top module in [v2].".into());
    }
    let Some(loaded) = &v.loaded else {
        let why = match &v.map {
            Some(Err(f)) => f.join("; "),
            _ => "not read yet".into(),
        };
        return say(format!("The engine's texts do not read here: {why}"));
    };
    let plan = p.plan.get_or_init(|| plan::plan(&v.modules, loaded, &c.top));
    let parties = v.desk.parties();
    let acts = (!c.party.is_empty()).then(|| page::Acts { me: c.party.clone(), send: crate::v2::act });
    let e = page::Engine {
        loaded: v.desk.loaded_line(),
        parties: &parties,
        ask: &|q| crate::v2::query(s, q),
        refused: &|a| v.desk.refused(a),
        no: v.no.as_deref(),
        acts,
        at: p.at.as_ref(),
        go: |s, at| s.ext.get_mut::<Policy>().at = Some(at),
    };
    (TITLE.to_string(), page::page(plan, &e, k))
}
