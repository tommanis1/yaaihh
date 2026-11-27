//! The install coordd runs the server from: links to the pinned one in a folder of its own, whose `extensions` holds
//! the built-ins less the heavy ones. The server takes its built-ins only from beside its own code, whatever it is
//! told, so node runs its code through the links (`--preserve-symlinks-main`), and that folder is the one it reads.

use std::path::{Path, PathBuf};

/// Built-ins left out: language servers and features of no use in a code view inside yaaihh (TypeScript's and JSON's
/// servers alone were most of the extension host's start), sign-ins, debuggers, notebooks, task runners and themes
/// beside the default ones. Each language's grammar stays, so every file is highlighted.
pub const LEFT_OUT: [&str; 38] = [
    "typescript-language-features",
    "json-language-features",
    "css-language-features",
    "html-language-features",
    "php-language-features",
    "configuration-editing",
    "extension-editing",
    "emmet",
    "npm",
    "grunt",
    "gulp",
    "jake",
    "github",
    "github-authentication",
    "microsoft-authentication",
    "tunnel-forwarding",
    "debug-auto-launch",
    "debug-server-ready",
    "ms-vscode.js-debug",
    "ms-vscode.js-debug-companion",
    "ms-vscode.vscode-js-profile-table",
    "ipynb",
    "notebook-renderers",
    "mermaid-chat-features",
    "prompt-basics",
    "simple-browser",
    "references-view",
    "terminal-suggest",
    "theme-2026",
    "theme-abyss",
    "theme-kimbie-dark",
    "theme-monokai",
    "theme-monokai-dimmed",
    "theme-quietlight",
    "theme-red",
    "theme-solarized-dark",
    "theme-solarized-light",
    "theme-tomorrow-night-blue",
];

/// The server's own switches: no trust prompt, no telemetry, no experiments fetched.
pub const FLAGS: [&str; 3] = ["--disable-workspace-trust", "--disable-telemetry", "--disable-experiments"];

/// The install BIN (`…/bin/openvscode-server`) belongs to: its folder with `node` and `out/server-main.js`.
pub fn root(bin: &Path) -> Result<PathBuf, String> {
    let r = std::fs::canonicalize(bin).ok().and_then(|b| Some(b.parent()?.parent()?.to_path_buf()));
    r.filter(|r| r.join("node").is_file() && r.join("out/server-main.js").is_file()).ok_or_else(|| format!("coord codeview: {} is no openvscode-server install", bin.display()))
}

/// How a server of ROOT is started, said in one line: a server started otherwise is stopped and started again.
pub fn how(root: &Path) -> String {
    format!("{} {} less {}", root.display(), FLAGS.join(" "), LEFT_OUT.join(","))
}

/// APP made afresh: a link to each part of ROOT, and `extensions` of links to its built-ins but those left out.
pub fn link(root: &Path, app: &Path) -> Result<(), String> {
    let at = |e: std::io::Error, p: &Path| format!("{}: {e}", p.display());
    if app.exists() {
        std::fs::remove_dir_all(app).map_err(|e| at(e, app))?;
    }
    let exts = app.join("extensions");
    std::fs::create_dir_all(&exts).map_err(|e| at(e, &exts))?;
    let each = |d: &Path| -> Result<Vec<std::ffi::OsString>, String> { Ok(std::fs::read_dir(d).map_err(|e| at(e, d))?.filter_map(|e| e.ok()).map(|e| e.file_name()).collect()) };
    for name in each(root)?.into_iter().filter(|n| n != "extensions") {
        std::os::unix::fs::symlink(root.join(&name), app.join(&name)).map_err(|e| at(e, &app.join(&name)))?;
    }
    for name in each(&root.join("extensions"))?.into_iter().filter(|n| !LEFT_OUT.iter().any(|l| n == l)) {
        std::os::unix::fs::symlink(root.join("extensions").join(&name), exts.join(&name)).map_err(|e| at(e, &exts.join(&name)))?;
    }
    Ok(())
}
