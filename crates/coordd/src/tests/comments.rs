//! His comments as data on the post his message made, resolved and reopened and read again at a start; and a brain's
//! suggestions: its pass's posts halted for his approval, until he lets one through or drops it.

use super::delivery::tests::{coord, load, open, t_of};
use crate::core::{Core, Who};
use docket_machine::{Anchor, Comment, Input, Text};
use serde_json::json;

fn on(quote: &str, body: &str) -> Comment {
    Comment { target: Anchor { source: "N-1.p1".into(), label: "N-1.p1, lead's message at 10:01".into(), quote: quote.into(), at: vec![[0, 4]] }, body: body.into(), ..Default::default() }
}

/// POST of T, posted as its writer BY with TEXT naming NAMES: its number.
fn post(core: &mut Core, t: u64, by: &str, text: &str, names: &[&str]) -> u32 {
    core.tickets.posted = None;
    core.ticket_input(Input::Post { t, by: by.into(), text: Text::New(text.into()), names: names.iter().map(|n| n.to_string()).collect(), told: vec![], inform: vec![], answers: None, kind: None }).unwrap();
    core.tickets.m.tickets[&t].last()
}

/// His two comments on the post his message made: each its own id, his, timed, on its passage; only he or the post's
/// writer resolves one, and only one there is; a start replays them as they stood.
#[test]
fn his_comments_are_kept_on_his_post_resolved_by_him_and_read_again_at_a_start() {
    let (dir, base, mut core, _rx) = coord("comments", "idle");
    let t = t_of(&open(&mut core, "lead", &["w2"], "the lock is held 7 s"));
    let two = vec![on("the lock", "by whom?"), on("7 s", "measured how?")];
    let n = post(&mut core, t, "owner", &docket_machine::comment::render(&two, "and the tests"), &["lead"]);
    core.ticket_comments(two).unwrap();
    let kept = |core: &Core| core.tickets.m.tickets[&t].posts.iter().find(|p| p.n == n).unwrap().comments.clone();
    let c = kept(&core);
    assert_eq!(c.iter().map(|c| (c.id.as_str(), c.author.as_str(), c.body.as_str(), c.resolved)).collect::<Vec<_>>(), [("c1", "owner", "by whom?", false), ("c2", "owner", "measured how?", false)]);
    assert!(c.iter().all(|c| !c.at.is_empty() && c.target.at == [[0, 4]]), "{c:?}");
    let id = core.tickets.m.id(t);
    assert!(!core.ticket_comment_resolve(&Who::Agent("w3".into()), &id, n, "c1", true).ok, "neither he nor the post's writer");
    assert!(!core.ticket_comment_resolve(&Who::Owner, &id, n, "c9", true).ok, "no such comment");
    assert!(core.ticket_comment_resolve(&Who::Owner, &id, n, "c1", true).ok);
    assert_eq!(kept(&core).iter().map(|c| c.resolved).collect::<Vec<_>>(), [true, false]);
    assert!(core.ticket_comment_resolve(&Who::Owner, &id, n, "c1", false).ok && !kept(&core)[0].resolved, "reopened");
    assert!(core.ticket_comment_resolve(&Who::Owner, &id, n, "c2", true).ok);
    let (again, _rx2) = load(&dir, &base, "idle");
    assert_eq!(kept(&again), kept(&core), "replayed as they stood");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A pass of w2's brain whose ticket is slow: each of its posts that would wake w2 halts, a suggestion, oldest first
/// with its first line; his Approve lets the first through and his refusal drops the second, in the record, and none
/// is left.
#[test]
fn a_brain_pass_post_halted_for_him_is_a_suggestion_until_he_lets_it_through_or_drops_it() {
    let (dir, _, mut core, _rx) = coord("brainready", "idle");
    let his = open(&mut core, "owner", &["w2"], "the brain");
    let form = json!({"agent": "w2", "pass": {"head": "Attend", "posts": [his]}});
    let r = core.ticket_input(Input::Open { kind: "brain".into(), under: Some(t_of(&his)), opener: "coordd".into(), form, text: None, names: vec![], told: vec![] }).unwrap();
    let t = docket_machine::parse_ticket(r.trim_start_matches("Opened ").split(['.', ';']).next().unwrap()).unwrap();
    core.ticket_input(Input::Slow { t, on: true, by: "owner".into() }).unwrap();
    assert!(core.brain_ready("w2").is_empty());
    let a = post(&mut core, t, "w3", "answer his post first\nbecause he asked", &["w2"]);
    let b = post(&mut core, t, "w3", "then the build", &["w2"]);
    let ready = core.brain_ready("w2");
    assert_eq!(ready.iter().map(|s| (s.ticket, s.post, s.words.as_str())).collect::<Vec<_>>(), [(t, a, "answer his post first"), (t, b, "then the build")]);
    assert!(core.brain_ready("w3").is_empty(), "w2's brain's, not w3's");
    let id = core.tickets.m.id(t);
    assert!(core.ticket_approve(&Who::Owner, &id, ready[0].halt).ok);
    assert_eq!(core.brain_ready("w2").iter().map(|s| s.post).collect::<Vec<_>>(), [b]);
    assert!(core.ticket_halt_reject(&Who::Owner, t, ready[1].halt, Some("not now".into())).ok);
    assert!(core.brain_ready("w2").is_empty());
    let x = &core.tickets.m.tickets[&t];
    let p = |n: u32| x.posts.iter().find(|p| p.n == n).unwrap();
    assert!(!p(a).halted && !p(a).dropped && p(b).dropped, "the first let through, the second dropped");
    let _ = std::fs::remove_dir_all(&dir);
}
