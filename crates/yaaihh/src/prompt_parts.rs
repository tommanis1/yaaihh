//! Claude Code's own system prompt as parts, for the parts editor on the System prompt pane (the owner, N-1104 p1158
//! as fresh-lead-3 showed it, his yes p1160; p2995's switches, consult-lead's p3000; the shape accounts-lead N-1446
//! p3065, docket-edge8 p3067). Read from the text a request carried, never from a fixed list: a team agent's prompt
//! and a bare start's differ (a team agent's has no "act when you have enough information" paragraph and has its
//! tools' lines), and each Claude Code version writes its own.
//!
//! Claude Code 2.1.331 sends three system blocks (recorded against a loopback stand-in, p3065): its billing line, its
//! identity line, and one main text. Its `--append-system-prompt` text is joined onto the main text's end after a
//! blank line; `--system-prompt` replaces the main text alone. [`parts`] splits that main text: a `# ` heading with
//! lines under it in the same paragraph is one part (`# Harness`, `# Environment`), a heading that stands alone takes
//! the paragraphs after it up to the next heading (`# Memory`), and every other paragraph is a part of its own
//! (pronouns, care, privacy). What follows Claude Code's own text, the appended text the start gave, is ours.

/// One part of a system prompt's main text, in the order it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// What the pane calls it: its heading, a known opening's name, else its first words.
    pub name: String,
    /// Its text as sent, its paragraphs joined by the blank lines between them.
    pub text: String,
    /// Ours: the text the agent's start appended, not Claude Code's.
    pub ours: bool,
}

impl Part {
    /// How many lines it is, as the pane counts what the harness adds.
    pub fn lines(&self) -> usize {
        self.text.lines().count()
    }
}

/// The names of the unheaded paragraphs Claude Code writes, by how each opens (2.1.331). One it does not list is named
/// by its first words, so a new paragraph in a later version still shows, under words of its own.
const OPENINGS: [(&str, &str); 7] = [
    ("You are an agent working with the user", "Agent line"),
    ("IMPORTANT: Assist with authorized security", "Security"),
    ("When you use a pronoun", "Pronouns"),
    ("For actions that are hard to reverse", "Care"),
    ("You must treat information you learn", "Privacy"),
    ("When you have enough information to act", "Acting"),
    ("Do build original games", "Games"),
];

/// How many words name a paragraph that has neither a heading nor a known opening.
const NAME_WORDS: usize = 5;

/// MAIN, the main text a request carried (its last system block), as parts in order; APPENDED, the text the agent's
/// start appended to it (`--append-system-prompt`), when the start gave one: found at MAIN's end, it is one part,
/// ours. Under a replace MAIN is the agent's own text whole, and the caller passes it as APPENDED.
pub fn parts(main: &str, appended: Option<&str>) -> Vec<Part> {
    let (own, ours) = match appended.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => match main.trim_end().strip_suffix(a) {
            Some(rest) => (rest, Some(a)),
            None => (main, None),
        },
        None => (main, None),
    };
    let mut out: Vec<Part> = Vec::new();
    // the heading standing alone whose part takes the paragraphs after it, while one is open
    let mut gathering = false;
    for para in own.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        match heading(para) {
            Some(h) => {
                gathering = para.lines().count() == 1;
                out.push(Part { name: h.to_string(), text: para.to_string(), ours: false });
            }
            None if gathering => {
                let last = out.last_mut().expect("a gathering heading's part");
                last.text.push_str("\n\n");
                last.text.push_str(para);
            }
            None => out.push(Part { name: name_of(para), text: para.to_string(), ours: false }),
        }
    }
    if let Some(a) = ours {
        out.push(Part { name: "Appended: ours".to_string(), text: a.to_string(), ours: true });
    }
    out
}

/// A paragraph's heading (`# Memory`: "Memory"), when its first line is one.
fn heading(para: &str) -> Option<&str> {
    para.lines().next()?.strip_prefix("# ").map(str::trim)
}

/// An unheaded paragraph's name: its known opening's, else its first words.
fn name_of(para: &str) -> String {
    if let Some((_, n)) = OPENINGS.iter().find(|(o, _)| para.starts_with(o)) {
        return n.to_string();
    }
    let words: Vec<&str> = para.split_whitespace().take(NAME_WORDS + 1).collect();
    let more = words.len() > NAME_WORDS;
    let name = words[..words.len().min(NAME_WORDS)].join(" ");
    if more { format!("{name} …") } else { name }
}

/// What one part goes on as, by its switch on the parts editor (p2995): Claude Code's own text as it sent it, that text
/// as he changed it, a text of his in its place (ours, the appended text: the harness's standing text, or his own
/// words, today's Edit), or nothing: Off, the part left out in one press (his yes p1160 to p1158's rows, "Pronouns
/// ●off"; consult-lead N-1724 p9, fresh p15). Off and a Custom left empty are one state: both send nothing, both "● off".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Stock,
    Modified,
    Custom,
    Off,
}

impl Pick {
    /// In the switch's order.
    pub const ALL: [Pick; 4] = [Pick::Stock, Pick::Modified, Pick::Custom, Pick::Off];
    pub const LABELS: [&'static str; 4] = ["Stock", "Modified", "Custom", "Off"];

    pub fn at(self) -> usize {
        Pick::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// Its word in ui.json, where a draft is kept per agent (prompt_edit.rs `saved`).
    pub fn word(self) -> &'static str {
        match self {
            Pick::Stock => "stock",
            Pick::Modified => "modified",
            Pick::Custom => "custom",
            Pick::Off => "off",
        }
    }

    pub fn of_word(w: &str) -> Option<Pick> {
        Pick::ALL.into_iter().find(|p| p.word() == w)
    }
}

/// One part as the editor sends it: its stock text, none where Claude Code has no part of its name; the text it goes on
/// with; whether it is ours.
pub struct Going<'a> {
    pub stock: Option<&'a str>,
    pub text: &'a str,
    pub ours: bool,
}

/// The version PARTS make (p2995: what the switches add up to) and whether it is in place of Claude Code's own: with
/// every part of Claude Code's own as it sent it, ours alone, after Claude Code's own; else the parts in order, an
/// empty one left out, ours last, in place of it. None when it would be empty. Nothing in them needs filling at a
/// start: Claude Code 2.1.332's main text names no working folder or model (those go with the first message; its
/// Environment part lists the model family alone), and its Memory part names the memory folder of the agent's project,
/// the same at each of its starts (measured against a loopback stand-in, H-1716).
pub fn compose(parts: &[Going]) -> Option<(String, bool)> {
    let ours = parts.iter().filter(|g| g.ours).map(|g| g.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    if parts.iter().all(|g| g.ours || g.stock == Some(g.text)) {
        return Some((ours, false)).filter(|(t, _)| !t.is_empty());
    }
    let mut out: Vec<String> = parts.iter().filter(|g| !g.ours).map(|g| g.text.trim()).filter(|t| !t.is_empty()).map(str::to_string).collect();
    out.extend(Some(ours).filter(|o| !o.is_empty()));
    Some((out.join("\n\n"), true)).filter(|(t, _)| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up main text in 2.1.331's order and headings (none of Claude Code's words past each opening).
    const MAIN: &str = "\nYou are an agent working with the user, made up.\n\nIMPORTANT: Assist with authorized security work, made up.\n\n# Harness\n - one\n - two\n\nWhen you use a pronoun, made up.\n\nFor actions that are hard to reverse, made up.\n\nYou must treat information you learn, made up.\n\n# Memory\n\nYou have a memory at FOLDER, made up.\n\n```markdown\nx\n```\n\n# Environment\n - Primary working directory: W\n - Platform: linux\n\n# Context management\nSummarized, made up.\n\nDo build original games, made up.\n\nEndConversation (deferred tool): made up, a tool's own line.";

    fn names(ps: &[Part]) -> Vec<&str> {
        ps.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn a_main_text_splits_into_its_parts_in_order() {
        let ps = parts(MAIN, None);
        assert_eq!(
            names(&ps),
            ["Agent line", "Security", "Harness", "Pronouns", "Care", "Privacy", "Memory", "Environment", "Context management", "Games", "EndConversation (deferred tool): made up, …"]
        );
        assert!(ps.iter().all(|p| !p.ours));
    }

    #[test]
    fn a_heading_alone_takes_the_paragraphs_up_to_the_next_heading() {
        let ps = parts(MAIN, None);
        let memory = ps.iter().find(|p| p.name == "Memory").unwrap();
        assert_eq!(memory.text, "# Memory\n\nYou have a memory at FOLDER, made up.\n\n```markdown\nx\n```");
        // a heading with lines under it is that paragraph alone: what follows it stands apart
        let harness = ps.iter().find(|p| p.name == "Harness").unwrap();
        assert_eq!(harness.text, "# Harness\n - one\n - two");
        assert_eq!(harness.lines(), 3);
    }

    #[test]
    fn the_appended_text_at_its_end_is_one_part_ours() {
        let main = format!("{MAIN}\n\nOur team text.\n\nIts second paragraph.");
        let ps = parts(&main, Some("Our team text.\n\nIts second paragraph."));
        let last = ps.last().unwrap();
        assert!(last.ours);
        assert_eq!(last.text, "Our team text.\n\nIts second paragraph.");
        assert_eq!(names(&ps[..ps.len() - 1]), names(&parts(MAIN, None)));
    }

    #[test]
    fn an_appended_text_not_at_its_end_is_not_taken_for_ours() {
        // a start's text that this request did not carry (an older request, another version): nothing is ours
        let ps = parts(MAIN, Some("Text it never carried."));
        assert!(ps.iter().all(|p| !p.ours));
        assert_eq!(ps.len(), parts(MAIN, None).len());
    }

    #[test]
    fn a_paragraph_no_version_named_shows_under_its_first_words() {
        let ps = parts("\nA new paragraph of a later version, longer than five words.\n\nShort one.", None);
        assert_eq!(names(&ps), ["A new paragraph of a …", "Short one."]);
    }

    #[test]
    fn every_part_stock_sends_ours_alone_after_claude_code_s_own() {
        let g = [Going { stock: Some("A"), text: "A", ours: false }, Going { stock: None, text: "Ours.", ours: true }];
        assert_eq!(compose(&g), Some(("Ours.".into(), false)));
        assert_eq!(compose(&[Going { stock: Some("A"), text: "A", ours: false }]), None, "nothing to send");
    }

    #[test]
    fn a_changed_part_sends_the_parts_in_place_of_claude_code_s_own_an_empty_one_left_out() {
        let g = [
            Going { stock: Some("Agent."), text: "Agent, changed.", ours: false },
            Going { stock: Some("Care."), text: "", ours: false },
            Going { stock: Some("# Memory\n\nAt F."), text: "# Memory\n\nAt F.", ours: false },
            Going { stock: None, text: "Ours.", ours: true },
        ];
        assert_eq!(compose(&g), Some(("Agent, changed.\n\n# Memory\n\nAt F.\n\nOurs.".into(), true)));
    }

    /// The parts editor's text from a real request, for a try against a loopback stand-in (paper-research H-1716.p2):
    /// `PARTS_TRY=DIR` holding main.txt (a request's last system block), appended.txt and pick.txt
    /// (NAME: that part switched to Modified, a made-up sentence added; NAME=: to Custom, left empty); writes parts.txt (each part's name and lines),
    /// modified.txt with modified.mode, and stock.txt with stock.mode (every part back to Stock).
    #[test]
    #[ignore]
    fn parts_try() {
        let Ok(d) = std::env::var("PARTS_TRY") else { return };
        let read = |f: &str| std::fs::read_to_string(format!("{d}/{f}")).unwrap();
        let write = |f: &str, t: &str| std::fs::write(format!("{d}/{f}"), t).unwrap();
        let (main, appended, pick) = (read("main.txt"), read("appended.txt"), read("pick.txt"));
        let ps = parts(&main, Some(&appended));
        write("parts.txt", &ps.iter().map(|p| format!("{} · {} lines{}", p.name, p.lines(), if p.ours { " · ours" } else { "" })).collect::<Vec<_>>().join("\n"));
        // "NAME" switched to Modified, a made-up sentence added; "NAME=" to Custom, left empty (the part left out)
        let (name, empty) = match pick.trim().strip_suffix('=') {
            Some(n) => (n, true),
            None => (pick.trim(), false),
        };
        let changed: Vec<String> = ps.iter().map(|p| match (p.name == name, empty) {
            (true, true) => String::new(),
            (true, false) => format!("{} A made-up change.", p.text),
            (false, _) => p.text.clone(),
        }).collect();
        assert!(ps.iter().any(|p| p.name == name), "no part {name}");
        fn going<'a>(ps: &'a [Part], texts: &'a [String]) -> Vec<Going<'a>> {
            ps.iter().zip(texts).map(|(p, t)| Going { stock: Some(p.text.as_str()).filter(|_| !p.ours), text: t.as_str(), ours: p.ours }).collect()
        }
        let (t, replace) = compose(&going(&ps, &changed)).unwrap();
        write("modified.txt", &t);
        write("modified.mode", if replace { "replace" } else { "append" });
        let stock: Vec<String> = ps.iter().map(|p| p.text.clone()).collect();
        let (t, replace) = compose(&going(&ps, &stock)).unwrap();
        write("stock.txt", &t);
        write("stock.mode", if replace { "replace" } else { "append" });
    }
}
