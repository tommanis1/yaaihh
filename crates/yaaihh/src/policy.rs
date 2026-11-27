//! The Settings page: one list of settings at three levels, every project, this project and one agent. A level below
//! shows what it inherits muted, with Override; a setting of its own is drawn plain, with Reset to the level above.
//! Read and written through coordd's `Request::Settings`; the per-provider models come from coordd's model list.

use crate::settings_look::{self as look, faint_value};
use crate::permission::{self, ALL, Permission};
use crate::{App, Proj, Sel};
use coord_types::{AgentView, Request};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Combobox, ComboboxOption, ComboboxState, Kit};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

/// The page's view id: no project file's view takes it.
pub const ID: &str = "~settings";

/// The level the page shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Scope {
    Global,
    #[default]
    Project,
    Agent(String),
}

impl Scope {
    /// Its name in `Request::Settings`.
    fn wire(&self) -> String {
        match self {
            Scope::Global => "yaaihh".into(),
            Scope::Project => "project".into(),
            Scope::Agent(n) => format!("agent:{n}"),
        }
    }
}

/// How a setting is picked: one of its choices (value, word), Safe, Guarded or Yolo, who may speak to him, or a model
/// of a provider's ids.
enum Control {
    Choice(&'static [(&'static str, &'static str)]),
    Permission,
    Speakers,
    Model(&'static str),
}

/// One setting: its key, its line, what it does, its control, and whether an agent sets it for itself.
struct Opt {
    key: &'static str,
    label: &'static str,
    about: &'static str,
    control: Control,
    agent: bool,
}

const ON_OFF: &[(&str, &str)] = &[("true", "On"), ("false", "Off")];
const SPEAKERS: [(&str, &str); 3] = [("leads", "Only leads"), ("named", "Named agents"), ("any", "Any agent")];

/// The page's sections, each a heading and its settings.
const SECTIONS: [(&str, &[Opt]); 3] = [
    ("Agents", &[
        Opt { key: permission::KEY, label: "Permissions", about: "", control: Control::Permission, agent: true },
        Opt { key: "boxed", label: "Agents run in a box", about: "Each agent sees only its folders, from its next start", control: Control::Choice(ON_OFF), agent: false },
    ]),
    ("Messages", &[
        Opt { key: "owner_speakers", label: "Who may speak to you", about: "Who may start threads with you", control: Control::Speakers, agent: false },
    ]),
    ("Default model per provider", &[
        Opt { key: "models.claude-gateway", label: "Claude gateway", about: "Claude models through the gateway", control: Control::Model("claude"), agent: false },
        Opt { key: "models.claude-personal", label: "Claude personal", about: "Claude models on your own subscription", control: Control::Model("claude"), agent: false },
        Opt { key: "models.agy-personal", label: "agy personal", about: "Gemini models on your own key", control: Control::Model("agy/"), agent: false },
        Opt { key: "models.t3code", label: "agy sign-in", about: "Gemini models on a Google sign-in: agy's own, or t3code's from before it", control: Control::Model("agy/"), agent: false },
        Opt { key: "models.opencode", label: "OpenCode", about: "OpenCode's models", control: Control::Model("opencode"), agent: false },
    ]),
];

/// What nothing set gives KEY in project P (coordd's own defaults); "" none.
fn default_of(p: &Proj, key: &str) -> &'static str {
    match key {
        "agent_mode" => "safe",
        "safety_check" if p.boxed => "false",
        "safety_check" => "true",
        "boxed" => "false",
        "owner_speakers" => "any",
        _ => "",
    }
}

/// coordd's answer: each key at the yaaihh-wide and project levels, unset ones null.
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq)]
pub struct Levels {
    #[serde(default)]
    pub yaaihh: Map<String, Value>,
    #[serde(default)]
    pub project: Map<String, Value>,
    #[serde(default)]
    pub said: Option<String>,
}

/// The page's state: the level shown, the project asked for and its answer, the models to pick from and each model
/// row's picker, Named agents pressed with none named yet, the writes in flight and what they said.
#[derive(Default)]
pub struct Settings {
    pub scope: Scope,
    pub asked: Option<String>,
    pub shown: Option<(String, Result<Levels, String>)>,
    pub models: Vec<String>,
    pub guesses: BTreeMap<String, crate::projects::Guess>,
    pub picks: HashMap<String, ComboboxState>,
    pub naming: bool,
    pub setting: usize,
    pub said: Option<Result<String, String>>,
}

/// A level's value of KEY as its word, `None` when unset.
fn word(m: &Map<String, Value>, key: &str) -> Option<String> {
    match m.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Agent A's own value of KEY, `None` when it takes the level above's.
fn agent_word(a: &AgentView, key: &str) -> Option<String> {
    match key {
        "agent_mode" => a.mode.clone().filter(|_| a.mode_from.as_deref() == Some("agent")),
        "safety_check" => (a.switches.from.get(key).map(String::as_str) == Some("agent")).then(|| a.switches.safety_check.to_string()),
        _ => None,
    }
}

/// Whether agent A's harness takes KEY at all, on project P's coordd.
fn agent_takes(p: &Proj, a: &AgentView, key: &str) -> bool {
    match key {
        "agent_mode" => crate::settings::takes_mode(&a.kind, p.protocol),
        permission::KEY => crate::settings::takes_mode(&a.kind, p.protocol) || a.capabilities.safety_toggle,
        _ => a.capabilities.safety_toggle,
    }
}

/// KEY at SCOPE: its own value, and the value it would inherit with where that comes from.
pub fn resolve(p: &Proj, l: &Levels, scope: &Scope, key: &str) -> (Option<String>, String, &'static str) {
    if key == permission::KEY {
        return permissions(p, l, scope);
    }
    let global = word(&l.yaaihh, key);
    let project = word(&l.project, key);
    let above_global = (global.clone().unwrap_or_else(|| default_of(p, key).into()), if global.is_some() { "every project" } else { "the default" });
    match scope {
        Scope::Global => (global, default_of(p, key).into(), "the default"),
        Scope::Project => (project, above_global.0, above_global.1),
        Scope::Agent(n) => {
            let own = p.live.agents.iter().find(|a| a.name == *n).and_then(|a| agent_word(a, key));
            match project {
                Some(v) => (own, v, "this project"),
                None => (own, above_global.0, above_global.1),
            }
        }
    }
}

/// The one Permissions row at SCOPE from its two settings: its own when either is set there, the one it would
/// inherit, and the nearer level that comes from (under Safe the mode's alone, the check changing nothing).
fn permissions(p: &Proj, l: &Levels, scope: &Scope) -> (Option<String>, String, &'static str) {
    let (mode, mut check) = (resolve(p, l, scope, "agent_mode"), resolve(p, l, scope, "safety_check"));
    // an agent whose harness has no check of its own (agy) is in Yolo, never Guarded
    if matches!(scope, Scope::Agent(n) if p.live.agents.iter().any(|a| a.name == *n && !permission::has_check(&a.kind))) {
        check = (None, "false".into(), check.2);
    }
    let at = |own: &Option<String>, up: &str| own.clone().unwrap_or_else(|| up.to_string());
    let own = (mode.0.is_some() || check.0.is_some()).then(|| Permission::of_words(&at(&mode.0, &mode.1), &at(&check.0, &check.1)).wire().to_string());
    let up = Permission::of_words(&mode.1, &check.1);
    let near = |f: &str| ["the default", "every project", "this project"].iter().position(|x| *x == f);
    let from = if up != Permission::Safe && near(check.2) > near(mode.2) { check.2 } else { mode.2 };
    (own, up.wire().to_string(), from)
}

/// The writes KEY = VALUE makes at the page's level: Permissions is its two settings, at an agent's level those its
/// harness takes.
fn writes(s: &App, pi: usize, key: &str, value: &str) -> Vec<(String, String)> {
    if key != permission::KEY {
        return vec![(key.into(), value.into())];
    }
    let p = Permission::from_wire(value);
    // the check first, so a restart for the mode starts with it
    let both = [("safety_check", p.map_or(String::new(), |p| p.check().to_string())), ("agent_mode", p.map_or(String::new(), |p| p.mode().into()))];
    let agent = match &s.settings.scope {
        Scope::Agent(n) => s.projects.get(pi).and_then(|p| Some((p, p.live.agents.iter().find(|a| a.name == *n)?))),
        _ => None,
    };
    both.into_iter().filter(|(k, _)| agent.is_none_or(|(p, a)| agent_takes(p, a, k))).map(|(k, v)| (k.to_string(), v)).collect()
}

/// The choices the Permissions row offers: all three, but at the level of an agent whose harness cannot switch the
/// mode or the check, only those it can be put in.
fn reachable(s: &App, pi: usize) -> Vec<Permission> {
    let (Scope::Agent(n), Some(p), Some((_, Ok(l)))) = (&s.settings.scope, s.projects.get(pi), s.settings.shown.as_ref()) else { return ALL.to_vec() };
    let Some(a) = p.live.agents.iter().find(|a| a.name == *n) else { return ALL.to_vec() };
    let now = |key| {
        let (own, up, _) = resolve(p, l, &s.settings.scope, key);
        own.unwrap_or(up)
    };
    let (yolo, check) = (now("agent_mode") == "yolo", now("safety_check") != "false" && permission::has_check(&a.kind));
    ALL.into_iter().filter(|c| c.reachable(agent_takes(p, a, "agent_mode"), agent_takes(p, a, "safety_check"), yolo, check)).collect()
}

/// The model each provider defaults to in project P, by its line, those set only: what New agent marks and picks.
pub fn default_models(s: &App, p: &Proj) -> Vec<(&'static str, String)> {
    let Some((_, Ok(l))) = s.settings.shown.as_ref().filter(|(id, _)| *id == p.id) else { return Vec::new() };
    let pick = |o: &Opt| {
        let (own, up, _) = resolve(p, l, &Scope::Project, o.key);
        Some((o.label, own.unwrap_or(up))).filter(|(_, m)| !m.is_empty())
    };
    SECTIONS[2].1.iter().filter_map(pick).collect()
}

/// While the page, Home or New agent shows: project PI's settings asked once, and the models it may run.
pub fn ask(s: &mut App, pi: usize) {
    let Some(p) = s.projects.get(pi) else { return };
    if p.protocol < coord_types::HOME_PROTOCOL || p.link.is_none() || s.settings.asked.as_deref() == Some(p.id.as_str()) {
        return;
    }
    s.settings.asked = Some(p.id.clone());
    let models = crate::projects::models_asked(p);
    s.request_in(pi, Request::Settings { scope: None, key: None, value: None }, crate::Pending::Settings { set: false });
    s.request_in(pi, models, crate::Pending::SettingsModels);
}

/// KEY set to VALUE ("" back to inheriting) at the page's level, through project PI's coordd; one change at a time.
pub fn set(s: &mut App, pi: usize, key: &str, value: &str) {
    if s.settings.setting > 0 {
        return;
    }
    let all = writes(s, pi, key, value);
    let scope = s.settings.scope.wire();
    (s.settings.setting, s.settings.said, s.settings.naming) = (all.len(), None, false);
    for (key, value) in all {
        s.request_in(pi, Request::Settings { scope: Some(scope.clone()), key: Some(key), value: Some(value) }, crate::Pending::Settings { set: true });
    }
}

/// coordd's answer R, to a change when SET.
pub fn came(s: &mut App, set: bool, r: Result<String, String>) {
    let id = s.settings.asked.clone().unwrap_or_default();
    let read = r.and_then(|t| serde_json::from_str::<Levels>(&t).map_err(|e| format!("unreadable answer: {e}")));
    if set {
        s.settings.setting = s.settings.setting.saturating_sub(1);
        // of several writes a refusal stays said
        if !matches!(s.settings.said, Some(Err(_))) {
            s.settings.said = Some(match &read {
                Ok(m) => Ok(m.said.clone().unwrap_or_default()),
                Err(e) => Err(crate::accounts::plain(e)),
            });
        }
        if read.is_err() {
            return;
        }
    }
    s.settings.shown = Some((id, read));
}

/// coordd's models, as New agent reads them.
pub fn models_came(s: &mut App, r: Result<String, String>) {
    if let Ok(out) = r {
        let (lines, guesses) = crate::projects::models_of(&out);
        (s.settings.models, s.settings.guesses) = (crate::projects::listed_models(&lines), guesses);
    }
}

/// Opens the page at SCOPE.
pub fn open(s: &mut App, scope: Scope) {
    s.settings.scope = scope;
    s.sel = Sel::View(ID.into());
}

/// The levels to pick from: every project, this project, then each agent that runs.
fn scopes(p: &Proj) -> Vec<(Scope, String)> {
    let mut v = vec![(Scope::Global, "Every project".to_string()), (Scope::Project, p.name.clone())];
    v.extend(p.live.agents.iter().filter(|a| a.state != "stopped").map(|a| (Scope::Agent(a.name.clone()), a.name.clone())));
    v
}

/// The page for P, project PI.
pub fn page(s: &App, p: &Proj, k: &Kit) -> El {
    let pi = s.place_of(p);
    let scope = &s.settings.scope;
    // the levels, a list on the left
    let mut menu = look::levels("settings-scopes");
    for (i, (sc, name)) in scopes(p).into_iter().enumerate() {
        if i == 2 {
            menu = menu.child(look::heading("Agents", k));
        }
        let (on, key) = (sc == *scope, format!("settings-scope-{}", sc.wire()));
        menu = menu.child(look::level(key, &name, on, move |s: &mut App| (s.settings.scope, s.settings.naming) = (sc.clone(), false), k));
    }
    let body = match s.settings.shown.as_ref().filter(|(id, _)| *id == p.id) {
        _ if p.protocol < coord_types::HOME_PROTOCOL => col().child(muted(k, "Needs a newer engine.".into())),
        None => col().child(muted(k, "Reading…".into())),
        Some((_, Err(e))) => col().child(muted(k, format!("Not read: {e}"))),
        Some((_, Ok(l))) => rows(s, p, pi, l, k),
    };
    let title = match scope {
        Scope::Global => "Every project".to_string(),
        Scope::Project => p.name.clone(),
        Scope::Agent(n) => n.clone(),
    };
    look::page("settings", menu, &title, body, k)
}

fn muted(k: &Kit, t: String) -> El {
    text(t).font(k.ty.small).color(k.c.text_muted).wrap()
}

/// VALUE of setting O as its row says it: its choice's word, the speakers, a model's plain name; "Not set" for none.
fn words(o: &Opt, value: &str) -> String {
    match &o.control {
        _ if value.is_empty() => "Not set".into(),
        Control::Choice(c) => c.iter().find(|(x, _)| *x == value).map_or(value.to_string(), |(_, w)| w.to_string()),
        Control::Permission => Permission::from_wire(value).map_or(value.to_string(), |p| p.word().to_string()),
        Control::Speakers => SPEAKERS.iter().find(|(x, _)| *x == value).map_or_else(|| format!("Named: {}", value.replace(',', ", ")), |(_, w)| w.to_string()),
        Control::Model(_) => crate::projects::plain_model(value),
    }
}

/// The settings at the page's level, by section, and what the last change said.
fn rows(s: &App, p: &Proj, pi: usize, l: &Levels, k: &Kit) -> El {
    let scope = &s.settings.scope;
    let agent = match scope {
        Scope::Agent(n) => p.live.agents.iter().find(|a| a.name == *n),
        _ => None,
    };
    let mut c = col().w_full().gap(0.0);
    for (heading, opts) in SECTIONS {
        c = c.child(look::section(heading, k));
        for o in opts {
            let (own, inherited, from) = resolve(p, l, scope, o.key);
            let right = match agent {
                Some(_) if !o.agent => faint_value(k, &words(o, &inherited), "Set for the whole project"),
                Some(a) if !agent_takes(p, a, o.key) => row().child(text("Its harness does not take this").font(k.ty.small).color(k.c.text_faint)),
                _ => match own {
                    Some(v) => {
                        let back = match (scope, from) {
                            (Scope::Global, _) => "Reset to default",
                            (_, "this project") => "Reset to project",
                            _ => "Reset to global",
                        };
                        let key = o.key;
                        let reset = Button::new(back).variant(ButtonVariant::Ghost).on_press(move |s: &mut App| set(s, pi, key, "")).build(k).key(format!("settings-reset-{key}"));
                        control(s, pi, o, Some(&v), k).child(reset)
                    }
                    // nothing to inherit: picked here at once
                    None if inherited.is_empty() => control(s, pi, o, None, k),
                    None => {
                        let tag = if *scope == Scope::Global { "Default".to_string() } else { format!("Inherited from {from}") };
                        let (key, value) = (o.key, inherited.clone());
                        let over = Button::new("Override").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| set(s, pi, key, &value)).build(k).key(format!("settings-override-{key}"));
                        faint_value(k, &words(o, &inherited), &tag).child(over)
                    }
                },
            };
            c = c.child(look::setting(format!("settings-row-{}", o.key), o.label, o.about, right, k));
        }
    }
    let said = match (&s.settings.said, s.settings.setting > 0) {
        (_, true) => Some(("Writing it…".to_string(), false)),
        (Some(Ok(t)), _) if !t.is_empty() => Some((t.clone(), false)),
        (Some(Err(e)), _) => Some((e.clone(), true)),
        _ => None,
    };
    if let Some((t, bad)) = said {
        c = c.child(text(t).font(k.ty.small).color(if bad { k.c.danger.fg } else { k.c.text_faint }).wrap().pt(12.0).key("settings-said"));
    }
    c
}

/// Setting O's control at value V (none: nothing yet), each pick written at once.
fn control(s: &App, pi: usize, o: &'static Opt, v: Option<&str>, k: &Kit) -> El {
    let key = o.key;
    let r = row().gap(12.0).center_y();
    match &o.control {
        Control::Choice(choices) => {
            let i = v.and_then(|v| choices.iter().position(|(x, _)| *x == v));
            let labels: Vec<&str> = choices.iter().map(|(_, w)| *w).collect();
            look::choice(o.label, &labels, i, move |s: &mut App, j| set(s, pi, key, choices[j].0), k)
        }
        Control::Permission => {
            let can = reachable(s, pi);
            let i = v.and_then(Permission::from_wire).and_then(|v| can.iter().position(|c| *c == v));
            let labels: Vec<&str> = can.iter().map(|c| c.word()).collect();
            look::choice(o.label, &labels, i, move |s: &mut App, j| set(s, pi, key, can[j].wire()), k).help(permission::HELP)
        }
        Control::Speakers => r.child(speakers(s, pi, v.unwrap_or("any"), k)),
        Control::Model(prefix) => {
            let models: Vec<String> = s.settings.models.iter().filter(|m| m.starts_with(prefix)).cloned().collect();
            let options = crate::projects::model_options(&models, &s.settings.guesses, &[]);
            let state = s.settings.picks.get(key).cloned().unwrap_or_default();
            let mut layer = Vec::new();
            let pick = Combobox::new(format!("settings-model-{key}"), &state, &options, move |s: &mut App| s.settings.picks.entry(key.to_string()).or_default())
                .label(o.label)
                .current(v)
                .placeholder(if models.is_empty() { "No model of it here" } else { "Not set" })
                .search_placeholder("Model")
                .width(260.0)
                .disabled(models.is_empty())
                .on_choose(move |s: &mut App, m, _| set(s, pi, key, m))
                .build(k, &mut layer);
            r.child(pick).children(layer)
        }
    }
}

/// Who may speak to him at V: Only leads, Named agents, Any agent; named, each name with its remove and Add another.
fn speakers(s: &App, pi: usize, v: &str, k: &Kit) -> El {
    let named: Vec<String> = if SPEAKERS.iter().any(|(x, _)| *x == v) { Vec::new() } else { v.split(',').map(str::to_string).collect() };
    let naming = s.settings.naming || !named.is_empty();
    let i = if naming { 1 } else { SPEAKERS.iter().position(|(x, _)| *x == v).unwrap_or(2) };
    let labels: Vec<&str> = SPEAKERS.iter().map(|(_, w)| *w).collect();
    let ctl = look::choice("Who may speak to you", &labels, Some(i), move |s: &mut App, j| match j {
        1 => s.settings.naming = true,
        j => set(s, pi, "owner_speakers", SPEAKERS[j].0),
    }, k);
    let mut c = col().items_end().gap(6.0).child(ctl);
    if naming {
        let mut names = row().gap(4.0).center_y();
        for n in &named {
            let rest: Vec<String> = named.iter().filter(|x| *x != n).cloned().collect();
            let b = Button::new(format!("{n} ×")).variant(ButtonVariant::Ghost).disabled(rest.is_empty()).on_press(move |s: &mut App| set(s, pi, "owner_speakers", &rest.join(","))).build(k).key(format!("settings-speaker-{n}"));
            names = names.child(b);
        }
        let options: Vec<ComboboxOption> = s.projects.get(pi).map(|p| p.live.agents.iter().filter(|a| !named.contains(&a.name)).map(|a| ComboboxOption::new(a.name.clone(), a.name.clone())).collect()).unwrap_or_default();
        let state = s.settings.picks.get("owner_speakers").cloned().unwrap_or_default();
        let mut layer = Vec::new();
        let add = Combobox::new("settings-speaker-add", &state, &options, |s: &mut App| s.settings.picks.entry("owner_speakers".into()).or_default())
            .placeholder("Add an agent…")
            .search_placeholder("Agent")
            .width(200.0)
            .on_choose(move |s: &mut App, name, _| {
                let mut all = named.clone();
                all.push(name.to_string());
                set(s, pi, "owner_speakers", &all.join(","));
            })
            .build(k, &mut layer);
        c = c.child(names.child(add).children(layer));
    }
    c
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
