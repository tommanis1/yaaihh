//! The owner's comments on passages as data: each a [`Comment`] kept on its post (`Post::comments`), given by
//! `Line::Comments` and resolved or reopened by `Line::CommentResolved`. [`render`] is the one way they are worded into
//! the post's text for its readers; [`read_back`] reads them out of a post from before them, which holds only that text.

use crate::line::At;
use serde::{Deserialize, Serialize};

/// The passage a comment is on: SOURCE, where it came from (`N-481.p13`, `AGENT@OFFSET`, a mail's id), LABEL, how its
/// reader is told so (`N-481.p13, lead's message at 12:44`), QUOTE, its words, and AT, its byte ranges in the source's
/// Markdown (empty when not known: then the words are found).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    pub quote: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub at: Vec<[usize; 2]>,
}

impl Anchor {
    /// The post it is on (`N-481.p13`), when its source or label names one.
    pub fn post(&self) -> Option<&str> {
        post_id(&self.source).or_else(|| post_id(&self.label))
    }
}

/// The post id S begins with (`N-481.p13, lead's message`), if it does.
fn post_id(s: &str) -> Option<&str> {
    let id = s.split([',', ' ']).next()?;
    let (t, n) = id.split_once(".p")?;
    (t.contains('-') && !n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then_some(id)
}

/// One comment: ID (`c1`, `c2`… in its post; empty when read back from an older post's text), its AUTHOR, when it was
/// given (AT), the passage it is on, his note (BODY), and whether it is RESOLVED.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub at: At,
    pub target: Anchor,
    pub body: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resolved: bool,
}

/// COMMENTS worded for a post's readers, then TYPED: each its passage quoted, a `(from LABEL)` line, and its note; the
/// paragraphs apart by a blank line.
pub fn render(comments: &[Comment], typed: &str) -> String {
    let mut parts: Vec<String> = comments
        .iter()
        .map(|c| {
            let mut q: String = c.target.quote.lines().map(|l| format!("> {l}\n")).collect();
            if !c.target.label.is_empty() {
                q.push_str(&format!("> (from {})\n", c.target.label));
            }
            format!("{q}\n{}", c.body.trim())
        })
        .collect();
    if !typed.trim().is_empty() {
        parts.push(typed.trim().to_string());
    }
    parts.join("\n\n")
}

/// The comments [`render`] wrote into TEXT, read back for a post from before `Post::comments`: a block of `> ` lines
/// (its `(from …)` line the label), then its note, every paragraph up to the next passage. Words under no passage are
/// no comment.
pub fn read_back(text: &str) -> Vec<Comment> {
    let mut out = Vec::new();
    let mut quote: Option<Anchor> = None;
    let mut note: Vec<&str> = Vec::new();
    let mut flush = |quote: &mut Option<Anchor>, note: &mut Vec<&str>| {
        if let Some(target) = quote.take() {
            let body = note.join("\n\n").trim().to_string();
            if !target.quote.is_empty() && !body.is_empty() {
                out.push(Comment { target, body, ..Default::default() });
            }
        }
        note.clear();
    };
    for part in text.split("\n\n") {
        let lines: Vec<&str> = part.lines().filter(|l| !l.trim().is_empty()).collect();
        if !lines.is_empty() && lines.iter().all(|l| l.trim_start().starts_with('>')) {
            flush(&mut quote, &mut note);
            let ls: Vec<&str> = lines.iter().map(|l| l.trim_start().trim_start_matches('>').trim()).collect();
            let label = ls.iter().find_map(|l| from_line(l)).unwrap_or_default().to_string();
            let words: Vec<&str> = ls.iter().filter(|l| from_line(l).is_none()).copied().collect();
            let mut a = Anchor { label, quote: words.join("\n"), ..Default::default() };
            a.source = a.post().unwrap_or_default().to_string();
            quote = Some(a);
        } else if quote.is_some() && !part.trim().is_empty() {
            note.push(part.trim());
        }
    }
    flush(&mut quote, &mut note);
    out
}

/// The label a `(from LABEL)` line names.
fn from_line(l: &str) -> Option<&str> {
    l.strip_prefix("(from ")?.strip_suffix(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(source: &str, quote: &str, body: &str) -> Comment {
        Comment { id: "c1".into(), author: "owner".into(), at: "2026-11-26T10:00:00Z".into(), target: Anchor { source: source.into(), label: format!("{source}, lead's message at 10:01"), quote: quote.into(), at: vec![[3, 9]] }, body: body.into(), resolved: false }
    }

    /// What `render` writes, `read_back` reads: passage, label, the post it is on and the note, a note of two
    /// paragraphs whole; the words typed after the last comment stand with its note, as they always read.
    #[test]
    fn a_rendered_comment_reads_back_as_its_passage_post_and_note() {
        let two = [on("N-1.p2", "the lock is held", "by whom?"), on("N-1.p4", "line one\nline two", "first part\n\nsecond part")];
        let text = render(&two, "");
        assert_eq!(text, "> the lock is held\n> (from N-1.p2, lead's message at 10:01)\n\nby whom?\n\n> line one\n> line two\n> (from N-1.p4, lead's message at 10:01)\n\nfirst part\n\nsecond part");
        let back = read_back(&text);
        assert_eq!(back.iter().map(|c| (c.target.source.as_str(), c.target.quote.as_str(), c.body.as_str())).collect::<Vec<_>>(), [("N-1.p2", "the lock is held", "by whom?"), ("N-1.p4", "line one\nline two", "first part\n\nsecond part")]);
        assert!(back.iter().all(|c| c.id.is_empty() && !c.resolved), "read back: no id, as it had none");
        assert!(read_back("no quote here\n\nnor here").is_empty());
        assert_eq!(render(&[], "just words"), "just words");
    }

    /// A post from before comments were data has none in its JSON; one with them carries them whole, and resolved only
    /// once it is.
    #[test]
    fn a_comment_goes_through_json_whole_and_an_older_line_has_none() {
        let c = on("agent@120", "a", "b");
        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("resolved").is_none(), "not resolved: not written");
        assert_eq!(serde_json::from_value::<Comment>(v).unwrap(), c);
        let old: Comment = serde_json::from_str(r#"{"target":{"quote":"q"},"body":"n"}"#).unwrap();
        assert_eq!((old.id.as_str(), old.target.post()), ("", None));
        assert_eq!(on("x@1", "q", "n").target.post(), None, "an entry of a session is on no post");
    }
}
