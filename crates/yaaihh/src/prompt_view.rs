//! An agent's system prompt as its model is sent it, whole and readable, each section marked quietly by where it came
//! from (the owner's live trial, N-1864.p1: "the text marked up in the text where it came from subtly"): Claude Code's
//! own (harness built-in), the global and the project's standing.md (coordd's `Request::Standing`), and his override.

use crate::App;
use crate::prompt_parts::Part;
use coord_types::{GivenPrompt, StandingPart};
use rune::{El, col, row, text};
use rune_kit::Kit;
use std::collections::{HashMap, HashSet};

/// Where a section of the system prompt came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Claude Code's own text, as its newest request carried it.
    Harness,
    /// standing.md at the root of the project's repository: every project's agents get it.
    Global,
    /// The project's standing.md, beside its project file.
    Project,
    /// agent-base's standing.md, given where the project has none.
    AgentBase,
    /// The standing text, where the coordd says nothing of its files.
    Standing,
    /// His version: his words appended, or a part of Claude Code's own he changed.
    Override,
    /// A prompt pack a recipe added (roster's diff file, prompt_diff.rs `of_file`).
    Pack,
}

impl Source {
    /// Its marker's words.
    pub fn words(self) -> &'static str {
        match self {
            Source::Harness => "harness built-in",
            Source::Global => "global standing.md",
            Source::Project => "project standing.md",
            Source::AgentBase => "agent-base standing.md",
            Source::Standing => "standing text",
            Source::Override => "agent override",
            Source::Pack => "prompt pack",
        }
    }

    fn of(part: &StandingPart) -> Source {
        match part.from.as_str() {
            "global" => Source::Global,
            "project" => Source::Project,
            "agent-base" => Source::AgentBase,
            _ => Source::Standing,
        }
    }
}

/// One section of the system prompt: its text as sent and where it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub source: Source,
    pub text: String,
}

/// The system prompt's sections in the order sent: Claude Code's IDENTITY line, then each of PARTS (the main text's,
/// `prompt_parts::parts`), those named in OVERRIDDEN his; the appended text (the parts that are ours) cut at each of
/// the STANDING texts found in it, in their order, what lies between them his words when HIS (a version of his is
/// appended), else the standing text.
pub fn sections(identity: Option<&str>, parts: &[Part], overridden: &[String], his: bool, standing: &[StandingPart]) -> Vec<Section> {
    let mut out: Vec<Section> = identity.filter(|t| !t.trim().is_empty()).map(|t| Section { source: Source::Harness, text: t.trim().to_string() }).into_iter().collect();
    for p in parts.iter().filter(|p| !p.ours) {
        let source = if overridden.contains(&p.name) { Source::Override } else { Source::Harness };
        out.push(Section { source, text: p.text.clone() });
    }
    let appended = parts.iter().filter(|p| p.ours).map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n\n");
    let rest = if his { Source::Override } else { Source::Standing };
    let mut push = |source: Source, t: &str| {
        if !t.trim().is_empty() {
            out.push(Section { source, text: t.trim().to_string() });
        }
    };
    let mut at = 0;
    for part in standing {
        let want = part.text.trim();
        let Some(i) = appended[at..].find(want).filter(|_| !want.is_empty()) else { continue };
        push(rest, &appended[at..at + i]);
        push(Source::of(part), want);
        at += i + want.len();
    }
    push(rest, &appended[at..]);
    out
}

/// The standing texts FILES joined as roster joins them (the global one with a line after it); none when there are none.
pub fn joined(files: &[StandingPart]) -> Option<String> {
    let t: String = files.iter().map(|p| if p.from == "global" { format!("{}\n", p.text) } else { p.text.clone() }).collect();
    (!t.trim().is_empty()).then_some(t)
}

/// A source's quiet marker, keyed `prompt-from-{AT}`.
fn marker(source: Source, at: &str, k: &Kit) -> El {
    text(source.words()).font(k.ty.small).color(k.c.text_faint).single_line().key(format!("prompt-from-{at}"))
}

/// The harness's own before its first request: one muted line.
pub fn to_come(k: &Kit) -> El {
    text("Harness built-in prompt captured after first turn").font(k.ty.small).color(k.c.text_faint).wrap().key("prompt-to-come")
}

/// SECTIONS drawn: each run of one source under its quiet marker, a faint rule down its left, its text as Markdown.
pub fn drawn(sections: &[Section], k: &Kit) -> Vec<El> {
    let mut out: Vec<El> = Vec::new();
    let mut i = 0;
    while i < sections.len() {
        let source = sections[i].source;
        let run = sections[i..].iter().take_while(|s| s.source == source).count();
        let marker = marker(source, &i.to_string(), k);
        let texts = sections[i..i + run].iter().map(|s| rune_kit::markdown::Markdown::new(s.text.clone()).selectable(true).build(k));
        let body = col().w_full().gap(10.0).pl(12.0).border_sides([0.0, 0.0, 0.0, 2.0], k.c.border_weak).children(texts);
        out.push(col().key(format!("prompt-section-{i}")).w_full().gap(4.0).pt(if i == 0 { 4.0 } else { 14.0 }).child(row().child(marker)).child(body));
        i += run;
    }
    out
}

/// The standing texts each project's coordd gave (`Request::Standing`), by project id, and those asked; each agent's
/// given file (`Request::Given`) by project id and name.
#[derive(Default)]
pub struct Standing {
    asked: HashSet<String>,
    held: HashMap<String, Vec<StandingPart>>,
    given: HashMap<(String, String), Held>,
}

/// An agent's given file as asked: the agent's `since` it was asked at (a new start asks again), whether an ask is on
/// its way, and the last answer.
#[derive(Default)]
struct Held {
    since: String,
    waiting: bool,
    got: Option<Result<GivenPrompt, String>>,
}

/// Where an agent's given file stands.
#[derive(Debug, PartialEq)]
pub enum Given<'a> {
    NotAsked,
    Waiting,
    Got(Result<&'a GivenPrompt, &'a str>),
}

impl Standing {
    /// Project ID's standing texts, none until its coordd gave them.
    pub fn of(&self, id: &str) -> &[StandingPart] {
        self.held.get(id).map_or(&[], Vec::as_slice)
    }

    /// Whether project ID's standing texts are asked and not yet answered.
    pub fn waiting(&self, id: &str) -> bool {
        self.asked.contains(id)
    }

    /// Agent NAME's given file in project ID: its last answer, else whether it is on its way.
    pub fn given(&self, id: &str, name: &str) -> Given<'_> {
        match self.given.get(&(id.to_string(), name.to_string())) {
            Some(Held { got: Some(r), .. }) => Given::Got(r.as_ref().map_err(String::as_str)),
            Some(Held { waiting: true, .. }) => Given::Waiting,
            _ => Given::NotAsked,
        }
    }
}

/// Asks, for each System prompt on screen ([`crate::sidebar::prompt_targets`]), its project's coordd for the standing
/// texts once and its agent's given file once per start; each is marked asked only once sent. Whether one was.
pub fn poll(s: &mut App) -> bool {
    let mut asked = false;
    for (pi, name) in crate::sidebar::prompt_targets(s) {
        let Some(p) = s.projects.get(pi).filter(|p| p.link.is_some() && p.down.is_none() && p.protocol >= coord_types::STANDING_PROTOCOL) else { continue };
        let (id, since) = (p.id.clone(), p.live.agents.iter().find(|a| a.name == name).map(|a| a.since.clone()));
        if let Some(since) = since.filter(|_| p.protocol >= coord_types::GIVEN_PROTOCOL) {
            let key = (id.clone(), name.clone());
            let done = s.standing.given.get(&key).is_some_and(|g| g.since == since && (g.waiting || g.got.is_some()));
            if !done && s.request_in(pi, coord_types::Request::Given { name: name.clone() }, crate::Pending::Given { project: id.clone(), name }) {
                let g = s.standing.given.entry(key).or_default();
                (g.since, g.waiting) = (since, true);
                asked = true;
            }
        }
        if !s.standing.held.contains_key(&id) && !s.standing.asked.contains(&id) && s.request_in(pi, coord_types::Request::Standing, crate::Pending::Standing { project: id.clone() }) {
            s.standing.asked.insert(id);
            asked = true;
        }
    }
    asked
}

/// An ask the pipe's end lost (`Incoming::Down`): asked again once it is back.
pub fn lost(st: &mut Standing, w: &crate::Pending) {
    match w {
        crate::Pending::Standing { project } => {
            st.asked.remove(project);
        }
        crate::Pending::Given { project, name } => {
            if let Some(g) = st.given.get_mut(&(project.clone(), name.clone())) {
                (g.since, g.waiting) = (String::new(), false);
            }
        }
        _ => {}
    }
}

/// Agent NAME's given file in project ID, coordd's answer R; a refusal (no file yet) kept as such.
pub fn came_given(s: &mut App, id: String, name: String, r: Result<String, String>) {
    let g = s.standing.given.entry((id, name)).or_default();
    g.waiting = false;
    g.got = Some(r.and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())));
}

/// Project ID's coordd's answer R kept; a refusal keeps none, so the standing text is marked as one.
pub fn came(s: &mut App, id: String, r: Result<String, String>) {
    s.standing.asked.remove(&id);
    s.standing.held.insert(id, r.ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(name: &str, text: &str, ours: bool) -> Part {
        Part { name: name.into(), text: text.into(), ours }
    }

    fn standing(from: &str, text: &str) -> StandingPart {
        StandingPart { from: from.into(), path: format!("/{from}/standing.md"), text: text.into() }
    }

    /// Claude Code's parts are the harness's, one he changed his; the appended text is cut at the global and the
    /// project's standing texts, his words after them his; with no files said, it is the standing text whole.
    #[test]
    fn each_section_says_where_it_came_from() {
        let parts = [part("Agent line", "You are an agent.", false), part("Environment", "# Environment\n - Linux", false), part("ours", "For every agent.\n\nOnly here.\n\nAnd mine.", true)];
        let files = [standing("global", "For every agent.\n"), standing("project", "Only here.\n")];
        let got = sections(Some("You are Claude Code."), &parts, &["Environment".into()], true, &files);
        let said: Vec<(Source, &str)> = got.iter().map(|s| (s.source, s.text.as_str())).collect();
        assert_eq!(said, [(Source::Harness, "You are Claude Code."), (Source::Harness, "You are an agent."), (Source::Override, "# Environment\n - Linux"), (Source::Global, "For every agent."), (Source::Project, "Only here."), (Source::Override, "And mine.")]);
        let bare = sections(None, &parts, &[], false, &[]);
        assert_eq!(bare.last().map(|s| (s.source, s.text.as_str())), Some((Source::Standing, "For every agent.\n\nOnly here.\n\nAnd mine.")));
        assert_eq!(sections(None, &parts[..1], &[], false, &files).len(), 1, "nothing appended: no standing section");
    }
}
