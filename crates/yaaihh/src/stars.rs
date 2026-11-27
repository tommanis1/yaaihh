//! The stars an agent puts on a post for the owner to look at (the owner, N-361: "start stuff for me to look at ...
//! clicked once then diapepsrs"; paper-research N-101.p1284). The left menu's Starred lists them, newest first, with a
//! count; a click opens the post in its place and the entry leaves the list. Until then the post carries a small star
//! where it stands in his stream.
//!
//! His click is the unstar of the post (`Request::Star { t: "N-347.p3", on: false }`): coordd ends every star on it,
//! whoever set it (docket-core d0eecd6), so the list is `TicketView.stars` as coordd holds them.

use crate::{App, Proj};
use coord_types::{FindHit, Request, TicketView};

/// The posts of V the agents starred, which his opening has not ended, one each: (post, who starred it, the newest
/// star's time).
fn on(_project: &str, v: &TicketView) -> Vec<(u32, String, String)> {
    let mut out: Vec<(u32, String, String)> = Vec::new();
    for x in v.stars.iter().filter(|x| x.by != "owner") {
        let Some(n) = x.post else { continue };
        let at = x.at.to_string();
        match out.iter_mut().find(|o| o.0 == n) {
            Some(o) => {
                o.1 = format!("{}, {}", o.1, x.by);
                o.2 = o.2.clone().max(at);
            }
            None => out.push((n, x.by.clone(), at)),
        }
    }
    out
}

/// The stars for the owner in project P, newest first, each as a hit (its thread's id and label, the post's first
/// words, who starred it and when).
pub fn for_owner(s: &App, p: &Proj) -> Vec<FindHit> {
    let mut out: Vec<FindHit> = Vec::new();
    for v in p.live.tickets.values().filter(|v| !v.archived) {
        for (n, by, at) in on(&p.id, v) {
            let Some(x) = v.posts.iter().find(|x| x.n == n) else { continue };
            let words = crate::texts::words(&s.tickets, &serde_json::to_value(&x.text).unwrap_or(serde_json::Value::Null));
            let first = words.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").chars().take(160).collect::<String>();
            let label = if v.title.trim().is_empty() { crate::refs::ticket(Some(p), v).label } else { v.title.clone() };
            out.push(FindHit { id: format!("{}.p{n}", v.id), t: v.n, post: Some(n), title: label, by: x.by.clone(), at: x.at.to_string(), words: first, star: Some((by, at)), archived: v.archived || v.archived_under.is_some() });
        }
    }
    out.sort_by(|a, b| b.star.as_ref().map(|x| &x.1).cmp(&a.star.as_ref().map(|x| &x.1)));
    out
}

/// The threads of project P the owner starred himself, newest star first, each as a hit on the thread itself. They
/// stay in the list until he takes the star off: opening one does not end it.
pub fn own(p: &Proj, asked: &[FindHit]) -> Vec<FindHit> {
    let mut out: Vec<FindHit> = p.live.tickets.values().filter(|v| !v.archived).filter_map(|v| own_star(v).map(|at| (v, at))).map(|(v, at)| {
        let label = if v.title.trim().is_empty() { crate::refs::ticket(Some(p), v).label } else { v.title.clone() };
        FindHit { id: v.id.clone(), t: v.n, post: None, title: label, by: v.opener.clone().unwrap_or_default(), at: v.opened_at.to_string(), words: String::new(), star: Some(("owner".into(), at)), archived: v.archived || v.archived_under.is_some() }
    }).collect();
    // a thread that closed more than a day ago leaves the snapshot but keeps its star: coordd's own list has it (the
    // live view wins where it holds the thread, so a star taken off since is not listed)
    out.extend(asked.iter().filter(|h| !p.live.tickets.contains_key(&h.t)).cloned());
    out.sort_by(|a, b| b.star.as_ref().map(|x| &x.1).cmp(&a.star.as_ref().map(|x| &x.1)));
    out
}

/// Coordd's starred list (`Request::Find { starred }`) kept to the owner's stars on threads themselves.
pub fn ask_own(p: &Proj) -> Vec<FindHit> {
    own_of(&p.ask_once(&coord_types::Request::Find { q: String::new(), starred: true, wide: false }).unwrap_or_default())
}

/// His stars on threads in coordd's starred list OUT (a hit a line).
pub fn own_of(out: &str) -> Vec<FindHit> {
    out.lines().filter_map(|l| serde_json::from_str::<FindHit>(l).ok()).filter(|h| h.post.is_none() && h.star.as_ref().is_some_and(|(by, _)| by == "owner")).collect()
}

/// When the owner starred thread V itself (empty when coordd kept it without a time), None when he has not.
fn own_star(v: &TicketView) -> Option<String> {
    let at = v.stars.iter().filter(|x| x.by == "owner" && x.post.is_none()).map(|x| x.at.to_string()).max();
    at.or_else(|| v.starred.then(String::new))
}

/// Whether the sidebar's Starred row stands: an agent's star waits for him, or he starred a thread (ASKED: coordd's list).
pub fn any(p: &Proj, asked: &[FindHit]) -> bool {
    count(p) > 0 || !own(p, asked).is_empty()
}

/// How many starred posts wait for the owner in project P.
pub fn count(p: &Proj) -> usize {
    p.live.tickets.values().filter(|v| !v.archived).map(|v| on(&p.id, v).len()).sum()
}

/// Who starred post N of V for the owner and he has not opened it: the small star's words, none when nobody.
pub fn mark(p: &Proj, v: &TicketView, n: u32) -> Option<String> {
    let author = v.posts.iter().find(|x| x.n == n).map(|x| x.by.as_str()).unwrap_or("");
    // its author is named above it: the star names whoever else starred it
    on(&p.id, v).into_iter().find(|x| x.0 == n).map(|x| if x.1 == author { "★".to_string() } else { format!("★ {}", x.1) })
}

/// Open hit H of project PI: the unstar of its post ends its stars (coordd), and its thread opens at the post.
pub fn open_hit(s: &mut App, pi: usize, h: &FindHit) {
    s.request_in(pi, Request::Star { t: h.id.clone(), on: false }, crate::Pending::TicketDone { clear: crate::threads::Cleared::Quiet });
    crate::find::open_at(s, h.t, &h.id, h.post);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn his_own_star_on_a_thread_is_listed_in_its_own_right() {
        let v: TicketView = serde_json::from_value(json!({"id": "N-9", "n": 9, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-16T21:00:00Z", "given": [], "posts": [],
            "stars": [{"by": "owner", "at": "2026-11-16T21:13:00Z"}, {"post": 3, "by": "owner", "at": "2026-11-16T21:15:00Z"}]}))
        .unwrap();
        assert_eq!(own_star(&v).as_deref(), Some("2026-11-16T21:13:00Z"), "the thread's own star, not a post's");
        let plain = TicketView { stars: Vec::new(), ..v };
        assert_eq!(own_star(&plain), None);
        assert_eq!(own_star(&TicketView { starred: true, ..plain }).as_deref(), Some(""), "a star kept before times counts");
    }

    #[test]
    fn an_agents_star_on_a_post_is_listed_and_his_own_is_not() {
        let v: TicketView = serde_json::from_value(json!({"id": "N-347", "n": 347, "kind": "thread", "opener": "owner", "ctx": {}, "form": {}, "title": "t",
            "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1, "opened_at": "2026-11-16T21:00:00Z", "given": [],
            "posts": [{"n": 3, "at": "2026-11-16T21:10:00Z", "by": "paper-research", "text": "milestone"}],
            "stars": [{"post": 3, "by": "paper-research", "at": "2026-11-16T21:11:00Z"}, {"post": 3, "by": "owner", "at": "2026-11-16T21:12:00Z"}, {"by": "paper-research", "at": "2026-11-16T21:13:00Z"},
                {"post": 3, "by": "yaaihh", "at": "2026-11-16T21:14:00Z"}]}))
        .unwrap();
        let s = on("stars-test", &v);
        assert_eq!(s, vec![(3, "paper-research, yaaihh".to_string(), "2026-11-16T21:14:00Z".to_string())], "one entry per post; his own star and a star on the thread itself are not for him to look at");
    }

    #[test]
    fn his_own_star_stays_listed_after_the_thread_leaves_the_snapshot() {
        let p = crate::Proj::new(&crate::projects::ProjectConfig { name: "research".into(), id: "research".into(), boxed: false }, Vec::new());
        assert!(own(&p, &[]).is_empty() && !any(&p, &[]));
        let h = FindHit { id: "N-122".into(), t: 122, post: None, title: "Hacker News scan".into(), by: "owner".into(), at: String::new(), words: String::new(), star: Some(("owner".into(), "2026-11-19T11:00:00Z".into())), archived: false };
        assert_eq!(own(&p, std::slice::from_ref(&h)), vec![h.clone()], "coordd's list keeps it");
        assert!(any(&p, &[h]), "so the Starred row stands, with no number");
        assert_eq!(count(&p), 0);
    }

    #[test]
    fn his_stars_are_read_from_coordds_list_and_an_agents_are_left() {
        let line = |id: &str, post: Option<u32>, by: &str| serde_json::to_string(&FindHit { id: id.into(), t: 1, post, by: by.into(), star: Some((by.into(), "2026-11-19T10:00:00Z".into())), ..Default::default() }).unwrap();
        let out = [line("N-1", None, "owner"), line("N-2.p3", Some(3), "owner"), line("N-3", None, "paper-research")].join("\n");
        assert_eq!(own_of(&out).iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["N-1"], "his star on a thread itself only");
    }
}
