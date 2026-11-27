//! The Harnesses page (vault designs/harness-versions-2026-11-11.md): the programs that run agents (Claude Code,
//! Codex) and the account proxy they go through. It shows each one's version, and for each harness the grid of
//! model × provider pairs the proxy can serve, each switched on or off.
//!
//! The proxy owns the decision: it reads `harness-models.json` beside its config (ordered rules per harness, the last
//! match wins, no match is on), lists only a harness's pairs that are on in `/v1/models`, and refuses a request for a
//! pair that is off. This page reads the pairs and the file's name from the proxy's `/v1/harness-models`. A checkbox
//! asks a project's coordd to set the rule (`Request::HarnessSet`, from `RULES_PROTOCOL` on; N-802.p23): coordd is
//! the file's one writer, for both windows. The proxy reads the file again within a second, and the page then reads
//! back what the proxy has in force, so a switch shows that, not what was pressed; a refusal shows in coordd's words.
//!
//! The proxy is reached as the owner's own Claude Code reaches it: `ANTHROPIC_BASE_URL` and `ANTHROPIC_AUTH_TOKEN`
//! in `~/.claude/settings.json`. The key goes to curl on its stdin, never on a command line.
//!
//! On Windows the machine is the WSL one: the page asks a project's coordd (`Request::Harness`, docket-core5's p129,
//! read on the server on the coord's own route and key), and the newest Codex only when the owner presses for it
//! (`Request::HarnessLatest`, p133). With a coordd older than `RULES_PROTOCOL` the switches are not set from there
//! (the page says it needs a newer server release), and the Linux window writes the file itself as it did before.

use crate::App;
use crate::ext::Extension;
use rune::{El, ScrollSpec, Tx, col, row, text};
use rune_kit::{Button, ButtonVariant, Checkbox, Kit};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::mpsc::{Receiver, Sender, channel};

pub(crate) const NAME: &str = "harnesses";

/// The extension that adds the page; the core binary registers it (main.rs).
pub fn extension() -> Extension {
    Extension {
        name: NAME,
        init,
        setup,
        poll,
        rows: |_, _, _| Vec::new(),
        nav_key: |_| String::new(),
        page,
        title: |_| "Harnesses".into(),
        reply,
        prices: None,
        scenes: &[("harnesses", scene)],
        replaces: &[],
        window_pages: &[],
        accounts: None,
    }
}

/// A rule of harness-models.json (the proxy's internal/harness.Rule).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Rule {
    model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    provider: String,
    on: bool,
}

/// One model × provider pair, and whether each harness may use it.
#[derive(Clone, Debug, Deserialize)]
struct Pair {
    model: String,
    provider: String,
    #[serde(default, deserialize_with = "or_empty")]
    owned_by: String,
    #[serde(default, deserialize_with = "or_empty")]
    on: BTreeMap<String, bool>,
}

/// The proxy's answer to /v1/harness-models; empty when the project reaches no proxy.
#[derive(Clone, Debug, Default, Deserialize)]
struct Snapshot {
    #[serde(default, deserialize_with = "or_empty")]
    version: String,
    #[serde(default, deserialize_with = "or_empty")]
    file: String,
    #[serde(default, deserialize_with = "or_empty")]
    harnesses: Vec<String>,
    #[serde(default, deserialize_with = "rules_or_empty")]
    rules: BTreeMap<String, Vec<Rule>>,
    #[serde(default, deserialize_with = "or_empty")]
    pairs: Vec<Pair>,
}

/// A field the proxy (Go) writes as `null` when it is empty, read as empty.
fn or_empty<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + Default>(d: D) -> Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// The rules, a harness's `null` list read as no rules.
fn rules_or_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Vec<Rule>>, D::Error> {
    let m: Option<BTreeMap<String, Option<Vec<Rule>>>> = Deserialize::deserialize(d)?;
    Ok(m.unwrap_or_default().into_iter().map(|(h, r)| (h, r.unwrap_or_default())).collect())
}

/// A harness program's versions: the one in use, the ones kept on disk, the newest upstream when known.
#[derive(Clone, Debug, Default, Deserialize)]
struct Versions {
    name: String,
    #[serde(default, deserialize_with = "or_empty")]
    installed: String,
    #[serde(default, deserialize_with = "or_empty")]
    kept: Vec<String>,
    #[serde(default)]
    latest: Option<String>,
    #[serde(default, deserialize_with = "or_empty")]
    how: String,
}

enum Msg {
    Pairs(Result<Snapshot, String>),
    Versions(Vec<Versions>),
    /// Windows: time to read back what the proxy has in force, after a switch.
    ReadBack,
}

/// coordd's answer to `Request::Harness` (docket-core5, N-412 p129).
#[derive(Deserialize)]
struct Served {
    #[serde(default, deserialize_with = "or_empty")]
    versions: Vec<Versions>,
    #[serde(default)]
    pairs: Option<Snapshot>,
    #[serde(default)]
    error: Option<String>,
}

/// coordd's answer read as the page's: no pairs and no error is a project that reaches no proxy, not a fault.
fn served(out: &str) -> Result<(Vec<Versions>, Result<Snapshot, String>), String> {
    let v: Served = serde_json::from_str(out).map_err(|e| format!("answer: {e}"))?;
    let pairs = match (v.pairs, v.error) {
        (Some(p), _) => Ok(p),
        (None, Some(e)) => Err(e),
        (None, None) => Ok(Snapshot::default()),
    };
    Ok((v.versions, pairs))
}

/// What a request to coordd was (Windows).
enum Asked {
    Harness,
    Latest,
    /// A switch (`Request::HarnessSet`).
    Set,
}

/// The grid's place when the project reaches no account proxy.
const NO_PROXY: &str = "No account proxy, no models to switch.";

/// How long the proxy may take to read its rules file again before the page reads back what is in force.
const READ_BACK_MS: u64 = 1200;

/// The page's state.
pub struct Harnesses {
    pairs: Option<Result<Snapshot, String>>,
    versions: Vec<Versions>,
    /// The harness whose grid shows.
    shown: String,
    /// A write of the rules file that failed.
    error: Option<String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    waker: Option<rune::Waker>,
    /// Windows: the project whose coordd was asked (its index), once one speaks the request.
    asked: Option<usize>,
    /// Windows: the newest Codex asked, until npm answers.
    looking: bool,
    /// A switch sent to coordd, until what the proxy has in force is read back: the switches wait.
    setting: bool,
}

fn init(s: &mut App) {
    let (tx, rx) = channel();
    s.ext.insert(Harnesses { pairs: None, versions: Vec::new(), shown: "claude-code".into(), error: None, tx, rx, waker: None, asked: None, looking: false, setting: false });
}

fn setup(s: &mut App, waker: rune::Waker) {
    let h = s.ext.get_mut::<Harnesses>();
    h.waker = Some(waker);
    // the WSL machine's programs and proxy: asked of coordd from poll, once a pipe is up
    if crate::elsewhere::away() {
        return;
    }
    refresh(h, 0);
    let (tx, waker) = (h.tx.clone(), h.waker.clone());
    std::thread::spawn(move || {
        let _ = tx.send(Msg::Versions(versions()));
        if let Some(w) = waker {
            w();
        }
    });
}

/// The first project whose bridge serves the server's reads.
fn server(s: &App) -> Option<usize> {
    s.projects.iter().position(|p| p.protocol >= crate::SERVER_READS)
}

/// Where a switch goes: a project's coordd, or (Linux, with no coordd that sets rules) the file as before, or nowhere
/// (Windows, with none).
#[derive(Debug, PartialEq)]
enum Setter {
    Coordd(usize),
    File,
    Nowhere,
}

fn setter(away: bool, protocols: &[u32]) -> Setter {
    match protocols.iter().position(|p| *p >= coord_types::RULES_PROTOCOL) {
        Some(i) => Setter::Coordd(i),
        None if away => Setter::Nowhere,
        None => Setter::File,
    }
}

fn setter_of(s: &App) -> Setter {
    setter(crate::elsewhere::away(), &s.projects.iter().map(|p| p.protocol).collect::<Vec<_>>())
}

/// The request a switch sends: PAIRS of HARNESS on or off; none and ALL: every pair on again.
fn set_request(harness: &str, pairs: &[(String, String)], on: bool, all: bool) -> coord_types::Request {
    let pairs = if all { Vec::new() } else { pairs.iter().map(|(m, p)| coord_types::RulePair { model: m.clone(), provider: p.clone() }).collect() };
    coord_types::Request::HarnessSet { harness: harness.to_string(), pairs, on: on && !all, all_on: all }
}

/// Send a switch to coordd I; the switches wait for its answer and the read-back.
fn send(s: &mut App, i: usize, req: coord_types::Request) {
    let sent = s.projects.get_mut(i).is_some_and(|p| p.ask(&req, NAME, Box::new(Asked::Set)));
    let h = s.ext.get_mut::<Harnesses>();
    if sent {
        h.setting = true;
        h.error = None;
    } else {
        h.error = Some("Not set: the project's server is not connected.".into());
    }
}

fn poll(s: &mut App, _: &mut Tx) -> bool {
    if crate::elsewhere::away() && s.ext.get::<Harnesses>().asked.is_none() {
        if let Some(i) = server(s) {
            if s.projects[i].ask(&coord_types::Request::Harness, NAME, Box::new(Asked::Harness)) {
                s.ext.get_mut::<Harnesses>().asked = Some(i);
            }
        }
    }
    let h = s.ext.get_mut::<Harnesses>();
    let mut changed = false;
    while let Ok(m) = h.rx.try_recv() {
        match m {
            Msg::Pairs(p) => {
                h.pairs = Some(p);
                h.setting = false;
            }
            Msg::Versions(v) => h.versions = v,
            Msg::ReadBack => h.asked = None,
        }
        changed = true;
    }
    changed
}

/// Ask the proxy for the pairs again, after DELAY_MS (the proxy checks the rules file at most once a second).
fn refresh(h: &Harnesses, delay_ms: u64) {
    let (tx, waker) = (h.tx.clone(), h.waker.clone());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        let _ = tx.send(Msg::Pairs(fetch()));
        if let Some(w) = waker {
            w();
        }
    });
}

/// coordd's answer (Windows): the page's data, or the newest Codex.
fn reply(s: &mut App, _: usize, data: Box<dyn std::any::Any>, r: Result<String, String>) {
    let h = s.ext.get_mut::<Harnesses>();
    match data.downcast_ref::<Asked>() {
        Some(Asked::Harness) => {
            h.setting = false;
            match r.and_then(|out| served(&out)) {
                Ok((versions, pairs)) => {
                    h.versions = versions;
                    h.pairs = Some(pairs);
                }
                Err(e) => h.pairs = Some(Err(e)),
            }
        }
        // a switch: refused, coordd's words and the switches as they were; set, what the proxy now has in force read
        // back once it has read the file again
        Some(Asked::Set) => match r {
            Err(e) => {
                h.setting = false;
                h.error = Some(e.trim_start_matches("coordd: ").to_string());
            }
            Ok(_) if crate::elsewhere::away() => {
                let (tx, waker) = (h.tx.clone(), h.waker.clone());
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(READ_BACK_MS));
                    let _ = tx.send(Msg::ReadBack);
                    if let Some(w) = waker {
                        w();
                    }
                });
            }
            Ok(_) => refresh(h, READ_BACK_MS),
        },
        Some(Asked::Latest) => {
            h.looking = false;
            match r {
                Ok(v) => {
                    if let Some(c) = h.versions.iter_mut().find(|x| x.name == "Codex") {
                        c.latest = Some(v.trim().to_string());
                    }
                }
                Err(e) => h.error = Some(e),
            }
        }
        None => {}
    }
}

/// Windows: ask coordd for the newest Codex (npm, only when pressed).
fn look_up_latest(s: &mut App) {
    let Some(i) = s.ext.get::<Harnesses>().asked else { return };
    if s.projects.get_mut(i).is_some_and(|p| p.ask(&coord_types::Request::HarnessLatest, NAME, Box::new(Asked::Latest))) {
        s.ext.get_mut::<Harnesses>().looking = true;
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// The proxy's base URL and key, from the owner's Claude Code settings.
fn route() -> Result<(String, String), String> {
    let path = home().join(".claude/settings.json");
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?).map_err(|e| format!("{}: {e}", path.display()))?;
    let env = |k: &str| v.get("env").and_then(|e| e.get(k)).and_then(|x| x.as_str()).map(str::to_string);
    let key = env("ANTHROPIC_AUTH_TOKEN").ok_or("no ANTHROPIC_AUTH_TOKEN in ~/.claude/settings.json")?;
    Ok((env("ANTHROPIC_BASE_URL").unwrap_or_else(|| "http://127.0.0.1:8317".into()), key))
}

fn fetch() -> Result<Snapshot, String> {
    let (base, key) = route()?;
    let mut child = crate::places::command("curl")
        .args(["-sS", "--max-time", "10", "-f", "-H", "@-", &format!("{}/v1/harness-models", base.trim_end_matches('/'))])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    if let Some(mut i) = child.stdin.take() {
        let _ = writeln!(i, "Authorization: Bearer {key}");
    }
    let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("the proxy did not answer /v1/harness-models: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("the proxy's answer: {e}"))
}

fn first_line(cmd: &str, args: &[&str]) -> Option<String> {
    let out = crate::places::command(cmd).args(args).stderr(Stdio::null()).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).lines().next()?.trim().to_string();
    (out.status.success() && !s.is_empty()).then_some(s)
}

/// Claude Code's and Codex's versions: installed, kept, newest upstream (Codex from npm; Claude Code updates itself).
fn versions() -> Vec<Versions> {
    let dir = home().join(".local/share/claude/versions");
    let mut kept: Vec<String> = std::fs::read_dir(&dir).map(|d| d.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect()).unwrap_or_default();
    kept.sort();
    let claude = std::fs::read_link(home().join(".local/bin/claude")).ok().and_then(|p| Some(p.file_name()?.to_string_lossy().into_owned())).unwrap_or_else(|| "not found".into());
    let codex = first_line("codex", &["--version"]).map(|v| v.trim_start_matches("codex-cli ").to_string()).unwrap_or_else(|| "not found".into());
    vec![
        Versions { name: "Claude Code".into(), installed: claude, kept, latest: None, how: "native install; updates itself".into() },
        Versions { name: "Codex".into(), installed: codex, kept: Vec::new(), latest: first_line("npm", &["view", "@openai/codex", "version"]), how: "npm global @openai/codex".into() },
    ]
}

/// Switch pairs (model, provider) of harness H on or off: each pair's exact rules are replaced by one at the end.
fn set(s: &mut App, harness: &str, pairs: &[(String, String)], on: bool) {
    match setter_of(s) {
        Setter::Coordd(i) => return send(s, i, set_request(harness, pairs, on, false)),
        Setter::Nowhere => return,
        Setter::File => {}
    }
    let h = s.ext.get_mut::<Harnesses>();
    let Some(Ok(snap)) = &mut h.pairs else { return };
    let mut rules = snap.rules.clone();
    let list = rules.entry(harness.to_string()).or_default();
    for (m, p) in pairs {
        list.retain(|r| !(r.model == *m && r.provider == *p));
        list.push(Rule { model: m.clone(), provider: p.clone(), on });
    }
    match write_rules(&snap.file, &rules) {
        Ok(()) => {
            snap.rules = rules;
            for x in snap.pairs.iter_mut().filter(|x| pairs.iter().any(|(m, p)| *m == x.model && *p == x.provider)) {
                x.on.insert(harness.to_string(), on);
            }
            h.error = None;
        }
        Err(e) => h.error = Some(e),
    }
    refresh(h, 1200);
}

/// Every pair of harness H on again: its rules are dropped.
fn all_on(s: &mut App, harness: &str) {
    match setter_of(s) {
        Setter::Coordd(i) => return send(s, i, set_request(harness, &[], true, true)),
        Setter::Nowhere => return,
        Setter::File => {}
    }
    let h = s.ext.get_mut::<Harnesses>();
    let Some(Ok(snap)) = &mut h.pairs else { return };
    let mut rules = snap.rules.clone();
    rules.remove(harness);
    match write_rules(&snap.file, &rules) {
        Ok(()) => {
            snap.rules = rules;
            h.error = None;
        }
        Err(e) => h.error = Some(e),
    }
    refresh(h, 1200);
}

/// Write the rules file whole, through a temporary file and a rename (the proxy never reads half a file): the Linux
/// window with no coordd that sets rules (older than `RULES_PROTOCOL`).
fn write_rules(file: &str, rules: &BTreeMap<String, Vec<Rule>>) -> Result<(), String> {
    if file.is_empty() {
        return Err("the proxy names no rules file".into());
    }
    let tmp = format!("{file}.tmp.{}", std::process::id());
    let body = serde_json::to_string_pretty(rules).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body + "\n").and_then(|_| std::fs::rename(&tmp, file)).map_err(|e| format!("{file}: {e}"))
}

fn page(s: &App, k: &Kit, _: &str) -> (String, El) {
    let h = s.ext.get::<Harnesses>();
    let heading = |t: &str| text(t.to_string()).font(k.ty.body_medium).color(k.c.text_base);
    let muted = |t: String| text(t).font(k.ty.small).color(k.c.text_muted);
    let mut c = col().w_full().gap(10.0).padding(24.0);
    let away = crate::elsewhere::away();
    // the switches: set by a coordd, or by this window's own write on Linux; nowhere on Windows with an older coordd
    let fixed = setter_of(s) == Setter::Nowhere || h.setting;
    if away && h.asked.is_none() && h.pairs.is_none() {
        return ("Harnesses".into(), c.child(muted(crate::elsewhere::LINE.into()).wrap()).into_page());
    }

    // versions
    c = c.child(heading("Versions"));
    for v in &h.versions {
        let mut line = format!("{}: {}", v.name, v.installed);
        if let Some(l) = v.latest.as_ref().filter(|l| **l != v.installed) {
            line += &format!(" (newest {l})");
        }
        c = c.child(text(line).font(k.ty.ui).color(k.c.text_base));
        let kept = if v.kept.len() > 1 { format!(" · kept: {}", v.kept.join(", ")) } else { String::new() };
        c = c.child(muted(format!("{}{kept}", v.how)));
    }
    if let Some(Ok(snap)) = h.pairs.as_ref().filter(|p| p.as_ref().is_ok_and(|s| !s.version.is_empty())) {
        c = c.child(text(format!("Account proxy: {}", snap.version)).font(k.ty.ui).color(k.c.text_base)).child(muted("our fork, updated by hand".into()));
    }
    if h.versions.is_empty() {
        c = c.child(muted("reading…".into()));
    } else if away {
        let b = Button::new(if h.looking { "Looking up the newest Codex…" } else { "Look up the newest Codex" }).variant(ButtonVariant::Ghost).disabled(h.looking);
        c = c.child(b.on_press(look_up_latest).build(k));
    }

    // the grid
    c = c.child(col().pt(14.0).child(heading("Models and providers per harness")));
    let snap = match &h.pairs {
        None => return ("Harnesses".into(), c.child(muted("asking the proxy…".into())).into_page()),
        Some(Err(e)) => return ("Harnesses".into(), c.child(text(e.clone()).font(k.ty.small).color(k.c.warning.fg).wrap()).into_page()),
        Some(Ok(snap)) if snap.pairs.is_empty() && snap.harnesses.is_empty() => return ("Harnesses".into(), c.child(muted(NO_PROXY.into()).wrap()).into_page()),
        Some(Ok(snap)) => snap,
    };
    c = c.child(muted(format!("Off pairs are refused. Rules: {}", snap.file)).wrap());
    let mut tabs = row().gap(6.0);
    for name in &snap.harnesses {
        let n = name.clone();
        let count = snap.pairs.iter().filter(|p| p.on.get(name).copied().unwrap_or(true)).count();
        let b = Button::new(format!("{name} ({count}/{})", snap.pairs.len()))
            .variant(if *name == h.shown { ButtonVariant::Contrast } else { ButtonVariant::Ghost })
            .on_press(move |s: &mut App| s.ext.get_mut::<Harnesses>().shown = n.clone());
        tabs = tabs.child(b.build(k));
    }
    let shown = h.shown.clone();
    let all = shown.clone();
    tabs = tabs.child(Button::new("All on").variant(ButtonVariant::Ghost).disabled(fixed).on_press(move |s: &mut App| all_on(s, &all)).build(k));
    c = c.child(tabs);
    if setter_of(s) == Setter::Nowhere {
        c = c.child(muted(crate::elsewhere::LINE.into()).wrap());
    } else if h.setting {
        c = c.child(muted("Applying…".into()));
    }
    if let Some(e) = &h.error {
        c = c.child(text(e.clone()).font(k.ty.small).color(k.c.warning.fg).wrap());
    }

    // grouped by who makes the model, each group with a checkbox for all its pairs
    let mut groups: BTreeMap<&str, Vec<&Pair>> = BTreeMap::new();
    for p in &snap.pairs {
        groups.entry(if p.owned_by.is_empty() { "other" } else { p.owned_by.as_str() }).or_default().push(p);
    }
    let on = |p: &Pair| p.on.get(&shown).copied().unwrap_or(true);
    for (owner, ps) in groups {
        let n_on = ps.iter().filter(|p| on(p)).count();
        let keys: Vec<(String, String)> = ps.iter().map(|p| (p.model.clone(), p.provider.clone())).collect();
        let (hn, turn) = (shown.clone(), n_on < ps.len());
        let head = Checkbox::new(n_on == ps.len())
            .disabled(fixed)
            .indeterminate(n_on > 0 && n_on < ps.len())
            .label(format!("{owner} ({n_on}/{})", ps.len()))
            .on_toggle(move |s: &mut App| set(s, &hn, &keys, turn));
        let mut g = col().w_full().gap(2.0).pt(8.0).child(head.build(k));
        for p in ps {
            let (hn, key, now) = (shown.clone(), vec![(p.model.clone(), p.provider.clone())], on(p));
            let cb = Checkbox::new(now).disabled(fixed).label(format!("{}  ·  {}", p.model, p.provider)).on_toggle(move |s: &mut App| set(s, &hn, &key, !now));
            g = g.child(row().pl(24.0).child(cb.build(k)).key(format!("pair-{}-{}", p.model, p.provider)));
        }
        c = c.child(g.key(format!("group-{owner}")));
    }
    ("Harnesses".into(), c.into_page())
}

/// The page as the Windows window draws it, from the project's coordd (`--snap F --scene harnesses`); with
/// YAAIHH_HARNESS_SET=HARNESS:MODEL:PROVIDER:on|off, that switch pressed first, as the checkbox sends it, and the page
/// then read back.
fn scene(s: &mut App) {
    crate::elsewhere::still_as_windows();
    crate::read_once(s, crate::Sel::Ext(NAME.into(), String::new()));
    let Setter::Coordd(i) = setter_of(s) else { return };
    if let Some([harness, model, provider, on]) = std::env::var("YAAIHH_HARNESS_SET").ok().map(|v| v.split(':').map(str::to_string).collect::<Vec<_>>()).and_then(|v| <[String; 4]>::try_from(v).ok()) {
        let r = s.projects[i].ask_once(&set_request(&harness, &[(model, provider)], on == "on", false));
        if let Err(e) = r {
            s.ext.get_mut::<Harnesses>().error = Some(e);
        }
        s.ext.get_mut::<Harnesses>().shown = harness;
    }
    let r = s.projects[i].ask_once(&coord_types::Request::Harness);
    let error = s.ext.get::<Harnesses>().error.clone();
    reply(s, i, Box::new(Asked::Harness), r);
    let h = s.ext.get_mut::<Harnesses>();
    (h.asked, h.error) = (Some(i), error);
}

/// The page's column, scrolled.
trait IntoPage {
    fn into_page(self) -> El;
}

impl IntoPage for El {
    fn into_page(self) -> El {
        col().key("harnesses-page").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On Windows the page is coordd's answer: the versions, the pairs (or why none), and the newest Codex when asked.
    #[test]
    fn on_windows_the_page_takes_coordds_answer() {
        let (_held, mut s) = crate::view_cost::test_app();
        init(&mut s);
        let served = serde_json::json!({
            "versions": [{"name": "Claude Code", "installed": "2.1.40", "kept": ["2.1.39", "2.1.40"], "latest": null, "how": "native install; updates itself"},
                         {"name": "Codex", "installed": "0.60.1", "latest": null, "how": "npm global @openai/codex"}],
            "pairs": {"version": "v6.1", "file": "/p/harness-models.json", "harnesses": ["claude-code"], "pairs": [{"model": "m", "provider": "p", "on": {"claude-code": false}}]},
            "error": null
        });
        reply(&mut s, 0, Box::new(Asked::Harness), Ok(served.to_string()));
        let h = s.ext.get::<Harnesses>();
        assert_eq!(h.versions.iter().map(|v| v.installed.as_str()).collect::<Vec<_>>(), ["2.1.40", "0.60.1"]);
        assert!(matches!(&h.pairs, Some(Ok(snap)) if snap.pairs.len() == 1 && snap.file == "/p/harness-models.json"));
        reply(&mut s, 0, Box::new(Asked::Latest), Ok("0.61.0\n".into()));
        assert_eq!(s.ext.get::<Harnesses>().versions[1].latest.as_deref(), Some("0.61.0"));
        // no pairs: coordd's why
        let none = serde_json::json!({"versions": [], "pairs": null, "error": "the proxy did not answer"});
        reply(&mut s, 0, Box::new(Asked::Harness), Ok(none.to_string()));
        assert!(matches!(&s.ext.get::<Harnesses>().pairs, Some(Err(e)) if e == "the proxy did not answer"));
        // a switch pressed there with no coordd that sets rules changes nothing
        crate::elsewhere::as_on_windows(|| set(&mut s, "claude-code", &[("m".into(), "p".into())], true));
        assert!(s.ext.get::<Harnesses>().error.is_none());
    }

    /// A switch goes to the first coordd that sets rules, on Windows and Linux alike; with none, Linux writes the file as
    /// before and Windows nowhere (N-802.p23).
    #[test]
    fn a_switch_goes_to_coordd_from_both_windows() {
        let r = coord_types::RULES_PROTOCOL;
        assert_eq!(setter(true, &[r - 1, r]), Setter::Coordd(1));
        assert_eq!(setter(false, &[r]), Setter::Coordd(0));
        assert_eq!(setter(false, &[r - 1]), Setter::File);
        assert_eq!(setter(true, &[r - 1]), Setter::Nowhere);
        assert_eq!(setter(true, &[]), Setter::Nowhere);
        let wire = |q: coord_types::Request| serde_json::to_value(q).unwrap();
        assert_eq!(wire(set_request("codex", &[("gpt-5".into(), "openai".into())], false, false)), serde_json::json!({"op": "harnessset", "harness": "codex", "pairs": [{"model": "gpt-5", "provider": "openai"}], "on": false}));
        assert_eq!(wire(set_request("codex", &[("x".into(), "y".into())], true, true)), serde_json::json!({"op": "harnessset", "harness": "codex", "on": false, "all_on": true}));
        // and read back as sent (docket-core3, N-802.p25)
        let back: coord_types::Request = serde_json::from_value(wire(set_request("codex", &[("gpt-5".into(), "openai".into())], true, false))).unwrap();
        assert!(matches!(&back, coord_types::Request::HarnessSet { harness, pairs, on: true, all_on: false } if harness == "codex" && pairs == &[coord_types::RulePair { model: "gpt-5".into(), provider: "openai".into() }]), "{back:?}");
    }

    /// coordd's refusal shows in its words and the switches are free again; a switch set waits for the read-back.
    #[test]
    fn the_page_shows_coordds_refusal_and_waits_for_the_read_back() {
        let (_held, mut s) = crate::view_cost::test_app();
        init(&mut s);
        s.ext.get_mut::<Harnesses>().setting = true;
        reply(&mut s, 0, Box::new(Asked::Set), Err("not set: /p/x.json is not a plain file (a link is not followed)".into()));
        let h = s.ext.get::<Harnesses>();
        assert_eq!(h.error.as_deref(), Some("not set: /p/x.json is not a plain file (a link is not followed)"));
        assert!(!h.setting);
        // set: the switches wait until coordd's Harness answer, what the proxy has in force, comes
        s.ext.get_mut::<Harnesses>().setting = true;
        let served = serde_json::json!({"versions": [], "pairs": {"file": "/p/harness-models.json", "harnesses": ["codex"], "pairs": [{"model": "m", "provider": "p", "on": {"codex": false}}]}, "error": null});
        reply(&mut s, 0, Box::new(Asked::Harness), Ok(served.to_string()));
        let h = s.ext.get::<Harnesses>();
        assert!(!h.setting && matches!(&h.pairs, Some(Ok(snap)) if snap.pairs[0].on["codex"] == false));
    }
    /// The page drawn after coordd's answer ANSWER: its texts.
    fn drawn(answer: serde_json::Value) -> Vec<String> {
        let (_held, mut s) = crate::view_cost::test_app();
        init(&mut s);
        reply(&mut s, 0, Box::new(Asked::Harness), Ok(answer.to_string()));
        let mut rt = rune::runtime::Runtime::new(s, |s: &App, _: &mut rune::Cx<App>| page(s, &Kit::neutral(false), "").1, (1400.0, 900.0), rune::Env::new(false, 1.0));
        rt.frame(0.0);
        rt.elements().into_iter().filter_map(|e| e.text).collect()
    }

    /// A project with no route to a proxy: the page shows its versions and says there is nothing to switch, no error.
    #[test]
    fn with_no_route_the_page_loads_and_says_there_is_no_proxy() {
        let t = drawn(serde_json::json!({"versions": [{"name": "Codex", "installed": "0.160.0", "kept": null, "latest": null, "how": "npm"}], "pairs": null, "error": null}));
        assert!(t.iter().any(|x| x == "Codex: 0.160.0"), "{t:?}");
        assert!(t.iter().any(|x| x == NO_PROXY), "{t:?}");
        assert!(t.iter().all(|x| !x.contains("No such file") && !x.contains("coordd's answer") && !x.starts_with("Account proxy")), "{t:?}");
    }

    /// The proxy writes an empty list as `null` (Go): every such field reads as empty and the grid is drawn.
    #[test]
    fn nulls_in_the_proxys_answer_read_as_empty() {
        let t = drawn(serde_json::json!({"versions": null, "error": null, "pairs": {"version": "v6.2", "file": "/p/harness-models.json", "harnesses": ["claude-code", "codex"],
            "rules": {"claude-code": null, "codex": [{"model": "m", "on": false}]},
            "pairs": [{"model": "m", "provider": "p", "owned_by": null, "on": null}, {"model": "n", "provider": "q", "on": {"codex": false}}]}}));
        assert!(t.iter().all(|x| !x.contains("coordd's answer") && !x.contains("invalid type")), "{t:?}");
        assert!(t.iter().any(|x| x == "Account proxy: v6.2"), "{t:?}");
        assert!(t.iter().any(|x| x.starts_with("claude-code (2/2)")), "{t:?}");
        let (v, p) = served(r#"{"versions":[{"name":"Claude Code","installed":null,"kept":null,"how":null}],"pairs":{"file":null,"harnesses":null,"rules":null,"pairs":null}}"#).unwrap();
        assert_eq!((v[0].kept.len(), p.map(|p| p.pairs.len())), (0, Ok(0)));
    }
}
