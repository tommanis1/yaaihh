//! The code view: what the window asks coordd to open in its openvscode-server, and the answer.

use serde::{Deserialize, Serialize};

/// What to open: a file at a line, a commit's diff, a repository, or only the workbench of every repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "is", rename_all = "snake_case")]
pub enum CodeTarget {
    /// PATH absolute or relative to a project repository; LINE 1-based.
    File {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
    },
    /// REPO a project repository's name or its folder.
    Commit { repo: String, sha: String },
    /// PATH a project repository's name or its folder.
    Repo { path: String },
    /// Nothing opened: the workbench made ready, in the theme and colours asked ([`CODE_WARM_PROTOCOL`]).
    ///
    /// [`CODE_WARM_PROTOCOL`]: crate::CODE_WARM_PROTOCOL
    Workspace,
}

/// One look of the window, each colour `#rrggbbaa` as its kit has it, for the workbench to take as its own.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLook {
    /// The page's ground: the editor's.
    pub bg: String,
    /// The sidebar's and the panels' ground.
    pub layer: String,
    /// A hovered or inactive row's ground.
    pub raised: String,
    pub text: String,
    pub muted: String,
    pub faint: String,
    pub border: String,
    pub accent: String,
    pub selection: String,
}

/// Both of the window's looks, so the workbench keeps them when he switches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLooks {
    pub dark: CodeLook,
    pub light: CodeLook,
}

/// coordd's answer: the workbench URL of FOLDER, the project's one workspace of every repository; ORIGIN is the only
/// origin the pane may load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeViewAnswer {
    pub url: String,
    pub origin: String,
    pub folder: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_and_an_answer_read_back() {
        let t = CodeTarget::File { path: "yaaihh/crates/coordd/src/core.rs".into(), line: Some(142) };
        let v = serde_json::to_value(&t).unwrap();
        assert_eq!(v, serde_json::json!({"is": "file", "path": "yaaihh/crates/coordd/src/core.rs", "line": 142}));
        assert_eq!(serde_json::from_value::<CodeTarget>(v).unwrap(), t);
        let c: CodeTarget = serde_json::from_str(r#"{"is":"commit","repo":"yaaihh","sha":"e449d2afe"}"#).unwrap();
        assert_eq!(c, CodeTarget::Commit { repo: "yaaihh".into(), sha: "e449d2afe".into() });
        assert_eq!(serde_json::to_value(CodeTarget::Workspace).unwrap(), serde_json::json!({"is": "workspace"}));
        let a = CodeViewAnswer { url: "http://127.0.0.1:1/?folder=/r&tkn=t".into(), origin: "http://127.0.0.1:1".into(), folder: "/r".into() };
        assert_eq!(serde_json::from_str::<CodeViewAnswer>(&serde_json::to_string(&a).unwrap()).unwrap(), a);
    }
}
