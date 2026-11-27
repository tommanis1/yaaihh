//! Overview's cards: one surface for every card and panel, a wrapping grid, and the repository, agent and project cards.

use crate::home::RepoState;
use crate::{App, Proj};
use coord_types::AgentView;
use rune::{El, Icon, col, icon as glyph, row, text};
use rune_kit::glyphs::{FILE_TEXT, FOLDER};
use rune_kit::{Button, ButtonVariant, ControlSize, IconButton, IconButtonVariant, Kit};

/// A repository's or project's card width; an agent's.
const WIDE: f32 = 236.0;
const SMALL: f32 = 212.0;

/// The surface every card and panel stands on.
pub fn surface(k: &Kit) -> El {
    col().gap(6.0).padding(12.0).radius(k.radius.md).fill(k.c.layer_01).border(1.0, k.c.border_weak)
}

/// A panel of the lower half: a surface the width it is given, its heading and body inside.
pub fn panel(k: &Kit, body: El) -> El {
    surface(k).w_full().padding(14.0).child(body)
}

/// Cards side by side, onto a new line when the width runs out.
pub fn grid(key: &str, cards: Vec<El>) -> El {
    row().key(key.to_string()).w_full().flex_wrap().gap(12.0).items_start().children(cards)
}

/// A small icon button on a card, keyed KEY, that does GO.
fn icon(k: &Kit, glyph: &'static rune::Icon, label: &str, key: String, go: impl Fn(&mut App) + 'static) -> El {
    IconButton::new(glyph).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label(label).help(label).on_press(go).build(k).key(key)
}

/// What a repository's card says under its name: its branch and how many files changed, or that it was not read.
pub fn indicator(st: &RepoState) -> Option<String> {
    if st.refused.is_some() {
        return Some("not read".into());
    }
    let changed = st.dirty.filter(|d| *d > 0).map(|d| format!("{d} changed"));
    let v: Vec<String> = st.branch.clone().into_iter().chain(changed).collect();
    (!v.is_empty()).then(|| v.join(" · "))
}

/// A git branch, drawn before a repository's branch.
static BRANCH: Icon = Icon {
    name: "yaaihh-branch",
    view: 16.0,
    body: r#"<circle cx="5" cy="3.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="5" cy="12.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="11" cy="5.5" r="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M5 5V11M11 7C11 9.5 5 8.5 5 11" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#,
};

/// Repository NAME of project PI at FOLDER: a press opens it in Files, the code view where coordd serves it
/// (`home::show_files`), as its folder icon does; its PDF a chip that opens it at once; under the name its branch, and
/// a dot where agents start.
pub fn repo(s: &App, pi: usize, name: &str, folder: &str, marked: bool, about: Option<String>, st: Option<&RepoState>, k: &Kit) -> El {
    let dot = marked.then(|| text("●").font(k.ty.small).color(k.c.text_accent).key(format!("home-start-dot-{name}")).help("New agents start here"));
    let opens = !folder.is_empty() || crate::code::served(s, pi);
    let mut icons = row().gap(4.0).center_y();
    if let Some(pdf) = st.and_then(|x| x.pdf.clone()).filter(|_| !folder.is_empty()) {
        let (n, f, file) = (name.to_string(), folder.to_string(), pdf.rsplit('/').next().unwrap_or(&pdf).to_string());
        let chip = Button::new("PDF").icon(&FILE_TEXT).variant(ButtonVariant::Outline).control_size(ControlSize::Small).help(format!("Open {file}"));
        icons = icons.child(chip.on_press(move |s: &mut App| crate::files::open_file(s, &n, &f, &pdf)).build(k).key(format!("home-pdf-{name}")));
    }
    if opens {
        let (n, f) = (name.to_string(), folder.to_string());
        icons = icons.child(icon(k, &FOLDER, "Files", format!("home-files-{name}"), move |s| crate::home::show_files(s, pi, &n, &f)));
    }
    let title = text(name.to_string()).font(k.ty.body_medium).color(k.c.text_base).truncate().shrink();
    let head = row().w_full().gap(6.0).center_y().children(dot).child(col().grow(1.0).shrink().min_w(0.0).child(title)).child(icons);
    let quiet = st.and_then(indicator).map(|w| {
        let branch = st.is_some_and(|x| x.branch.is_some() && x.refused.is_none()).then(|| glyph(&BRANCH, 12.0).color(k.c.text_faint).key(format!("home-branch-{name}")));
        let said = st.and_then(|x| x.subject.as_ref().map(|j| format!("{}: {j}", x.head.as_deref().unwrap_or("last commit"))));
        let line = row().gap(4.0).center_y().min_w(0.0).children(branch).child(text(w).font(k.ty.small).color(k.c.text_faint).truncate().shrink());
        match said {
            Some(h) => line.help(h),
            None => line,
        }
    });
    let card = surface(k).key(format!("home-repo-{name}")).w(WIDE).child(head).children(quiet);
    if !opens {
        return card;
    }
    let (n, f) = (name.to_string(), folder.to_string());
    k.interactive(card).help(about.unwrap_or_else(|| "Open in Files".into())).on_press(move |s: &mut App| crate::home::show_files(s, pi, &n, &f))
}

/// The colour of an agent's state dot: working, waiting on someone, or at rest.
fn state_colour(state: &str, k: &Kit) -> rune::Color {
    match state {
        "working" => k.c.success.icon,
        "idle" => k.c.text_faint,
        _ => k.c.warning.icon,
    }
}

/// Agent A of project PI: its state as a dot, its name, context ring and model; a press opens it.
pub fn agent(s: &App, pi: usize, a: &AgentView, k: &Kit) -> El {
    let dot = text("●").font(k.ty.small).color(state_colour(&a.state, k)).help(a.state.clone());
    let name = text(a.name.clone()).font(k.ty.body_medium).color(k.c.text_base).truncate().shrink();
    let head = row().w_full().gap(6.0).center_y().child(dot).child(col().grow(1.0).shrink().min_w(0.0).child(name)).children(crate::circle::control(s, pi, a, k));
    let under = text(crate::projects::plain_model(&a.model)).font(k.ty.small).color(k.c.text_faint).truncate();
    let open = a.name.clone();
    k.interactive(surface(k).key(format!("home-agent-{}", a.name)).w(SMALL).child(head).child(under)).on_press(move |s: &mut App| s.open_in(pi, &open))
}

/// Whether project I is not connected: no reader of its own, and no pipe or a pipe that went down.
pub fn down(s: &App, i: usize) -> bool {
    s.projects.get(i).is_some_and(|q| !crate::machine::reads(s, i) && (q.link.is_none() || q.down.is_some()))
}

/// Project Q, the I-th: its name, its agents, and whether it is not connected; a press opens its Home.
pub fn project(s: &App, q: &Proj, i: usize, k: &Kit) -> El {
    let running = crate::agents::active(q).count();
    let stopped = q.live.agents.iter().filter(|a| a.state == "stopped").count();
    let name = text(q.name.clone()).font(k.ty.body_medium).color(k.c.text_base).truncate();
    let count = text(crate::parts::count(running, stopped)).font(k.ty.small).color(k.c.text_faint).truncate();
    let down = down(s, i).then(|| text("not connected").font(k.ty.small).color(k.c.text_faint).key(format!("global-down-{i}")));
    k.interactive(surface(k).key(format!("global-project-{i}")).w(WIDE).child(name).child(count).children(down)).on_press(move |s: &mut App| {
        s.cur = i;
        s.sel = crate::home::sel(&s.projects[i]);
    })
}
