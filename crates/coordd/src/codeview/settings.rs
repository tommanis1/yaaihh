//! The workbench's settings from coordd: the keys that keep every prompt, tip and bar away, as our own extension's
//! defaults (workbench.rs), which reach every setting; and the server's machine settings
//! (`server/data/Machine/settings.json`, read again when it changes), written at each open: the theme, the window's
//! own colours as the workbench's, his editor settings when he opts in. What he sets inside the workbench is its own.

use coord_types::codeview::{CodeLook, CodeLooks};
use serde_json::{Map, Value, json};
use std::path::Path;

/// The default themes the window's looks colour over.
const DARK: &str = "Default Dark Modern";
const LIGHT: &str = "Default Light Modern";

/// No trust prompt, welcome, tips, walkthrough, recommendation, update, telemetry or chat; only the Explorer, the
/// editor and the diff, with no bar of VS Code's own around them.
pub fn managed() -> Map<String, Value> {
    let v = json!({
        "security.workspace.trust.enabled": false,
        "workbench.startupEditor": "none",
        "workbench.tips.enabled": false,
        "workbench.welcomePage.walkthroughs.openOnInstall": false,
        "workbench.enableExperiments": false,
        "extensions.ignoreRecommendations": true,
        "extensions.autoCheckUpdates": false,
        "extensions.autoUpdate": false,
        "update.mode": "none",
        "telemetry.telemetryLevel": "off",
        "chat.disableAIFeatures": true,
        "chat.commandCenter.enabled": false,
        "window.commandCenter": false,
        "window.menuBarVisibility": "hidden",
        "workbench.layoutControl.enabled": false,
        "workbench.activityBar.location": "hidden",
        "workbench.statusBar.visible": false,
        "workbench.secondarySideBar.defaultVisibility": "hidden",
        "workbench.panel.defaultLocation": "bottom",
        "workbench.editor.enablePreview": false,
        "editor.minimap.enabled": false,
        "git.autofetch": false,
        "git.openRepositoryInParentFolders": "always",
        "scm.graph.showIncomingChanges": false,
    });
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// LOOK as the workbench's colours: grounds, text, borders, tabs, lists, selection and the one accent.
pub fn colours(l: &CodeLook) -> Map<String, Value> {
    let pairs: [(&str, &str); 44] = [
        ("editor.background", &l.bg),
        ("editorGutter.background", &l.bg),
        ("editorGroupHeader.tabsBackground", &l.layer),
        ("editorGroupHeader.noTabsBackground", &l.bg),
        ("editorGroup.border", &l.border),
        ("tab.activeBackground", &l.bg),
        ("tab.inactiveBackground", &l.layer),
        ("tab.hoverBackground", &l.raised),
        ("tab.border", &l.border),
        ("tab.activeBorder", &l.bg),
        ("tab.activeBorderTop", &l.accent),
        ("tab.activeForeground", &l.text),
        ("tab.inactiveForeground", &l.muted),
        ("sideBar.background", &l.layer),
        ("sideBar.foreground", &l.text),
        ("sideBar.border", &l.border),
        ("sideBarTitle.foreground", &l.muted),
        ("sideBarSectionHeader.background", &l.layer),
        ("sideBarSectionHeader.foreground", &l.muted),
        ("sideBarSectionHeader.border", &l.border),
        ("panel.background", &l.layer),
        ("panel.border", &l.border),
        ("panelTitle.activeForeground", &l.text),
        ("panelTitle.inactiveForeground", &l.muted),
        ("panelTitle.activeBorder", &l.accent),
        ("foreground", &l.text),
        ("editor.foreground", &l.text),
        ("descriptionForeground", &l.muted),
        ("editorLineNumber.foreground", &l.faint),
        ("editorLineNumber.activeForeground", &l.muted),
        ("editor.lineHighlightBackground", &l.raised),
        ("editor.selectionBackground", &l.selection),
        ("selection.background", &l.selection),
        ("list.activeSelectionBackground", &l.raised),
        ("list.activeSelectionForeground", &l.text),
        ("list.inactiveSelectionBackground", &l.raised),
        ("list.hoverBackground", &l.raised),
        ("focusBorder", &l.accent),
        ("textLink.foreground", &l.accent),
        ("progressBar.background", &l.accent),
        ("input.background", &l.bg),
        ("input.border", &l.border),
        ("editorWidget.background", &l.layer),
        ("widget.border", &l.border),
    ];
    pairs.into_iter().filter(|(_, c)| !c.is_empty()).map(|(k, c)| (k.to_string(), Value::from(c))).collect()
}

/// The machine settings at FILE written whole: HIS editor keys, THEME (`dark`, `light`; none keeps the one there) and
/// LOOKS (none keeps the colours there).
pub fn write(file: &Path, his: Map<String, Value>, theme: Option<&str>, looks: Option<&CodeLooks>) -> Result<(), String> {
    let was = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str::<Value>(&super::workbench::jsonc(&t)).ok()).unwrap_or_default();
    let mut s = his;
    let theme = match theme {
        Some("light") => LIGHT,
        Some(_) => DARK,
        None => was["workbench.colorTheme"].as_str().filter(|t| [DARK, LIGHT].contains(t)).unwrap_or(DARK),
    };
    s.insert("workbench.colorTheme".into(), theme.into());
    let colours = looks.map(|l| json!({ format!("[{DARK}]"): colours(&l.dark), format!("[{LIGHT}]"): colours(&l.light) })).or_else(|| was.get("workbench.colorCustomizations").cloned());
    if let Some(c) = colours {
        s.insert("workbench.colorCustomizations".into(), c);
    }
    let text = serde_json::to_string_pretty(&Value::Object(s)).unwrap_or_default();
    if std::fs::read_to_string(file).ok().as_deref() == Some(text.as_str()) {
        return Ok(());
    }
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    super::workbench::write(file, &text)
}
