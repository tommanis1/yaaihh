//! The groups one `@` tag reaches (0.2 B10; the owner, N-1033.p1: "send this message to the team", and paper-research
//! p2: "a recipient `the team` (and `a team`, for example the accounts group) … so your words go to each of them
//! directly"). `@team` is every agent of the project; `@PART` the agents of a part the policy names (`part(agent,
//! name)`, "/" nesting a part under another): its last name with `-` for its spaces (`@Merge-and-release`), a part
//! taking its nested parts' agents (`@yaaihh-project`). Only running agents are reached: a message to many starts
//! nobody. An agent's own name is the agent, never a group.
//!
//! One rule for both sides (consult-3, N-1402.p2: "One rule, not two"): the window resolves a tag in the Say it sends
//! (its names) and draws whom it reaches; coordd resolves one in the owner's post on a thread, whose `@NAME`s it reads
//! itself (`GROUPS_PROTOCOL`).

/// The tag of the whole team.
pub const TEAM: &str = "team";

/// An agent as a group is read from: its name, its part (`a/b`; empty: none) and whether it runs.
#[derive(Clone, Copy, Debug)]
pub struct Member<'a> {
    pub name: &'a str,
    pub part: &'a str,
    pub running: bool,
}

/// A group one tag reaches.
#[derive(Debug, PartialEq)]
pub struct Group<'a> {
    /// The tag as it completes: `team`, or the part's last name with `-` for its spaces.
    pub tag: String,
    /// As he reads it: "the team", or the part's last name.
    pub name: String,
    /// Its running agents, in the order given.
    pub members: Vec<&'a str>,
    /// Its stopped agents, not reached.
    pub stopped: Vec<&'a str>,
}

/// The `@word`s of TEXT, in order: an `@` at the start or after white space, then letters, digits, `_` and `-`.
pub fn tags_in(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices('@') {
        if i > 0 && !text[..i].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = &text[i + 1..];
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-')).unwrap_or(rest.len());
        if end > 0 {
            out.push(&rest[..end]);
        }
    }
    out
}

/// The tag of part PATH: its last name, a space as `-`.
pub fn part_tag(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim().replace(' ', "-")
}

/// The paths of the parts: the placed ones by place (PLACES, `part-place`), then any an agent is in; each part a
/// nested one is under among them too ("yaaihh project/Accounts" makes "yaaihh project" one).
pub fn part_paths(places: &[(String, i64)], agents: &[Member]) -> Vec<String> {
    let mut placed: Vec<&(String, i64)> = places.iter().collect();
    placed.sort_by_key(|(_, n)| *n);
    let mut out: Vec<String> = Vec::new();
    for path in placed.into_iter().map(|(n, _)| n.as_str()).chain(agents.iter().map(|a| a.part)) {
        for end in path.match_indices('/').map(|(i, _)| i).chain(std::iter::once(path.len())) {
            let up = path[..end].trim();
            if !up.is_empty() && !out.iter().any(|x| x == up) {
                out.push(up.to_string());
            }
        }
    }
    out
}

/// The group the tag WORD names (letter case aside), if any; none when an agent has that name.
pub fn group<'a>(word: &str, places: &[(String, i64)], agents: &[Member<'a>]) -> Option<Group<'a>> {
    if agents.iter().any(|a| a.name.eq_ignore_ascii_case(word)) {
        return None;
    }
    if word.eq_ignore_ascii_case(TEAM) {
        return Some(of(TEAM.into(), "the team".into(), agents, |_| true));
    }
    let path = part_paths(places, agents).into_iter().find(|path| part_tag(path).eq_ignore_ascii_case(word))?;
    let name = path.rsplit('/').next().unwrap_or(&path).trim().to_string();
    let inside = format!("{path}/");
    Some(of(part_tag(&path), name, agents, |a| a.part == path || a.part.starts_with(&inside)))
}

fn of<'a>(tag: String, name: String, agents: &[Member<'a>], inside: impl Fn(&Member) -> bool) -> Group<'a> {
    let (mut members, mut stopped) = (Vec::new(), Vec::new());
    for a in agents.iter().filter(|a| inside(a)) {
        if a.running { members.push(a.name) } else { stopped.push(a.name) }
    }
    Group { tag, name, members, stopped }
}

/// Every group: the team, then each part in order. A part whose tag is an agent's name, or an earlier group's, is
/// none of its own: the agent, or that group, is what the tag reaches.
pub fn groups<'a>(places: &[(String, i64)], agents: &[Member<'a>]) -> Vec<Group<'a>> {
    let mut out: Vec<Group> = group(TEAM, places, agents).into_iter().collect();
    for path in part_paths(places, agents) {
        let tag = part_tag(&path);
        if !out.iter().any(|g| g.tag.eq_ignore_ascii_case(&tag)) {
            out.extend(group(&tag, places, agents));
        }
    }
    out
}

/// The groups TEXT tags, each once, in order.
pub fn tagged<'a>(text: &str, places: &[(String, i64)], agents: &[Member<'a>]) -> Vec<Group<'a>> {
    let mut out: Vec<Group> = Vec::new();
    for word in tags_in(text) {
        if let Some(g) = group(word, places, agents).filter(|g| !out.iter().any(|o| o.tag == g.tag)) {
            out.push(g);
        }
    }
    out
}

/// The running agents the groups TEXT tags reach, in order, each once.
pub fn reached<'a>(text: &str, places: &[(String, i64)], agents: &[Member<'a>]) -> Vec<&'a str> {
    let mut out: Vec<&str> = Vec::new();
    for n in tagged(text, places, agents).into_iter().flat_map(|g| g.members) {
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A team as the window's test has it: Accounts (two running, one stopped) and Threads under "yaaihh project",
    /// Merge and release, Consultants, a placed part nobody is in, and the lead in no part.
    pub(crate) fn team() -> (Vec<(String, i64)>, Vec<Member<'static>>) {
        let m = |name, part, running| Member { name, part, running };
        let places = vec![("yaaihh project/Accounts".into(), 0), ("yaaihh project/Threads".into(), 1), ("Consultants".into(), 2), ("Empty part".into(), 3)];
        let agents = vec![
            m("accounts-2", "yaaihh project/Accounts", true),
            m("accounts-3", "yaaihh project/Accounts", false),
            m("accounts-lead", "yaaihh project/Accounts", true),
            m("consult-3", "Consultants", true),
            m("docket-edge2", "yaaihh project/Threads", true),
            m("merger", "yaaihh project/Merge and release", true),
            m("paper-research", "", true),
        ];
        (places, agents)
    }

    #[test]
    fn a_tag_resolves_to_a_group_by_one_rule() {
        let (places, agents) = team();
        let tags: Vec<String> = groups(&places, &agents).into_iter().map(|g| g.tag).collect();
        assert_eq!(tags, ["team", "yaaihh-project", "Accounts", "Threads", "Consultants", "Empty-part", "Merge-and-release"]);
        let team = group("TEAM", &places, &agents).unwrap();
        assert_eq!((team.members.len(), team.stopped.as_slice()), (6, ["accounts-3"].as_slice()));
        assert_eq!(group("merge-and-release", &places, &agents).map(|g| g.members), Some(vec!["merger"]));
        assert_eq!(group("yaaihh-project", &places, &agents).map(|g| g.members), Some(vec!["accounts-2", "accounts-lead", "docket-edge2", "merger"]));
        assert_eq!(group("Empty-part", &places, &agents).map(|g| (g.name, g.members.len())), Some(("Empty part".to_string(), 0)));
        assert_eq!(group("nobody", &places, &agents), None);
        assert_eq!(tags_in("(@team) a@b @Accounts, and @merger."), ["Accounts", "merger"]);
        assert_eq!(reached("@Accounts and @accounts again, @team", &places, &agents), ["accounts-2", "accounts-lead", "consult-3", "docket-edge2", "merger", "paper-research"]);
        // an agent's own name is the agent
        let mut named = agents.clone();
        named.push(Member { name: "threads", part: "", running: true });
        assert_eq!(group("Threads", &places, &named), None);
        assert!(!groups(&places, &named).iter().any(|g| g.tag == "Threads"));
    }
}
