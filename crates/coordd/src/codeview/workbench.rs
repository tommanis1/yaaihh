//! The workbench coordd gives the server: our extension, an extension of coordd's own with the settings that keep
//! every prompt away (settings.rs) and his keybindings when he opts in, the machine settings with his editor settings,
//! the one workspace of the project's Files roots, and the queue of opens our extension performs.

use super::Ctx;
use super::target::Open;
use coord_types::codeview::CodeLooks;
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};

const OPEN: &str = "yaaihh.yaaihh-open-0.0.1";
const OWN: &str = "yaaihh.yaaihh-own-0.0.1";
/// The keys of his own settings an opt-in carries over: how the editor behaves, nothing that installs anything.
const INHERITED: [&str; 4] = ["editor.", "diffEditor.", "files.", "keyboard."];

/// The server's folders; our extension linked in; coordd's own beside it; the machine settings in THEME and LOOKS;
/// the workspace of the project's roots, whose file is answered.
pub fn prepare(ctx: &Ctx, theme: Option<&str>, looks: Option<&CodeLooks>) -> Result<PathBuf, String> {
    let dir = ctx.dir();
    let exts = dir.join("extensions");
    for d in [&exts, &dir.join("done")] {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let src = ctx.agent_base.join("codeview/yaaihh-open");
    let link = exts.join(OPEN);
    if std::fs::read_link(&link).ok().as_deref() != Some(src.as_path()) {
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&src, &link).map_err(|e| format!("{}: {e}", link.display()))?;
    }
    let his = inherit_dir(&dir);
    let read = |f: &str| his.as_ref().and_then(|h| std::fs::read_to_string(h.join(f)).ok()).and_then(|t| serde_json::from_str::<Value>(&jsonc(&t)).ok());
    let mut editor = Map::new();
    if let Some(Value::Object(m)) = read("settings.json") {
        editor.extend(m.into_iter().filter(|(k, _)| INHERITED.iter().any(|p| k.starts_with(p))));
    }
    super::settings::write(&settings_file(&dir), editor, theme, looks)?;
    let own = exts.join(OWN);
    std::fs::create_dir_all(&own).map_err(|e| format!("{}: {e}", own.display()))?;
    let keys = match read("keybindings.json") {
        Some(Value::Array(k)) => k,
        _ => Vec::new(),
    };
    let manifest = own_manifest(keys).to_string();
    if std::fs::read_to_string(own.join("package.json")).ok() != Some(manifest.clone()) {
        write(&own.join("package.json"), &manifest)?;
    }
    let listed = vec![listing(&exts, OPEN, "yaaihh.yaaihh-open"), listing(&exts, OWN, "yaaihh.yaaihh-own")];
    write(&exts.join("extensions.json"), &Value::Array(listed).to_string())?;
    workspace(ctx)
}

/// The server's machine settings file under its data folder.
pub fn settings_file(dir: &Path) -> PathBuf {
    dir.join("server/data/Machine/settings.json")
}

/// `codeview/repos.code-workspace`: each root of the project that is a folder here, by its name; written when it
/// changed, so the workbench open on it takes a new one in place.
fn workspace(ctx: &Ctx) -> Result<PathBuf, String> {
    let folders: Vec<Value> = ctx.roots.iter().filter_map(|(n, p)| std::fs::canonicalize(p).ok().filter(|c| c.is_dir()).map(|c| json!({"name": n, "path": c.display().to_string()}))).collect();
    if folders.is_empty() {
        return Err("coord codeview: this project has no folder here".into());
    }
    let file = ctx.dir().join("repos.code-workspace");
    let text = json!({"folders": folders}).to_string();
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        write(&file, &text)?;
    }
    Ok(file)
}

/// One entry of the extensions folder's list, as the server reads it.
fn listing(exts: &Path, folder: &str, id: &str) -> Value {
    json!({"identifier": {"id": id}, "version": "0.0.1", "relativeLocation": folder,
        "location": {"$mid": 1, "scheme": "file", "path": exts.join(folder).display().to_string()}})
}

/// coordd's own extension: the settings that keep every prompt away as its defaults, and KEYS, his keybindings, as
/// its contributed ones; a removal (`-command`) has no such form and is left out.
fn own_manifest(keys: Vec<Value>) -> Value {
    let keys: Vec<Value> = keys.into_iter().filter(|k| k["command"].as_str().is_some_and(|c| !c.starts_with('-')) && k["key"].is_string()).collect();
    json!({"name": "yaaihh-own", "publisher": "yaaihh", "version": "0.0.1", "engines": {"vscode": "^1.90.0"},
        "capabilities": {"untrustedWorkspaces": {"supported": true}},
        "contributes": {"configurationDefaults": super::settings::managed(), "keybindings": keys}})
}

/// The VS Code User folder he opted in with: its path in `codeview/inherit`.
fn inherit_dir(dir: &Path) -> Option<std::path::PathBuf> {
    let p = std::fs::read_to_string(dir.join("inherit")).ok()?;
    Some(std::path::PathBuf::from(p.trim())).filter(|p| p.is_dir())
}

/// JSON with comments and trailing commas, as VS Code writes settings, made plain JSON.
pub fn jsonc(text: &str) -> String {
    let (mut out, mut chars) = (String::with_capacity(text.len()), text.chars().peekable());
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                out.extend(chars.next());
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_str = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                while chars.next_if(|c| *c != '\n').is_some() {}
            }
            ('/', Some('*')) => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '*' && chars.next_if_eq(&'/').is_some() {
                        break;
                    }
                }
            }
            (',', _) => {
                let rest: String = chars.clone().skip_while(|c| c.is_whitespace()).take(1).collect();
                if rest != "}" && rest != "]" {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// OPEN queued for the workbench: one line of `codeview/open.jsonl`, ID unique.
pub fn queue(dir: &Path, open: &Open) -> Result<(), String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let mut line = json!({"id": format!("{}-{}", now.as_nanos(), std::process::id()), "at": now.as_secs(), "folder": open.folder});
    if let (Value::Object(l), Value::Object(a)) = (&mut line, &open.action) {
        l.extend(a.clone());
    }
    let path = dir.join("open.jsonl");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    writeln!(f, "{line}").map_err(|e| format!("{}: {e}", path.display()))
}

/// FOLDER as a URL query value.
pub fn query(folder: &str) -> String {
    folder.bytes().map(|b| if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn write(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| format!("{}: {e}", path.display()))
}
