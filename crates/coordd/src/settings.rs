//! The owner's settings at the yaaihh-wide and project levels (`Request::Settings`), each in the file its reader reads:
//! `yaaihh.toml` beside the coords and the project file, but for Claude Code's safety check, which is `[claude]` of
//! `harness.toml` at the flavor's root and in the project's own folder.

use crate::files::Places;
use crate::project::{Change, Project, edit_project_file, mode_yolo};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The providers a default model is kept for, as `models.PROVIDER` keys.
pub const PROVIDERS: [&str; 5] = ["claude-gateway", "claude-personal", "agy-personal", "t3code", "opencode"];

const LEVELS: [&str; 2] = ["yaaihh", "project"];

/// Each setting by its wire key: its table and key in `yaaihh.toml` and in the project file ("" the top).
fn spot(key: &str) -> Option<(&'static str, String)> {
    match key {
        "agent_mode" => Some(("project", "agent_mode".into())),
        "boxed" => Some(("project", "box".into())),
        "owner_speakers" => Some(("project", "owner_speakers".into())),
        k => k.strip_prefix("models.").filter(|p| PROVIDERS.contains(p)).map(|p| ("models", p.to_string())),
    }
}

/// Every key the answer carries.
fn keys() -> Vec<String> {
    let mut v: Vec<String> = ["agent_mode", "safety_check", "boxed", "owner_speakers"].map(String::from).to_vec();
    v.extend(PROVIDERS.iter().map(|p| format!("models.{p}")));
    v
}

/// The flavor's root of a project whose own folder is DIR: the folder of the `projects` folder DIR is in.
pub fn flavor_root(dir: &str) -> Option<PathBuf> {
    let real = std::fs::canonicalize(dir).ok()?;
    real.parent().filter(|p| p.file_name().is_some_and(|n| n == "projects")).and_then(|p| p.parent()).map(Path::to_path_buf)
}

/// `yaaihh.toml` beside the coords.
pub fn wide_file(coord: &str) -> Option<PathBuf> {
    Some(Path::new(coord).parent()?.join("yaaihh.toml"))
}

/// The file KEY lives in at LEVEL, the table and the key there; None where there is none (no flavor, no project).
fn place(coord: &str, project: Option<&Project>, level: &str, key: &str) -> Option<(PathBuf, &'static str, String)> {
    let dir = project.map(|p| p.dir.as_str()).filter(|d| !d.is_empty());
    match (key, level) {
        ("safety_check", "yaaihh") => Some((flavor_root(dir?)?.join("harness.toml"), "claude", key.into())),
        ("safety_check", "project") => Some((Path::new(dir?).join("harness.toml"), "claude", key.into())),
        (_, "yaaihh") => {
            let (table, k) = spot(key)?;
            Some((wide_file(coord)?, if table == "models" { "models" } else { "" }, k))
        }
        _ => None,
    }
}

/// VALUE as the answer says KEY's: a mode or the speakers a word or names, the check and the box a bool, a model its id.
fn shown(key: &str, v: &toml::Value) -> Value {
    match (key, v) {
        ("agent_mode", toml::Value::String(w)) if mode_yolo(w).is_some() => json!(w),
        ("safety_check" | "boxed", toml::Value::Boolean(b)) => json!(b),
        ("owner_speakers", toml::Value::String(w)) if w == "leads" || w == "any" => json!(w),
        ("owner_speakers", toml::Value::Array(a)) => json!(a.iter().filter_map(|n| n.as_str()).collect::<Vec<_>>().join(",")),
        (k, toml::Value::String(m)) if k.starts_with("models.") && !m.is_empty() => json!(m),
        _ => Value::Null,
    }
}

/// KEY's value at LEVEL as the answer says it; null when unset.
fn value_at(coord: &str, project: Option<&Project>, level: &str, key: &str) -> Value {
    if level == "project" && key != "safety_check" {
        let Some((table, k)) = spot(key) else { return Value::Null };
        let raw = project.map(|p| p.raw.get(table).and_then(|t| t.get(&k)).cloned());
        return raw.flatten().and_then(|v| toml::Value::try_from(v).ok()).map_or(Value::Null, |v| shown(key, &v));
    }
    let Some((file, table, k)) = place(coord, project, level, key) else { return Value::Null };
    let Some(t) = std::fs::read_to_string(&file).ok().and_then(|t| t.parse::<toml::Table>().ok()) else { return Value::Null };
    let t = if table.is_empty() { Some(&t) } else { t.get(table).and_then(toml::Value::as_table) };
    t.and_then(|t| t.get(&k)).map_or(Value::Null, |v| shown(key, v))
}

/// The answer: every key at both levels, and SAID, what a change wrote.
pub fn read(coord: &str, project: Option<&Project>, said: Option<&str>) -> String {
    let level = |l: &str| keys().into_iter().map(|k| (k.clone(), value_at(coord, project, l, &k))).collect::<serde_json::Map<_, _>>();
    json!({"yaaihh": level("yaaihh"), "project": level("project"), "said": said}).to_string()
}

/// Who may speak to the owner first, on a thread he is not on or in a question: the project's, else every project's.
#[derive(Clone, Debug, PartialEq)]
pub enum Speakers {
    Any,
    Leads,
    Named(Vec<String>),
}

/// The speakers in force for PROJECT of COORD: Any when nothing is set.
pub fn speakers(coord: &str, project: Option<&Project>) -> Speakers {
    let v = match value_at(coord, project, "project", "owner_speakers") {
        Value::Null => value_at(coord, project, "yaaihh", "owner_speakers"),
        v => v,
    };
    match v.as_str() {
        Some("leads") => Speakers::Leads,
        Some("any") | None => Speakers::Any,
        Some(names) => Speakers::Named(names.split(',').map(str::to_string).collect()),
    }
}

/// The box default every project without its own `box` takes: `box` in `yaaihh.toml`.
pub fn wide_box(coord: &str) -> Option<bool> {
    let text = std::fs::read_to_string(wide_file(coord)?).ok()?;
    text.parse::<toml::Table>().ok()?.get("box")?.as_bool()
}

/// TEXT, a TOML file, with KEY in TABLE ("" the top) set to VALUE, or removed for none; its comments kept.
fn with_key(text: &str, table: &str, key: &str, value: Option<toml_edit::Item>) -> Result<String, String> {
    use toml_edit::{DocumentMut, Item, Table};
    let mut doc: DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| format!("the file does not read as TOML: {e}"))?;
    let mut t = doc.as_table_mut() as &mut dyn toml_edit::TableLike;
    if !table.is_empty() {
        if !t.contains_key(table) {
            t.insert(table, Item::Table(Table::new()));
        }
        t = t.get_mut(table).and_then(Item::as_table_like_mut).ok_or(format!("the file's `{table}` is no table"))?;
    }
    match (value, t.get_mut(key)) {
        (None, _) => drop(t.remove(key)),
        (Some(v), Some(item)) => *item = v,
        (Some(v), None) => drop(t.insert(key, v)),
    }
    Ok(doc.to_string())
}

/// FILE written as EDIT makes its text, beside the coord: never through a link, nor into a git repository outside the
/// coord (the flavor's), whose install would move it; the file as it was kept as `.previous`.
fn write_file(coord: &str, file: &Path, edit: impl FnOnce(&str) -> Result<String, String>) -> Result<String, String> {
    if std::fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} is a link: written through, the line would go wherever it points", file.display()));
    }
    let dir = file.parent().ok_or("no folder")?;
    let canon = |q: &Path| std::fs::canonicalize(q).unwrap_or_else(|_| q.to_path_buf());
    if let Some(top) = crate::change::repository(dir).filter(|_| !canon(dir).starts_with(canon(Path::new(coord)))) {
        return Err(format!("{} is in {}, a git repository outside this coord, which an install moves; it is set there, through that repository", file.display(), top.display()));
    }
    let old = match std::fs::read_to_string(file) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("{}: {e}", file.display())),
    };
    let text = edit(old.as_deref().unwrap_or(""))?;
    let back = match &old {
        Some(o) => {
            let previous = file.with_file_name(format!("{}.previous", file.file_name().unwrap_or_default().to_string_lossy()));
            std::fs::write(&previous, o).map_err(|e| format!("{}: {e}", previous.display()))?;
            format!("{} is the file as it was", previous.display())
        }
        None => format!("remove {}", file.display()),
    };
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::write(file, &text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(format!("written to {}; the way back: {back}", file.display()))
}

/// An agent's name as the speakers' list takes it.
fn is_name(n: &str) -> bool {
    !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// VALUE for KEY as the file keeps it, or why it is none of KEY's; `""` is unset (Ok(None)).
fn parse(key: &str, value: &str) -> Result<Option<toml_edit::Item>, String> {
    let item = |v: toml_edit::Value| Ok(Some(toml_edit::Item::Value(v)));
    match (key, value) {
        (_, "") => Ok(None),
        ("agent_mode", w) if mode_yolo(w).is_some() => item(w.into()),
        ("agent_mode", w) => Err(format!("{w} is not a mode: safe or yolo, or nothing")),
        ("safety_check" | "boxed", "true") => item(true.into()),
        ("safety_check" | "boxed", "false") => item(false.into()),
        ("safety_check" | "boxed", w) => Err(format!("{w}: {key} is true or false, or nothing")),
        ("owner_speakers", w @ ("leads" | "any")) => item(w.into()),
        ("owner_speakers", names) => {
            let list: Vec<&str> = names.split(',').map(str::trim).collect();
            match list.iter().find(|n| !is_name(n)) {
                Some(n) => Err(format!("{n:?} is not an agent's name: the speakers are leads, any, or names with commas between")),
                None => item(toml_edit::Value::Array(list.into_iter().collect())),
            }
        }
        (k, m) if k.starts_with("models.") && spot(k).is_some() && !m.contains(char::is_whitespace) => item(m.into()),
        (k, m) if k.starts_with("models.") && spot(k).is_some() => Err(format!("{m:?} is not a model's id")),
        (k, _) => Err(format!("{k}: no such setting; the settings are {}", keys().join(", "))),
    }
}

/// KEY set to VALUE (`""` unset) at LEVEL `yaaihh` or `project`; what was written and its way back.
pub fn set(at: &Places, project: Option<&Project>, level: &str, key: &str, value: &str) -> Result<String, String> {
    if !LEVELS.contains(&level) {
        return Err(format!("{level}: the level is yaaihh, project or agent:NAME"));
    }
    let v = parse(key, value)?;
    let what = if value.is_empty() { format!("{key} unset") } else { format!("{key} = {value}") };
    if level == "project" && key != "safety_check" {
        let (table, k) = spot(key).ok_or(format!("{key}: no such setting"))?;
        let change = Change { done: format!("{what} written"), there: "a project's setting is set there", message: format!("the project's {what}, set by the owner") };
        return edit_project_file(at, &change, |old, dir| {
            let text = with_key(old, table, &k, v)?;
            let mut p: Project = toml::from_str(&text).map_err(|e| format!("the project file as it would be: {e}"))?;
            p.order = crate::project::repo_order(&text);
            p.dir = dir.display().to_string();
            p.check_repos()?;
            Ok(text)
        });
    }
    let Some((file, table, k)) = place(&at.coord, project, level, key) else {
        return Err(format!("{key} has no place at {level} here: {}", if level == "yaaihh" { "this project is in no flavor" } else { "this coord has no project file" }));
    };
    write_file(&at.coord, &file, |old| with_key(old, table, &k, v)).map(|w| format!("{what}, {w}"))
}

#[cfg(test)]
#[path = "tests/settings.rs"]
mod tests;
