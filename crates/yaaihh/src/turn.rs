//! The work behind a reply (paper-research N-101 p1259, point 6, the owner: "i have 5 differnet windows ... isntead of
//! integrated ux"): a click on "› the work behind it" on an agent's reply opens, over the chat it stands in, the turn
//! that led to it as the Work view draws it (its runs of tool calls folded, each opening on a click); under that work
//! "› what the model was sent" opens the proxy's requests of that turn, folded the same way (raw.rs), in an OS window of
//! their own beside the window (sent_window.rs; paper-research N-1446 p2112, 2). The work opens apart (popup.rs), not
//! under the reply: the owner, N-1104 p562, "otherwise it destroys the chat and i cannot find the close at the top
//! again". The chat does not move, the view stays the one the reply is in, and the popup stays as it was.
//!
//! The turn is read from the session: from the last prompt it was given (a `user` or `mail` entry) before the reply to
//! the reply itself, with the run of work it ends in; its requests are those the proxy logged between the two times.

use crate::App;
use crate::live::Conv;
use crate::work::Item;
use rune::{El, col, row, text};
use rune_kit::Kit;

/// A moment as the proxy's log keys it: nanoseconds since the epoch; none when T is not a time.
pub fn nanos(t: &str) -> Option<u64> {
    let d = t.parse::<chrono::DateTime<chrono::Utc>>().ok()?;
    u64::try_from(d.timestamp_nanos_opt()?).ok()
}

/// How far after the post's own time its turn's entries still count: the reply tool's call is written as the post is
/// made, its result a moment after.
pub(crate) const SLACK_NS: u64 = 5_000_000_000;

/// The entries of C that make the turn behind a post made at AT: from the one after the turn's prompt to the last at
/// or before AT (and a few seconds); none when the session held does not reach back to it (`Before`) or holds nothing
/// that early.
#[derive(Debug, PartialEq)]
pub enum Span {
    /// Entries FROM..TO, and the prompt's time (where its requests begin).
    In { from: usize, to: usize, since: u64 },
    /// The turn is before the first entry held: earlier entries must be read.
    Before,
    /// Nothing of it in the session (another session, or the post was not made in a turn).
    None,
}

pub fn span(c: &Conv, at: &str) -> Span {
    let Some(until) = nanos(at).map(|t| t + SLACK_NS) else { return Span::None };
    let times: Vec<Option<u64>> = c.entries.iter().map(|e| nanos(&e.at)).collect();
    // the last entry at or before the post (and its slack)
    let Some(end) = times.iter().rposition(|t| t.is_some_and(|t| t <= until)) else {
        return if c.loaded && c.start > 0 { Span::Before } else { Span::None };
    };
    // the prompt that began the turn: the last user or mail entry before it
    match (0..=end).rev().find(|&i| matches!(c.entries[i].kind.as_str(), "user" | "mail")) {
        Some(p) => {
            let to = end + 1;
            if to <= p + 1 {
                return Span::None;
            }
            Span::In { from: p + 1, to, since: times[p].unwrap_or(0) }
        }
        None if c.start > 0 => Span::Before,
        None => Span::In { from: 0, to: end + 1, since: times.first().copied().flatten().unwrap_or(0) },
    }
}

/// The rows of AGENT's work view (chat.rs items) that fall in FROM..TO: an entry in it, a run that starts in it. A
/// brain's pass is none of the turn's work: it stands in the chat beside it.
fn items_in(all: &[Item], from: usize, to: usize) -> Vec<Item> {
    all.iter()
        .filter(|it| {
            let i = match **it {
                Item::Entry(i) => i,
                Item::Work { from: f, .. } => f,
                Item::Call { call, .. } => call,
                Item::Pass(_) => return false,
                Item::Restarted { at, .. } => at,
            };
            i >= from && i < to
        })
        .cloned()
        .collect()
}

/// The work behind agent A's post POST (an id like `N-5.p3`) made at AT, as its popup shows it: the turn's rows, then
/// the line that opens its requests in a window of their own, and while they are read, or were not, a word of it.
pub fn work(s: &App, pi: usize, a: &str, post: &str, at: &str, k: &Kit) -> El {
    let Some(p) = s.projects.get(pi) else { return col() };
    let note = |t: String| text(t).font(k.ty.small).color(k.c.text_faint);
    let mut c = col().w_full().gap(4.0);
    let Some(conv) = p.convs.get(a).filter(|c| c.loaded) else {
        let why = p.convs.get(a).and_then(|c| c.refused.clone()).map_or("reading the session…".to_string(), |w| format!("not read: {w}"));
        return c.child(note(why));
    };
    match span(conv, at) {
        Span::None => c.child(note("Its turn is gone (compacted or restarted).".into())),
        Span::Before => {
            let who = a.to_string();
            let label = if conv.older_pending { "reading earlier entries…" } else { "› read the session back to this reply" };
            c.child(text(label).font(k.ty.small).color(k.c.text_accent).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
                if let Some(p) = s.projects.get_mut(pi) {
                    p.read_older(&who);
                }
            }))
        }
        Span::In { from, to, since } => {
            let all = crate::chat::items(s, pi, a, conv);
            for it in items_in(&all, from, to) {
                c = c.child(crate::chat::item_row(s, pi, a, &it, false, k));
            }
            // what the model was sent in that turn: in a window of its own, titled as this popup is (its thread's label
            // and the post's id), commented on as this popup is: not at all (popup.rs `goes_to`)
            let until = crate::turn::nanos(at).map_or(u64::MAX, |t| t + SLACK_NS);
            let what = crate::popup::What::Work { pi, agent: a.to_string(), post: post.to_string(), at: at.to_string() };
            let from = (s.popups.iter().rev().find(|x| x.what == what).map_or_else(String::new, |x| x.title.clone()), what);
            let id = post.rsplit_once(".p").map_or(post, |(t, _)| t);
            let label = p.live.tickets.values().find(|v| v.id == id).map_or_else(String::new, |v| crate::refs::ticket(Some(p), v).label);
            let title = crate::popup::title(&label, post, &format!("what {a}'s model was sent"));
            let (who, id) = (a.to_string(), post.to_string());
            c = c.child(row().pt(6.0).child(text("› what the model was sent").key(format!("sent-{post}")).font(k.ty.small).color(k.c.text_faint).cursor(rune::Cursor::Pointer).on_press(move |s: &mut App| {
                crate::sent_window::open_turn(s, pi, &who, &id, since, until, title.clone(), Some(from.clone()))
            })));
            let said = match crate::raw::turn_log(s, pi, a, post).and_then(|l| l.refused.as_deref()) {
                Some(why) => Some(crate::raw::cannot(why)),
                None => crate::sent_window::waits(s, pi, a, post).then(|| crate::raw::LOADING.to_string()),
            };
            c.children(said.map(note))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coord_types::Entry;

    fn e(at: &str, kind: &str) -> Entry {
        serde_json::from_value(serde_json::json!({"offset": 0, "at": at, "kind": kind, "text": "x"})).unwrap()
    }

    #[test]
    fn a_replys_turn_runs_from_its_prompt_to_the_reply() {
        let mut c = Conv { loaded: true, ..Default::default() };
        c.entries = vec![
            e("2026-11-17T01:00:00Z", "user"),
            e("2026-11-17T01:00:05Z", "assistant"),
            e("2026-11-17T01:00:06Z", "tool"),
            e("2026-11-17T01:00:07.500Z", "result"),
            e("2026-11-17T01:00:09Z", "tool"),
            e("2026-11-17T01:00:10Z", "result"),
            e("2026-11-17T01:05:00Z", "user"),
            e("2026-11-17T01:05:03Z", "tool"),
        ];
        // the post made at 01:00:09 (its reply-tool call), its result a moment after
        assert_eq!(span(&c, "2026-11-17T01:00:09Z"), Span::In { from: 1, to: 6, since: nanos("2026-11-17T01:00:00Z").unwrap() });
        // the next turn's reply
        assert_eq!(span(&c, "2026-11-17T01:05:03Z"), Span::In { from: 7, to: 8, since: nanos("2026-11-17T01:05:00Z").unwrap() });
        // the session held starts later: read back
        c.start = 100;
        assert_eq!(span(&c, "2026-11-17T00:10:00Z"), Span::Before);
    }
}
