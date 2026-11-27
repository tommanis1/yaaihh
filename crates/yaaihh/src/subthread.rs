//! A post's actions (paper-research N-466 rule 3): `Reply · Subthread · ⋯` at the right edge of the post under the
//! pointer, and nowhere else. Reply and Subthread only set the target of the one message box (docket-edge2's
//! message_box.rs, agreed on N-449 posts 5 to 8): Reply answers at that post, Subthread opens a thread tied at that
//! post from its first line (docket-core4's form, N-412 post 161, N-447 post 35). Neither opens a box of its own.
//! `⋯` holds what the post's head no longer says: star, fold, the agent's session, and the engine's words about it
//! (held, to whom, moved).

use crate::message_box::{Target, Way};
use crate::App;
use docket_machine::TicketView;
use rune::{El, col, row, text};
use rune_kit::Kit;

/// Whom a subthread at post BY of thread V goes to: the post's agent, else whoever has the thread.
pub fn to_of(v: &TicketView, by: &str) -> Option<String> {
    if by != "owner" && by != "minder" && by != "clerk" && !by.is_empty() {
        return Some(by.to_string());
    }
    crate::outline::holder(v)
}

/// Whether project PI's coordd keeps a subthread's tie (`under`, from ASKS_CLOSED_PROTOCOL; an older one would open a
/// top thread silently); a scene's `YAAIHH_SUB` shows it anyway.
pub fn can_sub(s: &App, pi: usize) -> bool {
    s.projects.get(pi).is_some_and(|p| p.protocol >= coord_types::ASKS_CLOSED_PROTOCOL) || std::env::var_os("YAAIHH_SUB").is_some()
}

/// Whether post PN of V shows its actions: under the pointer, its `⋯` open, or the box aimed at it.
pub(crate) fn shown(s: &App, v: &TicketView, pn: u32) -> bool {
    let at = Some((v.n, pn));
    if s.tickets.hover_post == at || s.tickets.post_more == at {
        return true;
    }
    let default = Target::Thread { project: s.cur, t: v.id.clone(), post: None, way: Way::Here };
    matches!(crate::message_box::chosen(s, default), Target::Thread { post: Some(n), .. } if n == pn)
}

/// `Reply · Subthread · ⋯` of post PN (by BY) of V, at the right edge of its head; empty while it is not shown. The
/// one the box is aimed at reads in the text colour, the others dim.
pub fn actions(s: &App, v: &TicketView, pn: u32, by: &str, k: &Kit) -> Option<El> {
    if !shown(s, v, pn) {
        return None;
    }
    let pi = s.cur;
    let default = Target::Thread { project: pi, t: v.id.clone(), post: None, way: Way::Here };
    let now = crate::message_box::chosen(s, default);
    let word = |label: &str, way: Way| {
        let target = Target::Thread { project: pi, t: v.id.clone(), post: Some(pn), way };
        let on = now == target;
        let t = target.clone();
        k.interactive(row().key(format!("act-{label}-{}-{pn}", v.id)).child(text(label.to_string()).font(k.ty.small).color(if on { k.c.text_base } else { k.c.text_muted })))
            .on_press(move |s: &mut App| crate::message_box::choose(s, t.clone()))
    };
    let mut r = row().key(format!("acts-{}-{pn}", v.id)).gap(10.0).center_y().child(word("Reply", Way::Here));
    if can_sub(s, pi) && to_of(v, by).is_some() {
        r = r.child(word("Subthread", Way::Subthread));
    }
    let at = (v.n, pn);
    let more = k.interactive(row().key(format!("more-{}-{pn}", v.id)).child(text("⋯").font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| {
        s.tickets.post_more = if s.tickets.post_more == Some(at) { None } else { Some(at) };
    });
    // actions, not the post's text: the window's page (lib.rs `view`) leaves their words out of a selection
    Some(r.child(more).select_texts(false))
}

/// The open `⋯` of post PN of V: star, fold, the agent's session, and NOTES (the engine's words about the post).
pub fn more(s: &App, v: &TicketView, pn: u32, by: &str, notes: &str, k: &Kit) -> Option<El> {
    if s.tickets.post_more != Some((v.n, pn)) {
        return None;
    }
    let (vn, at) = (v.n, (v.n, pn));
    let folded = s.tickets.folded.contains(&at);
    let fold = k.interactive(row().child(text(if folded { "unfold" } else { "fold" }).font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| {
        if !s.tickets.folded.remove(&(vn, pn)) {
            s.tickets.folded.insert((vn, pn));
        }
        s.tickets.post_more = None;
    });
    // its actions out of the window's page (lib.rs `view`), as `actions`' are; the notes under them are text
    let mut r = row().gap(12.0).center_y().select_texts(false).child(crate::find::star(v, Some(pn), k)).child(fold);
    if by != "owner" && by != "minder" && by != "clerk" {
        let b = by.to_string();
        r = r.child(k.interactive(row().child(text(format!("{by}'s session")).font(k.ty.small).color(k.c.text_muted))).on_press(move |s: &mut App| s.sel = crate::Sel::Agent(b.clone())));
    }
    let mut c = col().key(format!("more-open-{}-{pn}", v.id)).gap(2.0).child(r);
    let notes = notes.trim_start_matches(" · ");
    if !notes.is_empty() {
        c = c.child(text(notes.replace(" · ", "; ")).font(k.ty.small).color(k.c.text_faint).wrap());
    }
    Some(c)
}

/// What the post's head says of it: who, and when.
pub fn head(who: &str, when: &str, k: &Kit) -> El {
    row().gap(8.0).center_y().child(text(who.to_string()).font(k.ty.small_medium).color(k.c.text_base)).child(text(when.to_string()).font(k.ty.small).color(k.c.text_muted))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_subthread_goes_to_the_posts_agent_else_the_threads() {
        let v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-234", "n": 234, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "paper-research"}, "title": "t", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-17T08:00:00Z", "given": [], "posts": [], "waits_on": []})).unwrap();
        assert_eq!(super::to_of(&v, "docket-core4").as_deref(), Some("docket-core4"));
        assert_eq!(super::to_of(&v, "owner").as_deref(), Some("paper-research"));
    }

    /// The box aimed at a post's Subthread sends one `say` tied under that post, to its agent, and only to a coordd at
    /// ASKS_CLOSED_PROTOCOL; an older one is told why and gets nothing.
    #[test]
    fn the_boxs_subthread_says_under_the_post_from_protocol_9_only() {
        use crate::message_box::{Target, Way};
        let mut s = crate::init();
        let mut p = crate::Proj::new(&crate::projects::ProjectConfig { name: "t".into(), id: "t".into(), boxed: false }, Vec::new());
        let v: docket_machine::TicketView = serde_json::from_value(serde_json::json!({"id": "N-234", "n": 234, "kind": "thread", "opener": "owner", "ctx": {},
            "form": {"to": "paper-research"}, "title": "t", "status": {"is": "resting"}, "children": [], "program": "", "bound": 3, "spent": 1,
            "opened_at": "2026-11-17T08:00:00Z", "given": [], "waits_on": [],
            "posts": [{"n": 1, "by": "owner", "at": "2026-11-17T08:00:00Z", "text": "a"}, {"n": 2, "by": "docket-core4", "at": "2026-11-17T08:01:00Z", "text": "b"}]})).unwrap();
        p.live.tickets.insert(234, v);
        s.projects = vec![p];
        s.cur = 0;
        let target = Target::Thread { project: 0, t: "N-234".into(), post: Some(2), way: Way::Subthread };
        s.composers.insert((0, target.place()), rune::TextEdit::new("look at this"));
        s.projects[0].protocol = coord_types::ASKS_CLOSED_PROTOCOL - 1;
        assert!(crate::message_box::request(&s, &target).is_err());
        s.projects[0].protocol = coord_types::ASKS_CLOSED_PROTOCOL;
        let v = serde_json::to_value(crate::message_box::request(&s, &target).unwrap()).unwrap();
        assert_eq!((v["op"].as_str(), v["to"].as_str(), v["under"].as_str(), v["text"].as_str()), (Some("say"), Some("docket-core4"), Some("N-234.p2"), Some("look at this")));
    }
}
