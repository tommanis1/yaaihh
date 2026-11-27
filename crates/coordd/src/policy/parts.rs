//! The sidebar's parts (N-412 ask 2, N-427): the owner's `part(agent, name)` and `part-place(name, n)` instances in the
//! policy, read as written, without a solve. `+part("docket-core", "yaaihh project/Engine").` puts an agent in a part
//! ("/" nests it under another); `+part-place("yaaihh project", 1).` gives a part its place, so an empty one is shown
//! too. coordd only passes them on: each agent's view carries its `part`, the snapshot the placed `parts`.
//! `+check-in-every("paper-research", 20).` is read here too, the same way: how many turns of an agent its own
//! `every e.every of turn_end(e)` binding counts (the brain, N-1444 p16), carried in its `turn_end` as `every`.

use super::syntax::{DeclKind, Expr};
use super::Places;
use std::collections::BTreeMap;

/// The parts the policy states.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parts {
    /// Each agent's part: its first `+part` line, in the policy's file order.
    pub of: BTreeMap<String, String>,
    /// The placed parts, by place, then name; a part's first `+part-place` line counts.
    pub places: Vec<(String, i64)>,
    /// Each agent's `check-in-every`: its first `+check-in-every` line.
    pub check_in: BTreeMap<String, i64>,
}

/// The parts of the policy files of PLACES. A line of another shape (`+part("a", 3).`) is not a part and is left out;
/// a policy that cannot be read gives none (the norms' load says why).
pub fn load(places: &Places) -> Result<Parts, String> {
    let mut p = Parts::default();
    let mut placed: BTreeMap<String, i64> = BTreeMap::new();
    for (_, decls) in places.sources()? {
        for d in decls {
            let DeclKind::Instance { fact, args } = &d.kind else { continue };
            match (fact.as_str(), args.as_slice()) {
                ("part", [Expr::Str(agent), Expr::Str(name)]) => {
                    p.of.entry(agent.clone()).or_insert_with(|| name.clone());
                }
                ("part-place", [Expr::Str(name), Expr::Int(n)]) => {
                    placed.entry(name.clone()).or_insert(*n);
                }
                ("check-in-every", [Expr::Str(agent), Expr::Int(n)]) => {
                    p.check_in.entry(agent.clone()).or_insert(*n);
                }
                _ => {}
            }
        }
    }
    p.places = placed.into_iter().collect();
    p.places.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder, removed when dropped.
    struct Dir(std::path::PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn places_with(tag: &str, text: &str) -> (Dir, Places) {
        let d = Dir(std::env::temp_dir().join(format!("coordd-parts-{tag}-{}", std::process::id())));
        let (base, project) = (d.0.join("base"), d.0.join("project"));
        std::fs::create_dir_all(base.join("policy")).unwrap();
        std::fs::create_dir_all(project.join("policy")).unwrap();
        std::fs::write(project.join("policy/parts.eflint"), text).unwrap();
        let p = Places { base: base.display().to_string(), project: Some(project.display().to_string()), coord: d.0.display().to_string(), ..Default::default() };
        (d, p)
    }

    #[test]
    fn the_parts_are_the_policys_instances_as_written() {
        let (_d, places) = places_with(
            "all",
            "Fact agent Identified by String\n\
             Fact part Identified by agent * String\n\
             Fact part-place Identified by String * Int\n\
             +part(\"docket-core\", \"yaaihh project/Engine\").\n\
             +part(\"a2h-speed\", \"Fast tools\").\n\
             +part(\"a2h-speed\", \"Papers\").\n\
             +part(\"odd\", 3).\n\
             +part-place(\"Tovrin team\", 4).\n\
             +part-place(\"yaaihh project\", 1).\n\
             +part-place(\"Fast tools\", 2).\n\
             Fact check-in-every Identified by agent * Int\n\
             +check-in-every(\"paper-research\", 20).\n\
             +check-in-every(\"paper-research\", 5).\n",
        );
        let p = load(&places).unwrap();
        assert_eq!(p.of.get("docket-core").map(String::as_str), Some("yaaihh project/Engine"));
        assert_eq!(p.of.get("a2h-speed").map(String::as_str), Some("Fast tools"), "the first line counts");
        assert!(!p.of.contains_key("odd"), "not a part's shape");
        assert_eq!(p.places, vec![("yaaihh project".into(), 1), ("Fast tools".into(), 2), ("Tovrin team".into(), 4)]);
        // the brain's N for an agent (N-1444 p16): its first line counts, as a part's does
        assert_eq!(p.check_in, BTreeMap::from([("paper-research".to_string(), 20)]));
    }

    #[test]
    fn a_policy_without_parts_has_none() {
        let (_d, places) = places_with("none", "Fact agent Identified by String\n");
        assert_eq!(load(&places).unwrap(), Parts::default());
    }
}
