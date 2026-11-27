//! The models an agent may run, with what people guess of those whose maker is not named (walk item 29;
//! `Request::ModelList`), read from the one `model-guesses.toml` at the flavor's root; and the harnesses' table of
//! what the owner's switches can do (`Request::HarnessCapabilities`), held to what agent-base's bin/harness-settings
//! gives a start, so the window never shows a switch as taken that a start drops.

use crate::core::Core;
use coord_types::harness::{HarnessCapabilities, ModelEntry, HARNESSES};

fn base() -> String {
    format!("{}/../../agent-base", env!("CARGO_MANIFEST_DIR"))
}

struct Flavor {
    dir: std::path::PathBuf,
    core: Core,
}

impl Drop for Flavor {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A coord whose project is FLAVOR/projects/p, serving MODELS; GUESSES, the flavor's model-guesses.toml, if any.
fn flavor(tag: &str, models: &[&str], guesses: Option<&str>) -> Flavor {
    let dir = std::env::temp_dir().join(format!("docket-guesses-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let project = dir.join("flavor/projects/p");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(dir.join("coord")).unwrap();
    std::fs::write(project.join("project.toml"), "[project]\nname = \"p\"\n").unwrap();
    std::fs::write(dir.join("coord/project"), project.join("project.toml").display().to_string()).unwrap();
    if let Some(g) = guesses {
        std::fs::write(dir.join("flavor/model-guesses.toml"), g).unwrap();
    }
    let coord = dir.join("coord").display().to_string();
    let mut core = Core::load(&coord, &base());
    core.project = crate::project::Project::load(&coord).unwrap();
    core.set_models(models.iter().map(|m| (m.to_string(), "opencode".to_string())).collect());
    Flavor { dir, core }
}

const GUESSES: &str = r#"# What people guess a model is, where its maker is not named.

["opencode-go/beam-free"]
guess = "Beam 1.0"
read = "a page about it"

["opencode-go/not-served-free"]
guess = "nothing served"
read = "nowhere"
"#;

#[test]
fn each_model_carries_the_flavors_guess_and_where_it_was_read_and_only_where_there_is_one() {
    let f = flavor("read", &["claude-haiku-5-5", "opencode-go/beam-free"], Some(GUESSES));
    let list = f.core.model_list();
    assert_eq!(
        list,
        vec![
            ModelEntry { id: "claude-haiku-5-5".into(), guess: None, read: None },
            ModelEntry { id: "opencode-go/beam-free".into(), guess: Some("Beam 1.0".into()), read: Some("a page about it".into()) },
        ],
        "the runnable models in their order, a guess only on the one the file names; a model it names that is not served is not added"
    );
    assert_eq!(list.iter().map(|m| m.id.clone()).collect::<Vec<_>>(), f.core.runnable_models(), "the same models as Request::Models");
    // read at each ask, never copied: an edit at the root shows at the next
    std::fs::write(f.dir.join("flavor/model-guesses.toml"), GUESSES.replace("Beam 1.0", "Beam 1.1")).unwrap();
    assert_eq!(f.core.model_list()[1].guess.as_deref(), Some("Beam 1.1"));
    assert!(!f.dir.join("flavor/projects/p/model-guesses.toml").exists() && !f.dir.join("coord/model-guesses.toml").exists(), "no copy made");
}

#[test]
fn no_file_a_link_a_file_too_big_or_not_toml_gives_the_models_with_no_guesses() {
    let models = ["opencode-go/beam-free"];
    let bare = vec![ModelEntry { id: "opencode-go/beam-free".into(), guess: None, read: None }];
    assert_eq!(flavor("none", &models, None).core.model_list(), bare);
    assert_eq!(flavor("bad", &models, Some("[\"opencode-go/beam-free\"\nguess = ")).core.model_list(), bare);
    let big = format!("{GUESSES}\n# {}\n", "x".repeat(crate::core::MODEL_GUESSES_CAP as usize));
    assert_eq!(flavor("big", &models, Some(&big)).core.model_list(), bare);
    let f = flavor("link", &models, None);
    let elsewhere = f.dir.join("elsewhere.toml");
    std::fs::write(&elsewhere, GUESSES).unwrap();
    std::os::unix::fs::symlink(&elsewhere, f.dir.join("flavor/model-guesses.toml")).unwrap();
    assert_eq!(f.core.model_list(), bare, "a link is not followed");
}

#[test]
fn a_project_outside_a_projects_folder_has_no_flavor_and_no_guesses() {
    let f = flavor("loose", &["opencode-go/beam-free"], Some(GUESSES));
    let loose = f.dir.join("loose");
    std::fs::create_dir_all(&loose).unwrap();
    std::fs::write(loose.join("project.toml"), "[project]\nname = \"l\"\n").unwrap();
    std::fs::write(f.dir.join("model-guesses.toml"), GUESSES).unwrap();
    std::fs::write(f.dir.join("coord/project"), loose.join("project.toml").display().to_string()).unwrap();
    let mut core = Core::load(&f.dir.join("coord").display().to_string(), &base());
    core.project = crate::project::Project::load(&f.dir.join("coord").display().to_string()).unwrap();
    core.set_models(vec![("opencode-go/beam-free".into(), "opencode".into())]);
    assert_eq!(core.model_list()[0].guess, None, "the folder above the project is no flavor's root");
}

/// For each harness, in a box and outside one: a switch the table says it cannot take is one harness-settings says
/// it does not take, and one it can take is never in its `not_taken`.
#[test]
fn the_table_and_harness_settings_agree_on_what_each_harness_takes() {
    let dir = std::env::temp_dir().join(format!("docket-harness-table-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("agents/a")).unwrap();
    for h in HARNESSES {
        std::fs::write(dir.join("agents/a/harness.toml"), format!("[{h}]\nsafety_check = false\ntools_off = [\"web_search\", \"SomeTool\"]\nparts_off = [\"A part.\"]\n")).unwrap();
        for boxed in [false, true] {
            let t = HarnessCapabilities::of(h, boxed).unwrap();
            if h == "opencode" && boxed {
                continue; // not started in a box at all: nothing reaches harness-settings
            }
            let mut cmd = std::process::Command::new(format!("{}/bin/harness-settings", base()));
            cmd.args([h, "a"]).env("COORD", &dir).env_remove("ROSTER_BOX").env_remove("COORD_IN_BOX");
            if boxed {
                cmd.env("ROSTER_BOX", "1");
            }
            let out = cmd.output().unwrap();
            assert!(out.status.success(), "{h}: {}", String::from_utf8_lossy(&out.stderr));
            let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            let not_taken: Vec<&str> = got["not_taken"].as_array().unwrap().iter().filter_map(|v| v.as_str()).collect();
            let said = |what: &str| not_taken.iter().any(|n| n.starts_with(what));
            assert_eq!(said("part "), !t.parts_off.taken(), "{h} boxed={boxed}: parts_off");
            assert_eq!(said("safety_check"), !t.safety_check_off.taken(), "{h} boxed={boxed}: safety_check");
            // tools: a harness that takes some takes web_search; one that takes none says so for each
            assert_eq!(not_taken.iter().any(|n| n.starts_with("tool web_search")), !t.tools_off.taken(), "{h} boxed={boxed}: tools_off");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
