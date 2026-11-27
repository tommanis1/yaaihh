//! His comments on a post's passages, once sent, stay at the passage with the answer under them (the owner, typed
//! 12:45 18 Nov on N-481: "this question in the ui for example should just appear as a comment on some other thread
//! ie the place i placed it. then your reply should be there"; paper-research N-695.p1, 1).
//!
//! A comment goes in his message as ask.rs `compose` writes it: the passage quoted, "(from N-481.p13, …)", then his
//! note; from `COMMENTS_PROTOCOL` it is kept on the post as data too (`Post::comments`), read from there, an older
//! post's read back from its words. Read back from the record, each comment stands at the post it names (one message with comments on two posts
//! stands at both, consult-2 N-586.p17, 1): the passage stays highlighted there, and under the post stand his note
//! and, faint, the first line of the answer; a click opens the answer whole in that place, with a link to it in the
//! thread. The answer is the first later post by an agent his message reached (named or told: an agent's turn-end
//! reply carries no `answers`), or one that answers it by the reply tool (`answers`, `also`). No request is new.

use crate::App;
use docket_machine::{Post, TicketView};
use rune::{El, col, row, text};
use rune_kit::Kit;

/// One of his sent comments on a post: the passage, his note, his post's number, whether it is that message's last
/// comment on the post (the answer stands under that one only), and the answer's (post, words).
#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub quote: String,
    pub note: String,
    pub his: u32,
    pub last: bool,
    pub answer: Option<(u32, String)>,
}

/// One comment as `compose` wrote it: the passage, his note, and the post it is on (`N-481.p13`), when it says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Pair {
    pub quote: String,
    pub note: String,
    pub from: Option<String>,
}

impl Pair {
    /// Comment C as its passage, note and post.
    fn of(c: &docket_machine::Comment) -> Pair {
        Pair { quote: c.target.quote.clone(), note: c.body.clone(), from: c.target.post().map(String::from) }
    }
}

/// The comments of TEXT as `compose` writes them (`docket_machine::comment::read_back`): the words typed after the
/// last comment cannot be told from its note's last paragraphs, and stand with it.
pub fn pairs(text: &str) -> Vec<Pair> {
    docket_machine::comment::read_back(text).iter().map(Pair::of).collect()
}

/// The comments of post Q, its words WORDS: those it keeps as data, else those read back from a post from before them.
fn pairs_of(q: &Post, words: &str) -> Vec<Pair> {
    if q.comments.is_empty() { pairs(words) } else { q.comments.iter().map(Pair::of).collect() }
}

/// The answer to his post HIS of V: a later post that answers it by the reply tool (`answers`, `also`), else the
/// first later post, not taken back and not a step's, by an agent it reached (named or told: a turn-end reply).
fn answer_to<'a>(v: &'a TicketView, his: &Post) -> Option<&'a Post> {
    let id = format!("{}.p{}", v.id, his.n);
    let reached = |by: &str| his.names.iter().chain(&his.told).any(|n| n == by);
    let later = || v.posts.iter().filter(|q| q.n > his.n && q.by != "owner" && !q.taken_back && q.step.is_none());
    later().find(|q| q.answers == Some(his.n) || q.also.iter().any(|a| *a == id)).or_else(|| later().find(|q| reached(&q.by)))
}

/// A post's words without the quoted lines it opens with: what the answer says.
pub(crate) fn said(words: &str) -> String {
    words.lines().skip_while(|l| l.trim().is_empty() || l.trim_start().starts_with('>')).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// His sent comments on post N of V, oldest first, with the answers, the words read through WORDS.
pub fn sent(v: &TicketView, n: u32, words: impl Fn(&Post) -> String) -> Vec<Sent> {
    let here = format!("{}.p{n}", v.id);
    let mut out: Vec<Sent> = Vec::new();
    for q in v.posts.iter().filter(|q| q.by == "owner" && !q.taken_back && !q.dropped) {
        let mine: Vec<Pair> = pairs_of(q, &words(q))
            .into_iter()
            .filter(|c| match &c.from {
                Some(f) => *f == here,
                // no "(from …)" line: the post his message answers
                None => q.answers == Some(n),
            })
            .collect();
        let answer = (!mine.is_empty()).then(|| answer_to(v, q)).flatten().map(|a| (a.n, said(&words(a))));
        let last = mine.len().saturating_sub(1);
        for (i, c) in mine.into_iter().enumerate() {
            out.push(Sent { quote: c.quote, note: c.note, his: q.n, last: i == last, answer: if i == last { answer.clone() } else { None } });
        }
    }
    out
}

/// The ticket and post SOURCE names (`N-481.p13`), in P.
fn post_of<'a>(p: &'a crate::Proj, source: &str) -> Option<(&'a TicketView, u32)> {
    let (t, n) = source.split_once(".p")?;
    let n: u32 = n.parse().ok()?;
    p.live.tickets.values().find(|v| v.id == t).map(|v| (v, n))
}

/// His sent comments on post SOURCE (`N-481.p13`) of P, as [`sent`].
pub fn on(s: &App, p: &crate::Proj, source: &str) -> Vec<Sent> {
    let Some((v, n)) = post_of(p, source) else { return Vec::new() };
    sent(v, n, |q| crate::threads::words(&s.tickets, q))
}

/// His sent comments on each post of V, by the post they are on: [`sent`] for every post at once, each of his posts
/// read and parsed once, not once for every post of the thread.
pub fn by_post(v: &TicketView, words: impl Fn(&Post) -> String) -> std::collections::HashMap<u32, Vec<Sent>> {
    let mut out: std::collections::HashMap<u32, Vec<Sent>> = std::collections::HashMap::new();
    let prefix = format!("{}.p", v.id);
    for q in v.posts.iter().filter(|q| q.by == "owner" && !q.taken_back && !q.dropped) {
        // its comments in order, each with the post it is on: the one its "(from …)" names in V, else the post his
        // message answers
        let mut on: Vec<(u32, Pair)> = Vec::new();
        for c in pairs_of(q, &words(q)) {
            let n = match &c.from {
                Some(f) => f.strip_prefix(&prefix).and_then(|n| n.parse::<u32>().ok()).filter(|n| *f == format!("{prefix}{n}")),
                None => q.answers,
            };
            if let Some(n) = n {
                on.push((n, c));
            }
        }
        if on.is_empty() {
            continue;
        }
        let answer = answer_to(v, q).map(|a| (a.n, said(&words(a))));
        let mut posts: Vec<u32> = on.iter().map(|(n, _)| *n).collect();
        posts.dedup();
        posts.sort_unstable();
        posts.dedup();
        for n in posts {
            let mine: Vec<&Pair> = on.iter().filter(|(m, _)| *m == n).map(|(_, c)| c).collect();
            let last = mine.len() - 1;
            let list = out.entry(n).or_default();
            for (i, c) in mine.into_iter().enumerate() {
                list.push(Sent { quote: c.quote.clone(), note: c.note.clone(), his: q.n, last: i == last, answer: if i == last { answer.clone() } else { None } });
            }
        }
    }
    out
}

thread_local! {
    /// [`by_post`] of each thread a view has drawn posts of, for that view alone ([`crate::view_gen`]): the view and the
    /// rows rune builds for it read the state as it stands, which no handler changes in between.
    static DRAWN: std::cell::RefCell<(u64, std::collections::HashMap<(String, String), std::rc::Rc<std::collections::HashMap<u32, Vec<Sent>>>>)> = Default::default();
}

/// [`on`] for a view that draws post SOURCE: the comments of its thread read once in that view, not once a post (the
/// owner's N-1793, item 1: each key in With you's box read and parsed every post of his in the thread again for each of
/// the 36 posts built, 3 ms a key on a thread of 300 posts, growing with the thread). Only for what a view builds; a
/// handler, which may have just changed what it reads, asks [`on`].
pub fn on_drawn(s: &App, p: &crate::Proj, source: &str) -> Vec<Sent> {
    let Some((t, n)) = source.split_once(".p") else { return Vec::new() };
    let Ok(n) = n.parse::<u32>() else { return Vec::new() };
    let view = crate::view_gen();
    let key = (p.id.clone(), t.to_string());
    let held = DRAWN.with_borrow_mut(|(at, m)| {
        if *at != view {
            m.clear();
            *at = view;
        }
        m.get(&key).cloned()
    });
    let index = match held {
        Some(i) => i,
        None => {
            let Some(v) = p.live.tickets.values().find(|v| v.id == t) else { return Vec::new() };
            let i = std::rc::Rc::new(by_post(v, |q| crate::threads::words(&s.tickets, q)));
            DRAWN.with_borrow_mut(|(_, m)| m.insert(key, i.clone()));
            i
        }
    };
    index.get(&n).cloned().unwrap_or_default()
}

/// Where QUOTE stands in SRC (a post's Markdown as drawn): the whole passage where it is found, else its first line.
/// Where it stands twice, the one nearest NEAR (a byte of SRC: where he selected it, its ranges no longer holding its
/// words; consult-3, N-1477 p10), else the first: the place he selected is not in the record of a sent one
/// (`compose`'s places do not go with a thread's message).
pub(crate) fn place(src: &str, quote: &str, near: Option<usize>) -> Option<Vec<std::ops::Range<usize>>> {
    let find = |q: &str| {
        let mut all = rune_kit::markdown::find(src, q, rune_kit::markdown::Case::Sensitive).into_iter();
        match near {
            Some(n) => all.min_by_key(|m| m.first().map_or(usize::MAX, |r| r.start.abs_diff(n))),
            None => all.next(),
        }
    };
    find(quote.trim()).or_else(|| find(quote.lines().map(str::trim).find(|l| !l.is_empty())?))
}

/// The passages SRC (post SOURCE's Markdown as drawn) holds of his sent comments, for their highlight: each comment's
/// place in [`on`], its passage, and where it stands. SRC a part of the post drawn after the words EARLIER (as drawn),
/// a passage EARLIER holds is lit there, not again here (proto-window, N-1477 p8).
pub fn places(s: &App, p: &crate::Proj, source: &str, src: &str, earlier: &str) -> Vec<(usize, String, Vec<std::ops::Range<usize>>)> {
    on_drawn(s, p, source)
        .into_iter()
        .enumerate()
        .filter(|(_, c)| earlier.is_empty() || place(earlier, &c.quote, None).is_none())
        .filter_map(|(j, c)| {
            let at = place(src, &c.quote, None)?;
            Some((j, c.quote, at))
        })
        .collect()
}

/// Under post POST of V: each comment he sent on it, his note and, under a message's last comment there, faint, the
/// answer's first line; a click opens the answer whole here, with a link to where it stands in the thread.
pub fn under(s: &App, p: &crate::Proj, v: &TicketView, post: &Post, k: &Kit) -> Vec<El> {
    let source = format!("{}.p{}", v.id, post.n);
    on_drawn(s, p, &source)
        .into_iter()
        .enumerate()
        .map(|(i, c)| {
            let mark = rune::el().w(2.0).h_full().radius(1.0).fill(k.c.border_weak);
            let quote = text(format!("“{}”", one_line(&c.quote, 60))).font(k.ty.small).color(k.c.text_faint).wrap();
            let mut body = col().grow(1.0).shrink().gap(2.0).child(quote).child(text(format!("you: {}", one_line(&c.note, 200))).font(k.ty.body).color(k.c.text_base).wrap());
            match &c.answer {
                Some((n, words)) => {
                    let to = format!("{}.p{n}", v.id);
                    let by = v.posts.iter().find(|q| q.n == *n).map(|q| q.by.clone()).unwrap_or_default();
                    // opened here only, not under another post the same answer stands at
                    let key = format!("{source}|{to}");
                    let open = s.passage_open.contains(&key);
                    let first = words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
                    let shown = if open { first.to_string() } else { one_line(first, 160) };
                    body = body.child(
                        k.interactive(text(format!("{by}: {shown}")).font(k.ty.small).color(k.c.text_muted).wrap())
                            .help(if open { "Show less" } else { "Show the whole answer here" })
                            .on_press(move |s: &mut App| {
                                if !s.passage_open.remove(&key) {
                                    s.passage_open.insert(key.clone());
                                }
                            }),
                    );
                    if open {
                        let rest: String = words.lines().skip_while(|l| l.trim().is_empty()).skip(1).collect::<Vec<_>>().join("\n");
                        if !rest.trim().is_empty() {
                            body = body.child(crate::post::message_body(Some(p), rest.trim(), None, None, k));
                        }
                        let go = to.clone();
                        body = body.child(k.interactive(text("in the thread →").font(k.ty.small).color(k.c.text_faint)).help("Go to the answer").on_press(move |s: &mut App| {
                            crate::mentions::open(s, &go);
                        }));
                    }
                }
                None if c.last => body = body.child(text("no answer yet").font(k.ty.small).color(k.c.text_faint)),
                None => {}
            }
            row().key(format!("sent-comment-{source}-{i}")).w_full().gap(10.0).padding(8.0).child(mark).child(body)
        })
        .collect()
}

fn one_line(t: &str, n: usize) -> String {
    let l = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if l.chars().count() <= n { l } else { format!("{}…", l.chars().take(n).collect::<String>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(posts: serde_json::Value) -> TicketView {
        serde_json::from_value(serde_json::json!({
            "id": "N-1", "n": 1, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": "t", "status": {"is": "resting"},
            "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-18T10:00:00Z", "given": [], "posts": posts
        }))
        .expect("a view")
    }

    fn plain(q: &Post) -> String {
        serde_json::to_value(&q.text).ok().and_then(|t| t.as_str().map(str::to_string)).unwrap_or_default()
    }

    fn comment(source: &str, quote: &str, note: &str) -> crate::ask::Comment {
        crate::ask::Comment { source: source.into(), label: format!("{source}, lead's message at 10:01"), quote: quote.into(), note: note.into(), at: Vec::new() }
    }

    #[test]
    fn a_sent_comment_reads_back_as_its_passage_note_and_post() {
        let (text, _) = crate::ask::compose(&[comment("N-1.p2", "the lock is held 7–12 s", "by whom?")], "");
        assert_eq!(pairs(&text), vec![Pair { quote: "the lock is held 7–12 s".into(), note: "by whom?".into(), from: Some("N-1.p2".into()) }]);
        let (text, _) = crate::ask::compose(&[comment("N-1.p2", "a", "x"), comment("N-1.p4", "line one\nline two", "why two?")], "");
        let got = pairs(&text);
        assert_eq!((got.len(), got[1].quote.as_str(), got[1].from.as_deref()), (2, "line one\nline two", Some("N-1.p4")));
        assert!(pairs("no quote here\n\nnor here").is_empty());
        // a note of two paragraphs stands whole
        let (text, _) = crate::ask::compose(&[comment("N-1.p2", "a", "first part\n\nsecond part")], "");
        assert_eq!(pairs(&text)[0].note, "first part\n\nsecond part");
    }

    #[test]
    fn the_answer_to_a_comment_stands_under_its_passage() {
        let (text, _) = crate::ask::compose(&[comment("N-1.p2", "the lock is held 7–12 s", "by whom?")], "");
        let v = view(serde_json::json!([
            {"n": 1, "at": "2026-11-18T10:00:00Z", "by": "owner", "text": "start"},
            {"n": 2, "at": "2026-11-18T10:01:00Z", "by": "lead", "text": "the lock is held 7–12 s"},
            {"n": 3, "at": "2026-11-18T10:02:00Z", "by": "owner", "text": text, "answers": 2, "told": ["lead"]},
            // the turn-end reply: no `answers`, by the agent his post told (consult-2, N-586.p17)
            {"n": 4, "at": "2026-11-18T10:03:00Z", "by": "lead", "text": "> by whom?\n\nthe follower's first round\n\nmore", "auto": true}
        ]));
        let got = sent(&v, 2, plain);
        assert_eq!(got, vec![Sent { quote: "the lock is held 7–12 s".into(), note: "by whom?".into(), his: 3, last: true, answer: Some((4, "the follower's first round\n\nmore".into())) }]);
        assert!(sent(&v, 4, plain).is_empty());
    }

    #[test]
    fn one_message_with_comments_on_two_posts_stands_at_both_and_the_answer_once_at_each() {
        let (text, _) = crate::ask::compose(&[comment("N-1.p1", "first", "why?"), comment("N-1.p2", "second", "and this?"), comment("N-1.p2", "third", "too?")], "");
        let v = view(serde_json::json!([
            {"n": 1, "at": "2026-11-18T10:00:00Z", "by": "lead", "text": "first"},
            {"n": 2, "at": "2026-11-18T10:01:00Z", "by": "lead", "text": "second and third"},
            {"n": 3, "at": "2026-11-18T10:02:00Z", "by": "owner", "text": text, "answers": 1, "told": ["lead"]},
            // the agent he told, posting something else first: the post that answers him by number wins (consult-2,
            // N-586.p21); and one not told is never the answer
            {"n": 4, "at": "2026-11-18T10:03:00Z", "by": "lead", "text": "handed · the box · W2"},
            {"n": 5, "at": "2026-11-18T10:03:30Z", "by": "other", "text": "not told"},
            {"n": 6, "at": "2026-11-18T10:04:00Z", "by": "lead", "text": "both answered", "answers": 3}
        ]));
        let at1 = sent(&v, 1, plain);
        assert_eq!(at1.iter().map(|c| (c.quote.as_str(), c.answer.as_ref().map(|a| a.0))).collect::<Vec<_>>(), vec![("first", Some(6))]);
        // two comments on post 2: the answer under the last only
        let at2 = sent(&v, 2, plain);
        assert_eq!(at2.iter().map(|c| (c.quote.as_str(), c.answer.as_ref().map(|a| a.0))).collect::<Vec<_>>(), vec![("second", None), ("third", Some(6))]);
    }

    /// A post that keeps its comments as data is read from them, its words whatever they are; one from before, from its
    /// words.
    #[test]
    fn a_post_with_comments_as_data_is_read_from_them() {
        let data = serde_json::json!([{"id": "c1", "author": "owner", "target": {"source": "N-1.p1", "quote": "first"}, "body": "why?"}]);
        let v = view(serde_json::json!([
            {"n": 1, "at": "2026-11-18T10:00:00Z", "by": "lead", "text": "first"},
            {"n": 2, "at": "2026-11-18T10:02:00Z", "by": "owner", "text": "worded some other way", "comments": data, "told": ["lead"]}
        ]));
        assert_eq!(sent(&v, 1, plain).iter().map(|c| (c.quote.as_str(), c.note.as_str(), c.his)).collect::<Vec<_>>(), vec![("first", "why?", 2)]);
    }

    /// [`by_post`] reads each of his posts once and gives, for every post, what [`sent`] gives for it alone (N-1793,
    /// item 1): two messages of his, one with comments on two posts, one answering a post with no "(from …)", a comment
    /// naming another thread, one of his taken back, on a thread of eight posts.
    #[test]
    fn the_comments_by_post_are_each_posts_sent_comments() {
        let (a, _) = crate::ask::compose(&[comment("N-1.p1", "first", "why?"), comment("N-1.p2", "second", "and this?"), comment("N-1.p2", "third", "too?"), comment("N-2.p1", "elsewhere", "not here")], "");
        let (b, _) = crate::ask::compose(&[comment("N-1.p2", "second", "again")], "");
        let (gone, _) = crate::ask::compose(&[comment("N-1.p1", "first", "taken back")], "");
        let v = view(serde_json::json!([
            {"n": 1, "at": "2026-11-18T10:00:00Z", "by": "lead", "text": "first"},
            {"n": 2, "at": "2026-11-18T10:01:00Z", "by": "lead", "text": "second and third"},
            {"n": 3, "at": "2026-11-18T10:02:00Z", "by": "owner", "text": a, "answers": 1, "told": ["lead"]},
            {"n": 4, "at": "2026-11-18T10:03:00Z", "by": "lead", "text": "both answered", "answers": 3},
            {"n": 5, "at": "2026-11-18T10:04:00Z", "by": "owner", "text": b, "told": ["lead"]},
            {"n": 6, "at": "2026-11-18T10:05:00Z", "by": "owner", "text": "> second and third\n\nplain answer", "answers": 2},
            {"n": 7, "at": "2026-11-18T10:06:00Z", "by": "owner", "text": gone, "taken_back": true},
            {"n": 8, "at": "2026-11-18T10:07:00Z", "by": "lead", "text": "the last", "told": ["owner"]}
        ]));
        let read = std::cell::Cell::new(0);
        let all = by_post(&v, |q| {
            read.set(read.get() + 1);
            plain(q)
        });
        for n in 1..=9 {
            assert_eq!(all.get(&n).cloned().unwrap_or_default(), sent(&v, n, plain), "post {n}");
        }
        assert!(all.get(&1).is_some_and(|c| c.len() == 1) && all.get(&2).is_some_and(|c| c.len() >= 3), "{all:?}");
        // his three posts read once each, and the answers to the two with comments once each
        assert_eq!(read.get(), 5, "each read once");
    }

    #[test]
    fn the_passage_is_found_whole_before_its_first_line() {
        let src = "the lock is held. Later: the lock is held 7–12 s";
        let at = place(src, "the lock is held 7–12 s", None).unwrap();
        assert_eq!(&src[at[0].start..at.last().unwrap().end], "the lock is held 7–12 s");
    }
}
