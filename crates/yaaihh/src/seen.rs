//! What is new since he last looked (the owner N-284.p10, paper-research N-471.p1). "Seen" is the window's own
//! knowledge, kept in its memory only: nothing is read from or written to a file, so the Windows window keeps it the
//! same way. A thread is seen up to its last post when he leaves it; a thread this window has not met yet is seen as it
//! stands when it is first met, so a window that starts shows nothing as new. Two windows each keep their own.

use crate::App;
use docket_machine::{Post, TicketView};
use rune::{El, row, text};
use rune_kit::Kit;
use std::collections::HashMap;

#[derive(Default)]
pub struct Seen {
    /// By (project, ticket id): the last post he has seen of it.
    pub upto: HashMap<(usize, String), u32>,
    /// The thread shown when this was last marked: left, it is seen whole.
    pub shown: Option<(usize, String)>,
}

fn last(v: &TicketView) -> u32 {
    v.posts.iter().map(|q| q.n).max().unwrap_or(0)
}

/// After each poll: the thread he left is seen whole; a thread first met is seen as it stands.
pub fn mark(s: &mut App) {
    let pi = s.cur;
    let now = s.tickets.shown.and_then(|n| s.projects.get(pi)?.live.tickets.get(&n)).map(|v| (pi, v.id.clone()));
    if s.tickets.seen.shown != now {
        if let Some((qi, id)) = s.tickets.seen.shown.take() {
            if let Some(v) = s.projects.get(qi).and_then(|p| p.live.tickets.values().find(|v| v.id == id)) {
                let n = last(v);
                s.tickets.seen.upto.insert((qi, id), n);
            }
        }
        s.tickets.seen.shown = now;
    }
    for (qi, p) in s.projects.iter().enumerate() {
        for v in p.live.tickets.values() {
            s.tickets.seen.upto.entry((qi, v.id.clone())).or_insert_with(|| last(v));
        }
    }
}

/// An agent's post of V he has not seen.
fn new_posts<'a>(s: &App, pi: usize, v: &'a TicketView) -> impl Iterator<Item = &'a Post> + 'a {
    let upto = s.tickets.seen.upto.get(&(pi, v.id.clone())).copied().unwrap_or(u32::MAX);
    v.posts.iter().filter(move |q| q.n > upto && q.by != "owner" && q.by != "coord" && q.visible() && q.moved.is_none())
}

/// Whether V holds a post he has not seen (not the thread on screen: he is reading it). Only on a thread of his: the
/// agents' talk with each other alone ([`crate::outline::apart`]) is new to nobody he waits for (yaaihh/fresh's Go,
/// N-1751 p2).
pub fn is_new(s: &App, pi: usize, v: &TicketView) -> bool {
    let shown = s.tickets.seen.shown.as_ref().is_some_and(|(qi, id)| *qi == pi && *id == v.id);
    !shown && !crate::outline::apart(v, &[]) && new_posts(s, pi, v).next().is_some()
}

/// At the foot of thread V: "↑ 1 new answer, under your message of 12:45" when an answer he has not seen came under
/// an earlier message of his, above what the thread's foot shows; a click goes there, and it is seen.
pub fn above(s: &App, pi: usize, v: &TicketView, k: &Kit) -> Option<El> {
    let lastn = last(v);
    let ups: Vec<(&Post, &Post)> = new_posts(s, pi, v)
        .filter_map(|q| {
            let a = v.posts.iter().find(|m| Some(m.n) == q.answers && m.by == "owner")?;
            // above the foot: something else came after the message it answers
            v.posts.iter().any(|x| x.n > a.n && x.n != q.n && x.answers != Some(a.n)).then_some((q, a))
        })
        .collect();
    let (q, a) = *ups.first()?;
    let n = ups.len();
    let words = format!("↑ {n} new answer{} to your {}", if n == 1 { "" } else { "s" }, crate::when::at(&a.at.to_string()));
    let (vn, id, qn) = (v.n, v.id.clone(), q.n);
    Some(k.interactive(row().key(format!("seen-above-{}", v.id)).w_full().justify_center().py(4.0).child(text(words).font(k.ty.small_medium).color(k.c.text_base))).on_press(move |s: &mut App| {
        crate::find::open_at(s, vn, &format!("{id}.p{qn}"), Some(qn));
        s.tickets.seen.upto.insert((s.cur, id.clone()), lastn);
    }))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_post_after_what_was_seen_is_new_and_his_own_is_not() {
        let mut s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-5", "n": 5, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "a"}, "title": "t", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-18T08:00:00Z", "given": [], "waits_on": [],
            "posts": [{"n": 1, "by": "owner", "at": "2026-11-18T08:00:00Z", "text": "a"}]})).unwrap();
        p.live.tickets.insert(5, v.clone());
        s.projects = vec![p];
        s.cur = 0;
        super::mark(&mut s);
        assert!(!super::is_new(&s, 0, &s.projects[0].live.tickets[&5]));
        v.posts.push(serde_json::from_value(serde_json::json!({"n": 2, "by": "owner", "at": "2026-11-18T08:01:00Z", "text": "b"})).unwrap());
        v.posts.push(serde_json::from_value(serde_json::json!({"n": 3, "by": "a", "at": "2026-11-18T08:02:00Z", "text": "c", "answers": 1})).unwrap());
        s.projects[0].live.tickets.insert(5, v);
        assert!(super::is_new(&s, 0, &s.projects[0].live.tickets[&5]));
        // shown, his answer under post 1 stands above post 2: the line says so; left, it is seen
        s.tickets.shown = Some(5);
        super::mark(&mut s);
        assert!(!super::is_new(&s, 0, &s.projects[0].live.tickets[&5]));
        assert!(super::above(&s, 0, &s.projects[0].live.tickets[&5], &rune_kit::Kit::neutral(true)).is_some());
        s.tickets.shown = None;
        super::mark(&mut s);
        s.tickets.shown = Some(5);
        super::mark(&mut s);
        assert!(super::above(&s, 0, &s.projects[0].live.tickets[&5], &rune_kit::Kit::neutral(true)).is_none());
    }

    /// The unseen dot takes only his threads (yaaihh/fresh's Go, N-1751 p2): agents' talk with each other alone is not
    /// new to him; once a post there names him, it is his and what he has not seen is new.
    #[test]
    fn talk_between_agents_alone_is_not_new_to_him() {
        let mut s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let mut v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-6", "n": 6, "kind": "thread", "opener": "a", "ctx": {},
            "form": {"to": "b"}, "title": "t", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-18T08:00:00Z", "given": [], "waits_on": [],
            "posts": [{"n": 1, "by": "a", "at": "2026-11-18T08:00:00Z", "text": "@b build it", "names": ["b"], "told": ["b", "owner"]}]})).unwrap();
        p.live.tickets.insert(6, v.clone());
        s.projects = vec![p];
        s.cur = 0;
        super::mark(&mut s);
        v.posts.push(serde_json::from_value(serde_json::json!({"n": 2, "by": "b", "at": "2026-11-18T08:01:00Z", "text": "@a built", "names": ["a"], "told": ["a", "owner"]})).unwrap());
        s.projects[0].live.tickets.insert(6, v.clone());
        assert!(!super::is_new(&s, 0, &s.projects[0].live.tickets[&6]), "between a and b, though the norms told him");
        v.posts.push(serde_json::from_value(serde_json::json!({"n": 3, "by": "a", "at": "2026-11-18T08:02:00Z", "text": "@owner it is built", "names": ["owner"]})).unwrap());
        s.projects[0].live.tickets.insert(6, v);
        assert!(super::is_new(&s, 0, &s.projects[0].live.tickets[&6]), "a post names him: his thread");
    }
}

/// Whom V's posts name (or policy told) that have not yet taken them: no given line to them reaches the post
/// (`Given.upto`, the post number a delivery shows up to), its author left out. In the order first named.
/// The owner is never among them: the owner is handed nothing and reads a ticket here.
pub fn unread_by(v: &TicketView) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // a closed ticket hands nothing more (a note a post was moved into closes at once): nobody waits to read it
    if matches!(v.status, docket_machine::ViewStatus::Done { .. } | docket_machine::ViewStatus::Failed { .. }) {
        return out;
    }
    // what each name has taken is read once, not once a post: N-101's 1,880 posts each walked its thousands of given
    // lines again for each name (docket-edge2, N-422: the Threads page, view_cost.rs)
    let mut upto: HashMap<&str, u32> = HashMap::new();
    for p in &v.posts {
        for n in p.names.iter().chain(p.told.iter()) {
            if *n != p.by && n != "owner" && !out.contains(n) && *upto.entry(n.as_str()).or_insert_with(|| read_upto(v, n)) < p.n {
                out.push(n.clone());
            }
        }
    }
    out
}

/// The last post of V that NAME has taken: what its given lines reach (`Given.upto`). A thread's words (`.wN`, a
/// one-off given before it was kept) are its post #1, so whoever was given them has read #1.
pub fn read_upto(v: &TicketView, name: &str) -> u32 {
    let upto = v.given.iter().filter(|g| g.to == name).map(|g| g.upto).max().unwrap_or(0);
    let words = v.kind == "thread" && v.given.iter().any(|g| g.to == name && g.delivery.contains(".w"));
    upto.max(u32::from(words))
}

/// "not yet sent to X, Y" for V, None when everyone named has been handed it (its given lines; 0.2 B7: never "read").
pub fn unread_line(v: &TicketView) -> Option<String> {
    let u = unread_by(v);
    (!u.is_empty()).then(|| format!("not yet sent to {}", u.join(", ")))
}

#[cfg(test)]
mod moved_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn a_kept_one_off_is_read_by_whom_its_words_went_to_and_the_owner_is_never_unread() {
        // coord keep N-4: the words lead was given (N-4.w1) are post #1, lead's answer (telling the owner) post #2
        let v: TicketView = serde_json::from_value(json!({"id": "N-4", "n": 4, "kind": "thread", "opener": "owner", "ctx": {}, "form": {"to": "lead"}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 1, "spent": 0, "opened_at": "2026-11-11T19:16:56Z", "waits_on": [],
            "given": [{"to": "lead", "delivery": "N-4.w1", "upto": 0, "at": "2026-11-11T19:16:57Z"}],
            "posts": [{"n": 1, "at": "2026-11-11T19:17:09Z", "by": "owner", "text": "q", "names": ["lead"]},
                      {"n": 2, "at": "2026-11-11T19:17:09Z", "by": "lead", "text": "a", "told": ["owner"]}]}))
        .unwrap();
        assert_eq!(read_upto(&v, "lead"), 1);
        assert_eq!(unread_line(&v), None);
        // closed, nothing more is handed: nobody is unread
        let mut c = v.clone();
        c.posts.push(serde_json::from_value(json!({"n": 3, "at": "2026-11-11T19:20:00Z", "by": "owner", "text": "moved here", "names": ["lead"], "from": "N-2"})).unwrap());
        c.status = docket_machine::ViewStatus::Done { result: json!([]) };
        assert_eq!(unread_line(&c), None);
        // a third post naming lead is unread by lead until a given line reaches it
        let mut w = v.clone();
        w.posts.push(serde_json::from_value(json!({"n": 3, "at": "2026-11-11T19:20:00Z", "by": "owner", "text": "more", "names": ["lead"]})).unwrap());
        assert_eq!(unread_line(&w).as_deref(), Some("not yet sent to lead"));
    }

}
