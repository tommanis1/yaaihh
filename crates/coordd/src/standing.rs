//! The standing texts roster appends after Claude Code's own system prompt, each with the file it is
//! (`Request::Standing`): what System prompt marks each section of the appended text by; and the file an agent was
//! given (`Request::Given`), what System prompt shows before its first request.

use coord_types::{GivenPrompt, StandingPart};
use std::path::Path;

/// The standing texts, in roster's order (agent-base/bin/roster `standing`): the global one at the root of the main
/// checkout of the repository the project file DIR is in, when it has one; then the project's beside it, else BASE's.
pub fn parts(dir: Option<&str>, base: &str) -> Vec<StandingPart> {
    let read = |from: &str, path: &Path| std::fs::read_to_string(path).ok().map(|text| StandingPart { from: from.into(), path: path.display().to_string(), text });
    let mut out = Vec::new();
    if let Some(top) = dir.and_then(main_checkout) {
        out.extend(read("global", &top.join("standing.md")));
    }
    let own = dir.map(|d| Path::new(d).join("standing.md")).filter(|p| p.is_file());
    out.extend(match own {
        Some(p) => read("project", &p),
        None => read("agent-base", &Path::new(base).join("claude/standing.md")),
    });
    out
}

/// The system prompt file roster gave agent NAME of the coord COORD at its last start (roster `system_prompt`): its own
/// folder's or, boxed, its box's, the newer; his and in place of the harness's own as `prompts/NAME.system/current` says.
pub fn given(coord: &str, name: &str) -> Result<GivenPrompt, String> {
    if name.is_empty() || name.starts_with('.') || name.contains('/') {
        return Err(format!("{name:?} is no agent's name"));
    }
    let c = Path::new(coord);
    let file = [c.join("agents").join(name), c.join("box").join(name)]
        .into_iter()
        .map(|d| d.join("system-prompt.md"))
        .filter_map(|p| Some((std::fs::metadata(&p).and_then(|m| m.modified()).ok()?, p)))
        .max_by_key(|x| x.0)
        .map(|x| x.1)
        .ok_or_else(|| format!("{name} {}: roster writes it at its start", coord_types::NO_GIVEN_FILE))?;
    let text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let versions = c.join("prompts").join(format!("{name}.system"));
    let current = std::fs::read_to_string(versions.join("current")).unwrap_or_default();
    let mut w = current.split_whitespace();
    let (n, how) = (w.next(), w.next());
    let his = n.is_some_and(|n| versions.join(format!("{n}.md")).is_file());
    // how a prompt recipe made it, where roster wrote that beside it; unread, the prompt is given without it
    let diff = std::fs::read_to_string(file.with_extension("md.diff.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
    Ok(GivenPrompt { path: file.display().to_string(), text, replace: his && how == Some("replace"), his, diff })
}

/// The main checkout of the repository DIR is in: the first worktree git lists.
fn main_checkout(dir: &str) -> Option<std::path::PathBuf> {
    let out = std::process::Command::new("git").args(["-C", dir, "worktree", "list", "--porcelain"]).output().ok().filter(|o| o.status.success())?;
    String::from_utf8(out.stdout).ok()?.lines().next()?.strip_prefix("worktree ").map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::{given, parts};

    /// roster's own `system_prompt` writes the file; `given` reads back what it wrote: the standing text, appended, before
    /// a version of his; his, in place, after `current` names one, with a recipe's diff beside it; none for an agent
    /// never started or a bad name.
    #[test]
    fn given_is_the_file_rosters_system_prompt_wrote() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let roster = std::fs::read_to_string(format!("{base}/bin/roster")).unwrap();
        let def = |f: &str| roster.lines().skip_while(|l| !l.starts_with(&format!("{f}() "))).scan(false, |done, l| (!*done).then(|| { *done = l == "}"; format!("{l}\n") })).collect::<String>();
        let defs = format!("{}{}", def("standing"), def("system_prompt"));
        let coord = std::env::temp_dir().join(format!("coordd-given-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        std::fs::create_dir_all(&coord).unwrap();
        let start = || assert!(std::process::Command::new("bash").arg("-c").arg(format!("{defs}system_prompt pa-temp")).env("COORD", &coord).env("AGENT_BASE", &base).status().unwrap().success());
        let c = coord.display().to_string();
        assert!(given(&c, "pa-temp").is_err_and(|e| e.contains(coord_types::NO_GIVEN_FILE)), "not started yet");
        start();
        let g = given(&c, "pa-temp").unwrap();
        let standing = std::fs::read_to_string(format!("{base}/claude/standing.md")).unwrap();
        assert_eq!((g.text.trim(), g.replace, g.his), (standing.trim(), false, false));
        assert!(g.path.ends_with("agents/pa-temp/system-prompt.md"), "{}", g.path);
        let versions = coord.join("prompts/pa-temp.system");
        std::fs::create_dir_all(&versions).unwrap();
        std::fs::write(versions.join("2.md"), "His own words.").unwrap();
        std::fs::write(versions.join("current"), "2 replace\n").unwrap();
        start();
        assert_eq!(given(&c, "pa-temp").map(|g| (g.text, g.replace, g.his)), Ok(("His own words.".into(), true, true)));
        // a prompt recipe's account of how it was made, beside it: given with it
        assert!(given(&c, "pa-temp").unwrap().diff.is_none());
        let seg = r##"{"kind":"stripped","label":"#4 Pronouns","text":"When you use a pronoun","tokens":6,"source":"base/claude-code.md"}"##;
        std::fs::write(coord.join("agents/pa-temp/system-prompt.md.diff.json"), format!(r#"{{"harness":"claude-code","how":"replace","notes":[],"segments":[{seg}]}}"#)).unwrap();
        let d = given(&c, "pa-temp").unwrap().diff.expect("its diff");
        assert_eq!((d.how.as_str(), d.segments[0].kind.as_str(), d.segments[0].label.as_str(), d.segments[0].tokens), ("replace", "stripped", "#4 Pronouns", 6));
        assert!(given(&c, "../pa-temp").is_err() && given(&c, "").is_err());
        let _ = std::fs::remove_dir_all(&coord);
    }

    /// Joined as roster joins them (each file, the global one with a line after it), the parts are what roster's
    /// `standing` prints, for a project with its own text, with none, and with no global one; each says its file.
    #[test]
    fn the_parts_joined_are_rosters_standing_text_and_each_says_its_file() {
        let base = format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"));
        let roster = std::fs::read_to_string(format!("{base}/bin/roster")).unwrap();
        let def: String = roster.lines().skip_while(|l| !l.starts_with("standing() ")).scan(false, |done, l| (!*done).then(|| { *done = l == "}"; format!("{l}\n") })).collect();
        let root = std::env::temp_dir().join(format!("coordd-standing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (coord, repo) = (root.join("coord"), root.join("flavor"));
        let project = repo.join("projects").join("p");
        std::fs::create_dir_all(&coord).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        assert!(std::process::Command::new("git").args(["init", "-q"]).arg(&repo).status().unwrap().success());
        std::fs::write(project.join("project.toml"), "[project]\n").unwrap();
        std::fs::write(coord.join("project"), format!("{}\n", project.join("project.toml").display())).unwrap();
        let roster_says = || String::from_utf8(std::process::Command::new("bash").arg("-c").arg(format!("{def}standing")).env("COORD", &coord).env("AGENT_BASE", &base).output().unwrap().stdout).unwrap();
        let dir = project.display().to_string();
        let joined = |ps: &[coord_types::StandingPart]| ps.iter().map(|p| if p.from == "global" { format!("{}\n", p.text) } else { p.text.clone() }).collect::<String>();
        let froms = |ps: &[coord_types::StandingPart]| ps.iter().map(|p| p.from.clone()).collect::<Vec<_>>();
        let ps = parts(Some(&dir), &base);
        assert_eq!((froms(&ps), joined(&ps)), (vec!["agent-base".to_string()], roster_says()), "no text of the project's, none global");
        std::fs::write(project.join("standing.md"), "Only what works here.\n").unwrap();
        std::fs::write(repo.join("standing.md"), "The flavor's.\n").unwrap();
        let ps = parts(Some(&dir), &base);
        assert_eq!((froms(&ps), joined(&ps)), (vec!["global".to_string(), "project".to_string()], roster_says()));
        assert!(ps[0].path.ends_with("flavor/standing.md") && ps[1].path.ends_with("projects/p/standing.md"), "{ps:?}");
        assert_eq!(froms(&parts(None, &base)), ["agent-base"], "no project file");
        let _ = std::fs::remove_dir_all(&root);
    }
}
