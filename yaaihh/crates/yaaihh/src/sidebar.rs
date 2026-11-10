//! The sidebar (owner 7 Nov night), top to bottom:
//!
//! - **Pinned**: chats pinned from any project (the agent page's Pin button), so the owner moves between agents of
//!   different projects (a host session and a box's orch) in one click. A row's close button unpins it; dragging a
//!   row reorders the list. Kept in `~/.config/yaaihh/pins.json` (`[{"project": ID, "id": AGT-NNN, "agent": NAME}]`):
//!   a pin is the agent's id, like every other reference to an agent, so it follows a rename; the name is the last
//!   one seen, shown while the project is down. A pin written before ids (no `id`) takes its agent's id when the
//!   project is first seen.
//!   Dragged onto the main area, a pinned or running agent opens in a pane there (panes.rs, drag and drop).
//! - **Projects +**: the projects; "+" starts a new one.
//! - **the current project**, under its name: its views, Agents with the agents that run indented beneath it (a
//!   click opens one; Agents itself opens the tiles, and its All agents tab lists the stopped ones too), Mail and
//!   Questions. Questions, Mail and Clerk come before Agents, so a long agent list never pushes them off-screen.
//! - each extension's section (ext.rs).

use crate::{App, Sel, agents, ask, ext};
use rune::{El, Icon};
use rune_kit::shell::{Reorder, RowStatus, Sidebar, SidebarRow, SidebarSection};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};
use coord_types::AgentView;
use serde::{Deserialize, Serialize};

static PLUS: Icon = Icon { name: "yaaihh-plus", view: 16.0, body: r#"<path d="M8 3.5V12.5M3.5 8H12.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"# };

/// A pinned chat: the agent whose id is ID (last seen named AGENT) of the project whose id is PROJECT.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pin {
    pub project: String,
    #[serde(default)]
    pub id: String,
    pub agent: String,
}

impl Pin {
    /// Whether PIN is the same chat: same project and agent id (by name only while either has no id yet).
    fn same(&self, pin: &Pin) -> bool {
        self.project == pin.project && if self.id.is_empty() || pin.id.is_empty() { self.agent == pin.agent } else { self.id == pin.id }
    }

    /// The pinned agent among AGENTS: by id, or by name for a pin without one.
    fn find<'a>(&self, agents: &'a [AgentView]) -> Option<&'a AgentView> {
        agents.iter().find(|a| if self.id.is_empty() { a.name == self.agent } else { a.id == self.id })
    }
}

/// The pinned chats and the drag that reorders them.
#[derive(Default)]
pub struct Pins {
    pub list: Vec<Pin>,
    pub drag: Reorder,
}

fn pins_path() -> std::path::PathBuf {
    std::path::Path::new(&crate::projects::config_path()).with_file_name("pins.json")
}

impl Pins {
    pub fn load() -> Pins {
        let list = std::fs::read_to_string(pins_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Pins { list, drag: Reorder::default() }
    }

    fn save(&self) {
        let _ = std::fs::write(pins_path(), serde_json::to_string_pretty(&self.list).unwrap());
    }

    pub fn has(&self, pin: &Pin) -> bool {
        self.list.iter().any(|p| p.same(pin))
    }

    /// Pin PIN, or unpin it when pinned.
    pub fn toggle(&mut self, pin: Pin) {
        match self.list.iter().position(|p| p.same(&pin)) {
            Some(i) => _ = self.list.remove(i),
            None => self.list.push(pin),
        }
        self.save();
    }

    /// Bring the pins of project PROJECT up to its AGENTS: an id for a pin without one, the current name of a renamed
    /// agent; saved when something changed.
    pub fn follow(&mut self, project: &str, agents: &[AgentView]) {
        let mut changed = false;
        for pin in self.list.iter_mut().filter(|p| p.project == project) {
            if let Some(a) = pin.find(agents)
                && (pin.id != a.id || pin.agent != a.name)
            {
                (pin.id, pin.agent) = (a.id.clone(), a.name.clone());
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    fn moved(&mut self, from: usize, to: usize) {
        if from < self.list.len() && to < self.list.len() {
            let p = self.list.remove(from);
            self.list.insert(to, p);
            self.save();
        }
    }
}

impl App {
    /// The pin for the agent shown, if one is.
    pub fn shown_pin(&self) -> Option<Pin> {
        let Sel::Agent(name) = &self.sel else { return None };
        let p = self.proj()?;
        let id = p.live.agents.iter().find(|a| &a.name == name).map_or(String::new(), |a| a.id.clone());
        Some(Pin { project: p.id.clone(), id, agent: name.clone() })
    }

    /// Show agent AGENT of project I.
    pub fn open_in(&mut self, i: usize, agent: &str) {
        self.cur = i;
        self.open_agent(agent, false);
    }
}

pub fn sidebar(s: &App, k: &Kit) -> El {
    // built again only when something it shows changes, not for a keystroke or a frame elsewhere
    let mut v = crate::version::Version::default();
    v.debug(&s.pins.list).debug(&s.pins.drag).add(s.cur).debug(&s.sel);
    for e in ext::all() {
        v.add((e.nav_key)(s));
    }
    for q in &s.projects {
        v.add((&q.id, &q.name, &q.down)).debug(&q.live.agents);
    }
    if let Some(p) = s.proj() {
        v.add((p.live.seq, p.live.mail.len(), ask::open_count(s, p), crate::clerk::unseen(s, p), p.live.project_name(), p.live.home()));
        for spec in p.live.view_specs() {
            let id = spec.get("id").and_then(|x| x.as_str()).unwrap_or("");
            v.add((id, spec.get("title").and_then(|x| x.as_str()), p.live.views.get(id).and_then(|d| d.error.as_deref())));
        }
    }
    // the root keeps the key the Sidebar gives it
    rune::memo::memo_with_hash("sidebar", v.finish(), || built(s, k))
}

fn built(s: &App, k: &Kit) -> El {
    let plus = IconButton::new(&PLUS).control_size(ControlSize::Small).variant(IconButtonVariant::GhostMuted).label("New project").help("New project…");
    let plus = plus.build(k).on_press(|s: &mut App| s.new_project.open = true);
    let mut sb = Sidebar::new().label("yaaihh");
    // pinned chats, from any project
    let n = s.pins.list.len();
    if n > 0 {
        let rows = s.pins.list.iter().enumerate().map(|(at, pin)| {
            let found = s.projects.iter().position(|p| p.id == pin.project);
            let agent = found.and_then(|i| pin.find(&s.projects[i].live.agents));
            // the agent's name now (a rename since the pin was saved shows at once), else the last one seen
            let shown = agent.map_or(pin.agent.clone(), |a| a.name.clone());
            let place = found.map_or(format!("{} (no such project)", pin.project), |i| s.projects[i].name.clone());
            let subtitle = agent.map_or(place.clone(), |a| format!("{place} · {}", first_line(&a.state)));
            let selected = found == Some(s.cur) && s.sel == Sel::Agent(shown.clone());
            let (gone, open) = (pin.clone(), shown.clone());
            let mut row = SidebarRow::new(format!("{shown} · {}", if pin.id.is_empty() { "no id yet" } else { &pin.id }))
                .subtitle(subtitle)
                .height(40.0)
                .selected(selected)
                .reorder_or_out(&s.pins.drag, at, n, |s: &mut App| &mut s.pins.drag, |s: &mut App, from, to| s.pins.moved(from, to), {
                    let (pi, name) = (found, shown.clone());
                    move |s: &mut App, d, _| pi.is_some_and(|pi| s.drag_agent(pi, &name, d))
                })
                .on_close(move |s: &mut App, _| s.pins.toggle(gone.clone()));
            if let Some(i) = found {
                let side = open.clone();
                row = row.on_select(move |s: &mut App, _| s.open_in(i, &open)).on_double_click(move |s: &mut App, _| s.open_to_side(i, &side));
            }
            if agent.is_some_and(|a| a.state == "working") {
                row = row.status(RowStatus::Busy);
            }
            row.build(k).key(format!("pin-{}-{}", pin.project, if pin.id.is_empty() { &pin.agent } else { &pin.id }))
        });
        sb = sb.section(SidebarSection::new("Pinned").key("pinned").count(n).rows(rows));
    }
    // the projects
    let projects = s.projects.iter().enumerate().map(|(i, q)| {
        let link = match &q.down {
            None => format!("{} running", agents::active(q).count()),
            Some(why) => first_line(why),
        };
        let mut row = SidebarRow::new(q.name.clone()).subtitle(link).height(40.0).selected(i == s.cur).on_select(move |s: &mut App, _| s.cur = i);
        if q.down.is_some() {
            row = row.status(RowStatus::Attention);
        }
        row.build(k).key(format!("project-{i}"))
    });
    sb = sb.section(SidebarSection::new("Projects").key("projects").action(plus).rows(projects));
    let Some(p) = s.proj() else { return sb.build(k) };
    // the current project's pages
    let shown = match &s.sel {
        Sel::View(v) if v.is_empty() => p.live.home(),
        Sel::View(v) => v.clone(),
        _ => String::new(),
    };
    let mut rows = Vec::new();
    for v in p.live.view_specs() {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let title = v.get("title").and_then(|x| x.as_str()).unwrap_or(&id).to_string();
        let mut row = SidebarRow::new(title).selected(shown == id);
        match p.live.views.get(&id).and_then(|d| d.error.as_deref()) {
            Some(err) => row = row.status(RowStatus::Attention).subtitle(first_line(err)).height(40.0),
            None => row = row.height(32.0),
        }
        let key = format!("view-{id}");
        rows.push(row.on_select(move |s: &mut App, _| s.sel = Sel::View(id.clone())).build(k).key(key));
    }
    rows.push(
        SidebarRow::new("Questions")
            .badge(ask::open_count(s, p).to_string())
            .height(32.0)
            .selected(s.sel == Sel::Questions)
            .on_select(|s: &mut App, _| s.sel = Sel::Questions)
            .build(k)
            .key("questions"),
    );
    rows.push(
        SidebarRow::new("Mail")
            .badge(p.live.mail.len().to_string())
            .height(32.0)
            .selected(s.sel == Sel::Mail)
            .on_select(|s: &mut App, _| s.sel = Sel::Mail)
            .build(k)
            .key("mail"),
    );
    let unseen = crate::clerk::unseen(s, p);
    let mut clerk = SidebarRow::new("Clerk").height(32.0).selected(s.sel == Sel::Clerk).on_select(|s: &mut App, _| {
        s.sel = Sel::Clerk;
        crate::clerk::mark_seen(s);
        crate::clerk::load(s);
    });
    if unseen > 0 {
        clerk = clerk.badge(unseen.to_string()).status(RowStatus::Attention);
    }
    rows.push(clerk.build(k).key("clerk"));
    rows.push(
        SidebarRow::new("Agents")
            .badge(agents::active(p).count().to_string())
            .height(32.0)
            .selected(s.sel == Sel::Agents)
            .on_select(|s: &mut App, _| s.sel = Sel::Agents)
            .build(k)
            .key("agents"),
    );
    // the agents that run, beneath Agents, as their tree
    for (depth, a) in p.live.agent_tree().into_iter().filter(|(_, a)| a.state != "stopped") {
        let (name, side, cur) = (a.name.clone(), a.name.clone(), s.cur);
        let mut row = SidebarRow::new(a.name.clone())
            .subtitle(match a.role.as_str() {
                "" | "member" => format!("{} · {}", first_line(&a.state), a.model),
                role => format!("{role} · {} · {}", first_line(&a.state), a.model),
            })
            .depth(depth as u8 + 1)
            .height(40.0)
            .selected(s.sel == Sel::Agent(a.name.clone()))
            .on_select(move |s: &mut App, _| s.open_agent(&name, false))
            .on_double_click(move |s: &mut App, _| s.open_to_side(cur, &side))
            .on_drag({
                let name = a.name.clone();
                move |s: &mut App, d, _| _ = s.drag_agent(cur, &name, d)
            });
        if a.state == "working" {
            row = row.status(RowStatus::Busy);
        }
        rows.push(row.build(k).key(format!("agent-{}", a.name)));
    }
    let title = p.live.project_name().unwrap_or(&p.name).to_string();
    sb = sb.section(SidebarSection::new(title).key("project").rows(rows));
    for e in ext::all() {
        sb = sb.section((e.nav)(s, k));
    }
    let link = match &p.down {
        None => format!("live · event {}", p.live.seq),
        Some(why) => first_line(why),
    };
    sb.footer(SidebarRow::new(p.name.clone()).subtitle(link).height(40.0).build(k).key("link")).build(k)
}

/// T's first non-empty line: a row's subtitle is one line, and a multi-line one (a view source's stderr) would push its
/// text out of the row over the rows above.
pub fn first_line(t: &str) -> String {
    t.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string()
}
