//! A thread's title as the Map's cards say it, far out where a thread is its card (N-1934 3): what it is about, never
//! who said it ("he says …") or a bare id.

/// The words a title's speaker is said in, dropped from its start.
const SPEAKERS: [&str; 8] = ["the owner says", "owner says", "he says", "she says", "they say", "he said", "she said", "they said"];

/// The most words a title made of a thread's first post keeps.
const WORDS: usize = 6;

/// Thread ID's title as its card says it, in a few words (the owner, Q-1941): TITLE without who says it; one that is
/// only an id, or none, the words of its first post FIRST; either cut to [`WORDS`]; else the id.
pub fn title(title: Option<&str>, first: Option<&str>, id: &str) -> String {
    let said = title.map(unspoken).filter(|t| !t.is_empty() && !is_id(t));
    let said = said.or_else(|| first.map(unspoken).filter(|t| !t.is_empty() && !is_id(t))).map(cut);
    said.map_or_else(|| id.to_string(), capital)
}

/// T without a speaker's words at its start, and the marks after them (`he says: "…"` → `…`).
fn unspoken(t: &str) -> String {
    let mut t = t.trim();
    let spoken = |t: &str, s: &str| t.get(..s.len()).is_some_and(|h| h.eq_ignore_ascii_case(s)) && !t[s.len()..].starts_with(char::is_alphanumeric);
    while let Some(s) = SPEAKERS.iter().find(|s| spoken(t, s)) {
        t = t[s.len()..].trim_start_matches([':', ',', ' ', '—', '-', '"', '“']);
    }
    t.trim_end_matches(['"', '”']).trim().to_string()
}

/// Whether T is an id and nothing else: a thread, item or post (`N-81`, `I-5`, `N-81.p3`).
fn is_id(t: &str) -> bool {
    let t = t.split_once(".p").map_or(t, |(n, p)| if !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) { n } else { "" });
    t.split_once('-').is_some_and(|(l, n)| !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphabetic()) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// T cut to its first [`WORDS`] words, with "…" when it was longer.
fn cut(t: String) -> String {
    let words: Vec<&str> = t.split_whitespace().collect();
    if words.len() <= WORDS { words.join(" ") } else { format!("{} …", words[..WORDS].join(" ")) }
}

/// T with its first letter a capital.
fn capital(t: String) -> String {
    let mut c = t.chars();
    c.next().map_or(t.clone(), |f| f.to_uppercase().chain(c).collect())
}

#[cfg(test)]
mod tests {
    use super::title;

    /// The owner's satellite view (N-1934 3): a topic, never who said it or a bare id.
    #[test]
    fn a_title_says_what_the_thread_is_about_never_who_said_it_or_an_id() {
        assert_eq!(title(Some("he says the map is too slow"), None, "N-3"), "The map is too slow");
        assert_eq!(title(Some("He says: \"fix the sidebar\""), None, "N-3"), "Fix the sidebar");
        assert_eq!(title(Some("he says N-81"), Some("Make the release notes short and plain for everyone"), "N-81"), "Make the release notes short and …");
        assert_eq!(title(Some("N-81.p4"), Some("Plan"), "N-81"), "Plan");
        assert_eq!(title(None, None, "N-81"), "N-81");
        assert_eq!(title(Some("Hacker News scan"), None, "N-122"), "Hacker News scan");
        assert_eq!(title(Some("the window's sidebar shows every project at once and nothing folds"), None, "N-9"), "The window's sidebar shows every project …", "a few words");
        assert_eq!(title(Some("Heading home"), None, "N-5"), "Heading home", "a word that begins like a speaker's stays");
        assert_eq!(title(Some("they sayonara"), None, "N-5"), "They sayonara");
    }
}
