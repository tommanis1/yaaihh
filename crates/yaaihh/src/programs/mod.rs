//! The harness's programs, drawn (N-1988): the engine and the texts it runs named over one badge that says whether
//! they check (check.rs), every rule from what starts it to what it does (board.rs), the text of what is pressed beside
//! the drawing (source.rs), why a fact holds or an act is not enabled (live.rs), and an agent's own part in its Inspect
//! drawer (agent.rs). The engine's project draws the texts its projects file names, live from its record; a coordd (v1)
//! project its kinds' check, and on a press the texts this window was built with, which coordd-v2 runs in the release.

pub mod agent;
pub mod board;
pub mod check;
pub mod live;
pub mod read;
pub mod source;
#[cfg(test)]
mod tests;

use crate::{App, Proj};
use rune::{El, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit};
use serde_json::Value as Json;
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;

/// The texts the release's coordd-v2 loads, in its order (ops/release/cutover.sh's STD, then PKG).
pub const BUILT_IN: [(&str, &str); 18] = [
    ("std/hooks.docket", include_str!("../../../../docket/std/hooks.docket")),
    ("std/bridges/claude-code.docket", include_str!("../../../../docket/std/bridges/claude-code.docket")),
    ("std/bridges/codex.docket", include_str!("../../../../docket/std/bridges/codex.docket")),
    ("std/bridges/opencode.docket", include_str!("../../../../docket/std/bridges/opencode.docket")),
    ("std/bridges/agy.docket", include_str!("../../../../docket/std/bridges/agy.docket")),
    ("std/delivery.docket", include_str!("../../../../docket/std/delivery.docket")),
    ("std/comms.docket", include_str!("../../../../docket/std/comms.docket")),
    ("std/rails.docket", include_str!("../../../../docket/std/rails.docket")),
    ("std/budgets.docket", include_str!("../../../../docket/std/budgets.docket")),
    ("std/control.docket", include_str!("../../../../docket/std/control.docket")),
    ("std/adapt.docket", include_str!("../../../../docket/std/adapt.docket")),
    ("defaults/audit.docket", include_str!("../../../../docket/defaults/audit.docket")),
    ("defaults/train.docket", include_str!("../../../../docket/defaults/train.docket")),
    ("defaults/hygiene.docket", include_str!("../../../../docket/defaults/hygiene.docket")),
    ("defaults/brain.docket", include_str!("../../../../docket/defaults/brain.docket")),
    ("defaults/desk.docket", include_str!("../../../../docket/defaults/desk.docket")),
    ("defaults/project.docket", include_str!("../../../../docket/defaults/project.docket")),
    ("defaults/v1.docket", include_str!("../../../../docket/defaults/v1.docket")),
];

/// The module the release's engine reads its queries in (`--top`).
pub const TOP: &str = "yaaihh.project";

/// The keys in `pic_open` of what is picked on the page: a thing or rule lit (a node's key, a row's `row:MODULE:LINE`),
/// the text shown beside the drawing (`PATH\tLINE`, 0 the whole text), the whole text rather than the declaration,
/// what an explanation is asked of, the Fact whose instances are listed, the built-in texts shown on a coordd project.
pub const PICK: &str = "programs-pick:";
pub const SOURCE: &str = "programs-source:";
pub const WHOLE: &str = "programs-whole";
pub const WHY: &str = "programs-why:";
pub const HELD: &str = "programs-held:";
pub const BUILT: &str = "programs-built-in";
/// The height the drawing and the text beside it take, each scrolling in its own: the page does not scroll.
pub const VIEW_H: f32 = 720.0;

/// Where a stack's texts come from: the files the engine's projects file names, or this window's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Engine,
    BuiltIn,
}

/// A stack of texts as the page draws it: where they come from, each text's path and words, what the check found and
/// the drawing, the module queries are read in.
pub struct Built {
    pub from: Origin,
    pub texts: Vec<(String, String)>,
    pub checked: check::Checked,
    pub top: String,
    /// The folder every text is in, which their names are shown without.
    base: String,
}

impl Built {
    pub fn of(from: Origin, texts: Vec<(String, Result<String, String>)>, top: &str) -> Built {
        let checked = check::check(&texts);
        let texts: Vec<(String, String)> = texts.into_iter().map(|(p, t)| (p, t.unwrap_or_default())).collect();
        let first = texts.first().map_or("", |(p, _)| p.as_str());
        let base = first.match_indices('/').map(|(i, _)| &first[..=i]).filter(|d| texts.iter().all(|(p, _)| p.starts_with(d))).last().unwrap_or("").to_string();
        Built { from, texts, checked, top: top.to_string(), base }
    }

    /// PATH as it is shown: without the folder all the texts are in.
    pub fn short<'p>(&self, path: &'p str) -> &'p str {
        path.strip_prefix(self.base.as_str()).unwrap_or(path)
    }

    /// The window's own texts.
    pub fn built_in() -> Built {
        Built::of(Origin::BuiltIn, BUILT_IN.iter().map(|(p, t)| (p.to_string(), Ok(t.to_string()))).collect(), TOP)
    }

    /// The words of the text at PATH.
    pub fn words(&self, path: &str) -> Option<&str> {
        self.texts.iter().find(|(p, _)| p == path).map(|(_, t)| t.as_str())
    }

    /// The path of module M's text.
    pub fn path(&self, m: &str) -> String {
        self.checked.paths.get(m).cloned().unwrap_or_else(|| m.to_string())
    }

    /// The module of FULL, a declaration's full name, and its own name.
    fn split<'f>(&self, full: &'f str) -> Option<(&(String, docket_v2::syntax::tree::Module), &'f str)> {
        let m = self.checked.modules.iter().filter(|(n, _)| full.strip_prefix(n.as_str()).is_some_and(|r| r.starts_with('.'))).max_by_key(|(n, _)| n.len())?;
        Some((m, &full[m.0.len() + 1..]))
    }

    /// The path and line of the declaration FULL names.
    pub fn decl(&self, full: &str) -> Option<(String, u32)> {
        let ((m, module), name) = self.split(full)?;
        let d = module.decls.iter().find(|d| d.what.name() == Some(name))?;
        Some((self.path(m), d.at.line))
    }

    /// FULL, a declaration's full name, as the top module names it: what `explain` reads.
    pub fn in_top(&self, full: &str) -> Option<String> {
        let top = self.checked.modules.iter().find(|(n, _)| *n == self.top)?;
        let ((_, module), name) = self.split(full)?;
        Some(format!("{}{name}", crate::policy_v2::plan::prefix(&top.1, module)?))
    }
}

/// The page's stacks, each made once: the window's own, and the engine's while its texts and its last load stay.
#[derive(Default)]
pub struct State {
    built_in: OnceCell<Rc<Built>>,
    engine: RefCell<Option<(Vec<String>, Option<String>, Rc<Built>)>>,
}

pub fn built_in(s: &App) -> Rc<Built> {
    s.programs.built_in.get_or_init(|| Rc::new(Built::built_in())).clone()
}

/// The texts the engine's projects file names, read and checked, with a finding when they are not the text the
/// engine's record says it loaded last.
pub fn engine(s: &App) -> Option<Rc<Built>> {
    let v = s.ext.try_get::<crate::v2::V2>()?;
    let c = v.config.as_ref()?;
    let loaded = v.desk.loaded_line().map(str::to_string);
    let mut held = s.programs.engine.borrow_mut();
    if let Some((t, l, b)) = &*held {
        if *t == c.texts && *l == loaded {
            return Some(b.clone());
        }
    }
    let texts = c.texts.iter().map(|p| (p.clone(), std::fs::read_to_string(p).map_err(|e| format!("cannot be read: {e}")))).collect();
    let mut b = Built::of(Origin::Engine, texts, &c.top);
    let hash = b.checked.loaded.as_ref().map(|l| l.hash.iter().map(|x| format!("{x:02x}")).collect::<String>());
    if let (Some(h), Some(l)) = (&hash, &loaded) {
        if h != l {
            let short = |h: &str| h.chars().take(12).collect::<String>();
            let path = c.texts.first().cloned().unwrap_or_default();
            let why = format!("these files are not the text the engine runs: its record's last load is {}, these files hash to {}", short(l), short(h));
            b.checked.diags.insert(0, check::Diag { level: check::Level::Error, path, line: 0, why });
        }
    }
    let b = Rc::new(b);
    *held = Some((c.texts.clone(), loaded, b.clone()));
    Some(b)
}

/// What the engine says now: its record's tally, and its answers to `holding` and `explain`, each asked again after a
/// line that moves the facts.
pub struct Live<'a> {
    pub tally: &'a live::Tally,
    pub holding: &'a dyn Fn(&str) -> Option<Result<Json, String>>,
    pub explain: &'a dyn Fn(&str) -> Option<Result<Json, String>>,
}

/// The one value of PREFIX picked.
pub fn picked<'s>(s: &'s App, prefix: &str) -> Option<&'s str> {
    crate::picture::docket::picked(&s.tickets.pic_open, prefix)
}

/// PREFIX set to V, or taken away.
pub fn set(s: &mut App, prefix: &str, v: Option<String>) {
    s.tickets.pic_open.retain(|k| !k.starts_with(prefix));
    if let Some(v) = v {
        s.tickets.pic_open.insert(format!("{prefix}{v}"));
    }
}

/// KEY lit and the text at PATH shown at LINE; the same pressed again puts both away.
pub fn pick(s: &mut App, key: &str, path: &str, line: u32) {
    let again = picked(s, PICK) == Some(key);
    set(s, PICK, (!again).then(|| key.to_string()));
    show(s, (!again).then_some((path, line)));
}

/// The text at PATH shown beside the drawing at LINE (0: whole), or none; what it listed and explained put away.
pub fn show(s: &mut App, at: Option<(&str, u32)>) {
    set(s, SOURCE, at.map(|(p, l)| format!("{p}\t{l}")));
    set(s, HELD, None);
    set(s, WHY, None);
}

/// The text shown beside the drawing and its line.
pub fn shown(s: &App) -> Option<(String, u32)> {
    let (p, l) = picked(s, SOURCE)?.split_once('\t')?;
    Some((p.to_string(), l.parse().ok()?))
}

/// The engine's project's Docket: its texts drawn, live, in place of the kinds a coordd serves; none for another
/// project.
pub fn engine_page(s: &App, p: &Proj, k: &Kit) -> Option<El> {
    let v = s.ext.try_get::<crate::v2::V2>()?;
    if v.at.and_then(|i| s.projects.get(i)).map(|q| &q.id) != Some(&p.id) {
        return None;
    }
    let b = engine(s)?;
    let holding = |n: &str| crate::v2::holding(s, n);
    let explain = |i: &str| crate::v2::explain(s, i);
    let live = Live { tally: &v.desk.tally, holding: &holding, explain: &explain };
    let state = match (&v.refused, &v.down, v.up) {
        (Some(why), _, _) => format!("refused to show its record: {why}"),
        (None, _, true) => format!("{} lines of its record", v.desk.len()),
        (None, Some(why), false) => format!("not connected: {why}"),
        (None, None, false) => "connecting".into(),
    };
    Some(col().w_full().gap(12.0).padding(24.0).child(page(s, &b, Some(&live), &state, k)))
}

/// A coordd (v1) project's programs over its kinds: the engine and its kinds named, their check, and a press for the
/// texts coordd-v2 runs in the release, drawn as the engine's are.
pub fn v1(s: &App, p: &Proj, k: &Kit) -> El {
    let small = |t: String| text(t).font(k.ty.small).color(k.c.text_muted).wrap();
    let mut diags: Vec<check::Diag> = p.live.refused().map(|e| v1_diag(e)).collect();
    let named = match &s.tickets.kinds {
        Some(Ok(ks)) => {
            let (_, bad) = crate::picture::docket::programs(ks);
            for f in ks.iter().filter(|f| bad.contains(&f.kind)) {
                diags.push(check::Diag { level: check::Level::Error, path: f.file.clone(), line: 0, why: format!("kind {} does not parse here", f.kind) });
            }
            let files: std::collections::BTreeSet<&str> = ks.iter().map(|f| f.file.as_str()).collect();
            format!("coordd (v1) · {} kinds in {} files, with its policy (coordd checks it; the window is not sent its text)", ks.len(), files.len())
        }
        Some(Err(e)) => format!("coordd (v1) · its kinds could not be read: {e}"),
        None => "coordd (v1) · its kinds are on their way".into(),
    };
    let on = picked(s, BUILT).is_some();
    let toggle = Button::new(if on { "Hide the v2 harness" } else { "The v2 harness, as the release runs it" })
        .variant(ButtonVariant::Outline)
        .help("The texts coordd-v2 loads in the release")
        .on_press(move |s: &mut App| set(s, BUILT, (!on).then(String::new)))
        .build(k)
        .key("programs-built-in");
    let head = row().w_full().gap(10.0).center_y().child(text("Engine").font(k.ty.small_medium).color(k.c.text_base)).child(text(named).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink()).child(badge(&diags, k));
    let mut c = col().w_full().gap(8.0).key("programs-v1").child(head).children(diag_rows(&diags, None, k));
    c = c.child(row().gap(10.0).center_y().child(toggle).child(small("Its rules, from the harness moments that start them to what they do".into())));
    if on {
        c = c.child(page(s, &built_in(s), None, "built into this window", k));
    }
    c
}

/// One of coordd's refusals as a finding, at the kind file and line it names first (`NAME.kind:LINE`) when it names one.
fn v1_diag(e: &str) -> check::Diag {
    let at = e.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',').find_map(|w| {
        let (f, rest) = w.trim_end_matches(':').split_once(':')?;
        let line: u32 = rest.split(':').next()?.parse().ok()?;
        (f.ends_with(".kind") || f.ends_with(".eflint")).then(|| (f.to_string(), line))
    });
    let (path, line) = at.unwrap_or_default();
    check::Diag { level: check::Level::Error, path, line, why: e.to_string() }
}

/// The badge over the findings: Validated, or how many errors and notes.
pub fn badge(diags: &[check::Diag], k: &Kit) -> El {
    let n = |l: check::Level| diags.iter().filter(|d| d.level == l).count();
    let pill = |w: String, c: &rune_kit::StateColors| row().px(8.0).py(2.0).radius(9.0).fill(c.bg).border(1.0, c.border).child(text(w).font(k.ty.small_medium).color(c.fg));
    let (errors, notes) = (n(check::Level::Error), n(check::Level::Note));
    let mut r = row().gap(6.0).key("programs-badge");
    if errors + notes == 0 {
        return r.child(pill("Validated".into(), &k.c.success).key("programs-validated"));
    }
    if errors > 0 {
        r = r.child(pill(format!("{errors} {}", if errors == 1 { "error" } else { "errors" }), &k.c.danger).key("programs-errors"));
    }
    if notes > 0 {
        r = r.child(pill(format!("{notes} {}", if notes == 1 { "note" } else { "notes" }), &k.c.warning).key("programs-notes"));
    }
    r
}

/// Each finding as a row: its level, its place (a press shows the text there, when B holds it), why.
pub fn diag_rows(diags: &[check::Diag], b: Option<&Built>, k: &Kit) -> Vec<El> {
    diags
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let ink = if d.level == check::Level::Error { k.c.danger.fg } else { k.c.warning.fg };
            let shown = b.map_or(d.path.as_str(), |b| b.short(&d.path));
            let place = if d.line == 0 { shown.to_string() } else { format!("{shown}:{}", d.line) };
            let mut at = text(place).font(k.ty.mono).color(ink);
            if b.is_some_and(|b| b.words(&d.path).is_some_and(|w| !w.is_empty())) {
                let (path, line) = (d.path.clone(), d.line);
                at = k.interactive(at.key(format!("programs-diag-{i}"))).on_press(move |s: &mut App| show(s, Some((&path, line))));
            }
            row().w_full().gap(8.0).child(at).child(text(d.why.clone()).font(k.ty.small).color(k.c.text_base).wrap().grow(1.0).shrink())
        })
        .collect()
}

/// Stack B drawn: the engine and the texts named, the badge, the findings, the refusals of late, and the drawing with
/// the text pressed and what is explained beside it.
fn page(s: &App, b: &Built, live: Option<&Live>, state: &str, k: &Kit) -> El {
    let engine = match b.from {
        Origin::Engine => format!("coordd-v2 · {} texts read in {} · {state}", b.texts.len(), b.top),
        Origin::BuiltIn => format!("coordd-v2 in the release · the {} texts it loads, {state}, read in {}", b.texts.len(), b.top),
    };
    let head = row().w_full().gap(10.0).center_y().child(text("Engine").font(k.ty.small_medium).color(k.c.text_base)).child(text(engine).font(k.ty.small).color(k.c.text_muted).wrap().grow(1.0).shrink()).child(badge(&b.checked.diags, k));
    let stack = row().w_full().gap(6.0).flex_wrap().children(b.texts.iter().enumerate().map(|(i, (p, _))| {
        let path = p.clone();
        k.interactive(row().px(6.0).py(2.0).radius(k.radius.sm).fill(k.c.layer_02).child(text(b.short(p).to_string()).font(k.ty.small).color(k.c.text_muted)).key(format!("programs-text-{i}"))).on_press(move |s: &mut App| show(s, Some((&path, 0))))
    }));
    let mut c = col().w_full().gap(10.0).key(format!("programs-{}", if b.from == Origin::Engine { "engine-stack" } else { "built-in-stack" })).child(head).child(stack);
    c = c.children(diag_rows(&b.checked.diags, Some(b), k));
    if b.from == Origin::BuiltIn {
        c = c.child(text("A project's own world (the import's world.docket) loads after these where the release made one; this window does not have it.").font(k.ty.small).color(k.c.text_faint).wrap());
    }
    if let Some(l) = live {
        c = c.children(source::refusals(b, l, k));
    }
    let drawn = col().w(0.0).grow(1.0).min_w(0.0).max_h(VIEW_H).scroll_xy().child(board::el(s, b, live, k));
    c.child(row().w_full().gap(16.0).child(drawn).children(source::panel(s, b, live, k)))
}
