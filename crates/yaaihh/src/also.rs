//! Where else a post of his stands (the owner, N-354.p1, N-355.p1: a message that fits two threads goes on both, and
//! nobody asks him; paper-research N-586.p14). coordd posts the same words on each thread (docket-core5's
//! `Post.on`, N-725, from `POSTED_ON_PROTOCOL`, E4) and each post says faintly where else it stands ("also on N-12 ·
//! the lexer"), a click going there. `Post.on` is read through serde, so this builds against a docket-machine without
//! it and draws nothing then.

use crate::{App, Proj};
use docket_machine::Post;
use rune::{El, row, text};
use rune_kit::Kit;

/// The other posts POST stands as (`N-12.p4`), from coordd's view (`Post.on`, read through serde).
pub fn others(post: &Post) -> Vec<String> {
    serde_json::to_value(post).ok().and_then(|v| v.get("on").cloned()).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

/// Under POST: faintly, the other threads it stands on, each a click to its post there.
pub fn on_line(p: &Proj, post: &Post, k: &Kit) -> Option<El> {
    let others = others(post);
    if others.is_empty() {
        return None;
    }
    let mut r = row().gap(6.0).center_y().flex_wrap().child(text("also on").font(k.ty.small).color(k.c.text_faint));
    for id in others {
        let t = id.split_once(".p").map_or(id.as_str(), |(t, _)| t);
        let label = p.live.tickets.values().find(|v| v.id == t).map(|v| crate::outline::label(p, v)).unwrap_or_default();
        let shown = if label.is_empty() { t.to_string() } else { format!("{t} · {label}") };
        r = r.child(k.interactive(text(shown).font(k.ty.small).color(k.c.text_faint)).help("Go to it there").on_press(move |s: &mut App| {
            crate::mentions::open(s, &id);
        }));
    }
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_on_two_threads_names_the_other() {
        let p: Post = serde_json::from_value(serde_json::json!({"n": 3, "at": "2026-11-18T22:00:00Z", "by": "owner", "text": "both", "on": ["N-12.p4"]})).unwrap();
        // docket-machine's Post carries `on` since posted-on (E4, POSTED_ON_PROTOCOL 19)
        assert_eq!(others(&p), ["N-12.p4"]);
        let q: Post = serde_json::from_value(serde_json::json!({"n": 3, "at": "2026-11-18T22:00:00Z", "by": "owner", "text": "one"})).unwrap();
        assert!(others(&q).is_empty());
    }
}
