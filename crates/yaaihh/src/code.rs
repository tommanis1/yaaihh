//! The code view, the project's Files where its coordd serves it: every repository of the project in one workspace of
//! coordd's Code-OSS, inside a pane of the window. Nothing starts until he uses it: a press on a repository, a file or
//! a commit, or Files shown, asks coordd, which starts its server. The view made is kept while a pane shows it and,
//! hidden, for the shown project a while after (`keep`), so a press then only queues the open; every other view is
//! closed and its coordd told, which stops its server. A PDF is shown by Files' own view (`shows`).

use crate::{App, Sel};
use coord_types::codeview::{CodeLook, CodeLooks, CodeTarget, CodeViewAnswer};
use coord_types::Request;
use rune::{col, row, text, El, WebSurface};
use rune_kit::{Button, ButtonVariant, Kit};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

/// How long the shown project's view is kept, hidden, with no pane showing it: a look elsewhere and back finds it at
/// once; past it the view is closed and its coordd told, so its server (about 220 MB) is let go.
pub const KEPT_HIDDEN: Duration = Duration::from_secs(5 * 60);

/// A project's workspace page: its address, its origin, the project's id, the theme its workbench was given and when
/// a pane last showed it.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    pub url: String,
    pub origin: String,
    pub project: String,
    pub dark: bool,
    pub seen: Instant,
}

/// The code view's asks and the workspaces opened.
#[derive(Default)]
pub struct Code {
    /// The ask under way: its project, whether its answer shows Files, and coordd's answer to come.
    pub asked: Option<(usize, bool, Receiver<Result<String, String>>)>,
    /// The theme the ask under way gave, dark or not.
    asked_dark: bool,
    /// Each workspace opened, by its file.
    pub open: BTreeMap<String, Opened>,
    /// The projects whose coordd was asked for the code view and not told the window left it, by id.
    pub started: BTreeSet<String>,
    /// Why the last open did not happen, and the project whose Files asked it unpressed.
    pub error: Option<String>,
    failed: Option<String>,
    /// The system's look as the last frame read it (lib.rs), for a window that follows it.
    pub system_dark: Cell<Option<bool>>,
}

/// Whether project PI's coordd has the code view.
pub fn served(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::CODE_VIEW_PROTOCOL)
}

/// Whether project PI's Files is the code view: its coordd serves it, and the file Files shows is not a PDF, which
/// Files' own view draws.
pub fn shows(s: &App, pi: usize) -> bool {
    served(s, pi) && !s.projects.get(pi).and_then(|p| s.files.kept.get(&p.id)).and_then(|k| k.shown.as_deref()).is_some_and(|f| crate::files::is_pdf(std::path::Path::new(f)))
}

/// Asks project PI's coordd to open TARGET in the code view; its answer shows Files (`ended`), the code view and no
/// longer a PDF Files showed.
pub fn open(s: &mut App, pi: usize, target: CodeTarget) {
    if let Some(k) = s.projects.get(pi).and_then(|p| s.files.kept.get_mut(&p.id)) {
        if k.shown.as_deref().is_some_and(|f| crate::files::is_pdf(std::path::Path::new(f))) {
            k.shown = None;
        }
    }
    if let Err(e) = ask(s, pi, target, true) {
        s.code.error = Some(e);
    }
}

/// The window's look: his, else the system's.
fn dark(s: &App) -> bool {
    s.dark.or(s.code.system_dark.get()).unwrap_or(true)
}

/// The projects whose code view a pane shows now, the focused one's and the others'.
fn in_sight(s: &App) -> BTreeSet<String> {
    let files = |sel: &Sel| matches!(sel, Sel::Files | Sel::Code(_));
    let mut out = BTreeSet::new();
    if files(&s.sel) && shows(s, s.cur) {
        out.extend(s.projects.get(s.cur).map(|p| p.id.clone()));
    }
    for (j, (pi, sel)) in s.panes.iter().enumerate() {
        if j != s.focus && files(sel) && shows(s, *pi) {
            out.extend(s.projects.get(*pi).map(|p| p.id.clone()));
        }
    }
    out
}

/// The code view kept to what he uses, on each poll: the views a pane shows, and the shown project's hidden for
/// KEPT_HIDDEN; every other view closed and its coordd told the window left it. A pane showing Files with no view asks
/// for its workspace (a coordd from CODE_WARM_PROTOCOL), one at a time; a view in another look than the window's is
/// asked again in it.
/// Whether a view was closed, so a frame lets it go.
pub fn keep(s: &mut App) -> bool {
    let now = Instant::now();
    let sight = in_sight(s);
    let cur = s.projects.get(s.cur).map(|p| p.id.clone()).unwrap_or_default();
    for o in s.code.open.values_mut().filter(|o| sight.contains(&o.project)) {
        o.seen = now;
    }
    let gone: Vec<String> = s.code.open.iter().filter(|(_, o)| !sight.contains(&o.project) && (o.project != cur || now.duration_since(o.seen) >= KEPT_HIDDEN)).map(|(f, _)| f.clone()).collect();
    let closed = !gone.is_empty();
    for f in gone {
        s.code.open.remove(&f);
        rune::web_surface::close(&format!("code:{f}"));
    }
    let pending = s.code.asked.as_ref().and_then(|(pi, ..)| s.projects.get(*pi)).map(|p| p.id.clone());
    let left: Vec<String> = s.code.started.iter().filter(|id| !s.code.open.values().any(|o| o.project == **id) && pending.as_ref() != Some(*id)).cloned().collect();
    for id in left {
        s.code.started.remove(&id);
        if let Some(p) = s.projects.iter_mut().find(|p| p.id == id && p.protocol >= coord_types::CODE_WARM_PROTOCOL) {
            let _ = p.ask_roster(&Request::CodeViewLeft);
        }
    }
    if s.code.asked.is_some() {
        return closed;
    }
    let warm = |p: &crate::Proj| p.link.is_some() && p.protocol >= coord_types::CODE_WARM_PROTOCOL;
    let order = std::iter::once(s.cur).chain(s.panes.iter().map(|(pi, _)| *pi));
    let want = order.filter(|pi| s.projects.get(*pi).is_some_and(|p| warm(p) && sight.contains(&p.id) && s.code.failed.as_ref() != Some(&p.id)) && of(s, *pi).is_none()).next();
    let d = dark(s);
    let again = s.code.open.values().find(|o| o.dark != d).and_then(|o| s.projects.iter().position(|p| p.id == o.project && warm(p)));
    if let Some(pi) = want {
        if let Err(e) = ask(s, pi, CodeTarget::Workspace, false) {
            s.code.error = Some(e);
            s.code.failed = Some(s.projects[pi].id.clone());
        }
    } else if let Some(pi) = again {
        let _ = ask(s, pi, CodeTarget::Workspace, false);
    }
    closed
}

/// TARGET asked of project PI's coordd in the window's look and colours; SHOW: its answer shows Files.
fn ask(s: &mut App, pi: usize, target: CodeTarget, show: bool) -> Result<(), String> {
    let dark = dark(s);
    let theme = Some(if dark { "dark" } else { "light" }.to_string());
    let looks = Some(CodeLooks { dark: look(&crate::look::kit(s, true)), light: look(&crate::look::kit(s, false)) });
    let p = s.projects.get_mut(pi).ok_or("no such project")?;
    let rx = p.ask_roster(&Request::CodeView { target, theme, looks }).ok_or_else(|| format!("{} is not reached now.", p.name))?;
    s.code.started.insert(p.id.clone());
    s.code.asked = Some((pi, show, rx));
    s.code.asked_dark = dark;
    if show {
        s.code.error = None;
        s.code.failed = None;
    }
    Ok(())
}

/// K's colours as the workbench takes them.
fn look(k: &Kit) -> CodeLook {
    let c = &k.c;
    CodeLook { bg: c.bg_base.hex(), layer: c.layer_01.hex(), raised: c.layer_02.hex(), text: c.text_base.hex(), muted: c.text_muted.hex(), faint: c.text_faint.hex(), border: c.border_weak.hex(), accent: c.text_accent.hex(), selection: k.selection().hex() }
}

/// coordd's answer, once given: its workspace kept (moved to the new page when its server moved), its view made, Files
/// shown when asked by a press; an unpressed answer for a project no pane shows any more makes nothing. True when it
/// answered.
pub fn ended(s: &mut App) -> bool {
    let Some((pi, show, r)) = s.code.asked.as_ref().and_then(|(pi, show, rx)| rx.try_recv().ok().map(|r| (*pi, *show, r))) else { return false };
    s.code.asked = None;
    let project = s.projects.get(pi).map(|p| p.id.clone()).unwrap_or_default();
    let wanted = show || in_sight(s).contains(&project) || s.code.open.values().any(|o| o.project == project);
    match r.and_then(|out| answer(&out)) {
        Ok(_) if !wanted => {}
        Ok(a) => {
            let dark = s.code.asked_dark;
            let seen = Instant::now();
            match s.code.open.get_mut(&a.folder) {
                Some(o) if o.origin == a.origin => o.dark = dark,
                _ => {
                    s.code.open.insert(a.folder.clone(), Opened { url: a.url, origin: a.origin, project: project.clone(), dark, seen });
                }
            }
            if let Some(o) = s.code.open.get(&a.folder) {
                rune::web_surface::warm(surface(&a.folder, o));
            }
            if s.code.failed.as_ref() == Some(&project) {
                (s.code.failed, s.code.error) = (None, None);
            }
            if show {
                s.open_pane_beside(pi, Sel::Files);
            }
        }
        Err(e) if show => s.code.error = Some(e),
        Err(e) => {
            if pi == s.cur && of(s, pi).is_none() {
                s.code.error = Some(e);
                s.code.failed = Some(project);
            }
        }
    }
    true
}

/// OUT as coordd's answer, with its page on its origin and that origin on 127.0.0.1; else why not.
fn answer(out: &str) -> Result<CodeViewAnswer, String> {
    let a: CodeViewAnswer = serde_json::from_str(out).map_err(|_| "The code view gave no address.".to_string())?;
    let on_origin = a.url.strip_prefix(&a.origin).is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?']));
    if !rune::web_surface::loopback(&a.origin) || !on_origin {
        return Err("Code view not on this machine.".into());
    }
    Ok(a)
}

/// What the pages said: that their view could not start.
pub fn heard(s: &mut App) -> bool {
    let said = rune::web_surface::take_messages();
    for (id, body) in &said {
        let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
        if id.starts_with("code:") && v["kind"] == "failed" {
            s.code.error = Some(format!("The code view could not start: {}", v["text"].as_str().unwrap_or("")));
        }
    }
    !said.is_empty()
}

/// Project PI's workspace page, by its file.
fn of(s: &App, pi: usize) -> Option<(&String, &Opened)> {
    let id = &s.projects.get(pi)?.id;
    s.code.open.iter().find(|(_, o)| o.project == *id)
}

/// Files of project PI in the code view: its workspace's web view, or why it is not shown yet and a press to open it.
pub fn page(s: &App, pi: usize, k: &Kit) -> El {
    let Some((folder, o)) = of(s, pi) else {
        let open = Button::new("Open it").variant(ButtonVariant::Ghost).on_press(move |s: &mut App| open(s, pi, CodeTarget::Workspace)).build(k).key("code-open-again");
        let why = match (&s.code.error, &s.code.asked) {
            (Some(e), _) => e.clone(),
            (None, Some((p, ..))) if *p == pi => "Opening the code view…".into(),
            _ => "Not open in this window.".into(),
        };
        return col().key("code-closed").w_full().h_full().padding(24.0).gap(12.0).child(text(why).font(k.ty.body).color(k.c.text_muted).wrap()).child(row().child(open));
    };
    let mut c = col().key("code-page").w_full().h_full();
    if let Some(e) = &s.code.error {
        c = c.child(text(e.clone()).font(k.ty.small).color(k.c.text_muted).px(12.0).py(4.0));
    }
    c.child(rune::el().key("code-view").w_full().grow(1.0).min_h(0.0).fill(k.c.bg_base).web_surface(surface(folder, o)))
}

/// The view of workspace FOLDER: one per workspace, its page and the script run in it.
fn surface(folder: &str, o: &Opened) -> WebSurface {
    WebSurface::new(format!("code:{folder}"), o.url.clone()).script(SCRIPT.to_string())
}

/// The script run in each page before its own (code/page.js): its hidden chrome, and the page unseen until its
/// workbench is in.
pub(crate) const SCRIPT: &str = include_str!("code/page.js");

#[cfg(test)]
mod tests;
