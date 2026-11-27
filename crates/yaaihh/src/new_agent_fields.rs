//! New agent's search, Effort and first message (the QA walk's P1 #8, N-1890.p1): a search that finds a model by the
//! words of its id, name or guess, not by letters scattered through them; the effort it starts at where its harness
//! takes fixed levels; the first message it is given, in place of the standing note.

use crate::App;
use crate::projects::Guess;
use rune::{El, text};
use rune_kit::{ComboboxOption, Kit, SegmentedControl, TextArea};
use std::collections::BTreeMap;

/// Whether every word of QUERY stands, whole or as the start of a part, in model ID, its LABEL or its GUESS: "neon"
/// finds `opencode/neon-2` and "Neon 2", never a model whose words only hold its letters in turn.
pub fn matches(query: &str, id: &str, label: &str, guess: Option<&str>) -> bool {
    let hay = format!("{id} {label} {}", guess.unwrap_or("")).to_lowercase();
    let parts: Vec<&str> = hay.split(|c: char| !c.is_alphanumeric() && c != '.').filter(|p| !p.is_empty()).collect();
    let whole = hay.split_whitespace().collect::<Vec<_>>();
    query.to_lowercase().split_whitespace().all(|w| parts.iter().any(|p| p.starts_with(w)) || whole.iter().any(|p| p.starts_with(w)))
}

/// OPTIONS (one per id of MODELS) left as the search QUERY finds them ([`matches`], its id read with the name of the
/// harness it runs on: "codex" finds `gpt-6.2-sol`); one found by its id alone shows the id, which the combobox's own
/// search reads too.
pub fn searched(options: Vec<ComboboxOption>, guesses: &BTreeMap<String, Guess>, query: &str) -> Vec<ComboboxOption> {
    if query.trim().is_empty() {
        return options;
    }
    options
        .into_iter()
        .filter(|o| matches(query, &format!("{} {}", o.id, coord_types::harness::kind_of_model(&o.id)), &o.label, guesses.get(&o.id).map(|g| g.guess.as_str())))
        .map(|mut o| {
            if !matches(query, "", &o.label, guesses.get(&o.id).map(|g| g.guess.as_str())) && !o.detail.contains(&o.id) {
                o.detail = if o.detail.is_empty() { o.id.clone() } else { format!("{} · {}", o.detail, o.id) };
            }
            o
        })
        .collect()
}

/// The Effort field for the picked model, where its harness takes fixed levels: none picked is its model's default.
pub fn effort(s: &App, k: &Kit) -> Option<El> {
    let na = &s.new_agent;
    let levels = coord_types::harness::efforts(coord_types::harness::kind_of_model(na.model.as_deref()?))?;
    let at = na.effort.as_deref().and_then(|e| levels.iter().position(|l| *l == e));
    let pick = SegmentedControl::new(levels, at)
        .label("Effort")
        .fit()
        .on_pick(move |s: &mut App, i, _| {
            let l = levels[i].to_string();
            s.new_agent.effort = if s.new_agent.effort.as_deref() == Some(l.as_str()) { None } else { Some(l) };
        })
        .build(k)
        .key("new-agent-effort");
    let note = text(if at.is_none() { "Its model's default unless you pick one." } else { "Press it again for its model's default." }).font(k.ty.small).color(k.c.text_faint);
    Some(rune::col().gap(4.0).child(pick).child(note))
}

/// The first message box: optional; empty, the agent waits for his messages.
pub fn first_message(s: &App, disabled: bool, k: &Kit) -> El {
    TextArea::new(&s.new_agent.first).placeholder("Optional: its first message, its task").width(None).auto_grow(2, 8).disabled(disabled).on_edit(|s: &mut App| &mut s.new_agent.first).build(k).key("new-agent-first")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "neon" finds the Neon model by its id, name or guess, and none whose words only hold n, e, o, n in turn; every
    /// word must stand; a model found by its id alone shows it.
    #[test]
    fn a_search_finds_a_model_by_its_words_not_scattered_letters() {
        let free = |id: &str, label: &str| ComboboxOption { id: id.into(), label: label.into(), detail: "Other · guess: a model from a free provider (read: model-guesses.toml)".into(), disabled: false };
        let options = vec![free("opencode/hy4-free", "Hy4 Free"), free("opencode/beam-free", "Beam Free"), free("opencode/hungerg-free", "Hungerg Free"), free("opencode/iceman-free", "Iceman Free"), free("opencode/neon-2", "Neon 2"), ComboboxOption::new("claude-opus-5-5", "Opus 5.5")];
        let mut guesses = BTreeMap::new();
        guesses.insert("opencode/iceman-free".to_string(), Guess { guess: "Neon's fine-tune".into(), read: String::new() });
        let ids = |q: &str| searched(options.clone(), &guesses, q).into_iter().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(ids("neon"), ["opencode/iceman-free", "opencode/neon-2"], "by its name, and one by its guess");
        assert_eq!(ids("NEON 2"), ["opencode/neon-2"]);
        assert_eq!(ids("opus 5.5"), ["claude-opus-5-5"]);
        assert!(ids("free model").is_empty(), "every word must stand");
        assert_eq!(ids("free").len(), 4);
        assert_eq!(ids("").len(), 6);
        let by_id = searched(options.clone(), &guesses, "claude");
        assert_eq!(by_id.iter().map(|o| (o.id.as_str(), o.detail.as_str())).collect::<Vec<_>>(), [("claude-opus-5-5", "claude-opus-5-5")]);
        // a model by the harness it runs on
        let codex = [ComboboxOption::new("gpt-6.2-sol", "GPT-6.2 Sol"), ComboboxOption::new("agy/gemini-4-neon", "Gemini 4 Neon")];
        let found = |q: &str| searched(codex.to_vec(), &guesses, q).into_iter().map(|o| o.id).collect::<Vec<_>>();
        assert_eq!(found("codex"), ["gpt-6.2-sol"]);
        assert_eq!(found("neon"), ["agy/gemini-4-neon"]);
        assert_eq!(ids("codex"), Vec::<String>::new(), "no Codex model among the others");
    }
}
