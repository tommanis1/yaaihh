//! The owner's steps that go on by themselves (N-487): a step owed to the owner whose call names a `default` takes
//! that answer when the policy's `lapse-after` for the call has passed since it was owed (`time`). coordd reads the
//! machine's kept set of such tickets each second (`Machine::lapsing`), never every ticket; the policy's times are
//! read once and again only when a policy file changed. It goes on at once (`closed`) when the window that opened the
//! ticket is gone: the form's `window` names a window start (`coordc bridge --box ID`), and coordd counts the watch
//! connections up for each ([`Boxes`]). The machine reads no clock: what lapsed is the record's `lapsed` line, which
//! replay applies as recorded.

use crate::core::Core;
use docket_machine::{Input, Status};
use serde_json::json;
use std::collections::BTreeMap;

/// The owner's window as a worker (`step box judge(...)`, N-487): the box that sent his words answers the steps owed
/// to it, as the runner answers its own; nothing is delivered to it and nothing is started for it.
pub const BOX: &str = "box";

/// The form field naming the window start that opened a ticket (the words kind's `window`, N-487 p28; its value is
/// the bridge's `--box`). Not `box`: that is the worker of `step box judge`, and a form field of the name would
/// stand for it.
pub const FORM_BOX: &str = "window";

/// How long a box may have no watch connection up before it counts as closed: the window's pause before starting a
/// dropped bridge again, doubled once as a pipe that fails at once is (3 s, then 6 s), and the start of each, so a
/// window whose pipe dropped is back with the same box (`--from`) before its step goes on. The window's own constant,
/// not a second number (paper-research N-487 p30).
pub const BOX_GONE_AFTER: std::time::Duration = std::time::Duration::from_secs(coord_types::PIPE_PAUSE_MIN.as_secs() * 5);

/// A box gone this long is forgotten; a ticket whose box coordd does not know goes on by time only.
const BOX_FORGOTTEN_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

/// The watch connections up for each box (`Request::Watch`'s `box`), and since when a box has none.
#[derive(Default)]
pub struct Boxes {
    by_id: BTreeMap<String, (u32, Option<std::time::Instant>)>,
}

impl Boxes {
    /// A watch with box ID began.
    pub fn up(&mut self, id: &str, now: std::time::Instant) {
        self.by_id.retain(|_, (n, gone)| *n > 0 || gone.is_none_or(|g| now.duration_since(g) < BOX_FORGOTTEN_AFTER));
        let e = self.by_id.entry(id.to_string()).or_insert((0, None));
        e.0 += 1;
        e.1 = None;
    }

    /// A watch with box ID ended.
    pub fn down(&mut self, id: &str, now: std::time::Instant) {
        if let Some(e) = self.by_id.get_mut(id) {
            e.0 = e.0.saturating_sub(1);
            if e.0 == 0 {
                e.1 = Some(now);
            }
        }
    }

    /// Whether box ID has had no watch up for [`BOX_GONE_AFTER`]; a box never seen is not closed.
    pub fn closed(&self, id: &str, now: std::time::Instant) -> bool {
        matches!(self.by_id.get(id), Some((0, Some(g))) if now.duration_since(*g) >= BOX_GONE_AFTER)
    }
}

/// Why an owed step goes on now, if it does: its window CLOSED, else WAITED seconds of the policy's SECS.
fn because(closed: bool, waited: i64, secs: Option<u64>) -> Option<&'static str> {
    if closed {
        Some("closed")
    } else if secs.is_some_and(|s| waited >= s as i64) {
        Some("time")
    } else {
        None
    }
}

/// The policy's times that coordd acts on by the clock, and the policy files they were read from: `lapse-after` by
/// call, `key-again-after` (accounts/mover.rs `key_again`), the window opener's rule (accounts/opener.rs), and how far
/// back the brain reads his unanswered posts (`unanswered-within`, brain.rs) and his earlier ones (`earlier-within`,
/// `earlier-lines`), and the owner model's two (`owner-model-within`, `owner-model-rebuild-after`).
#[derive(Default)]
pub struct Times {
    stamp: Vec<(String, Option<std::time::SystemTime>)>,
    checked: Option<std::time::Instant>,
    by_call: BTreeMap<String, u64>,
    /// `lapse-at-most` by call: how long his writing holds such a step at most (E1b).
    at_most: BTreeMap<String, u64>,
    key_again_after: Option<u64>,
    opener: Option<crate::accounts::opener::Times>,
    unanswered_within: Option<u64>,
    earlier_within: Option<u64>,
    earlier_lines: Option<u64>,
    owner_model_within: Option<u64>,
    owner_model_rebuild_after: Option<u64>,
}

/// The seconds since AT (`2026-11-18T14:00:00Z`) at NOW; none when AT cannot be read.
fn waited(at: &str, now: chrono::DateTime<chrono::Utc>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(at).ok().map(|t| (now - t.to_utc()).num_seconds())
}

/// The outline's state for a thread whose only owed steps go on by themselves at AT: "goes through at HH:MM" on his
/// clock in ZONE, the minute AT falls in (proto-lead, N-1446; a retry time rounds up instead, `unanswered_words_in`).
pub fn goes_through_words<Z: chrono::TimeZone>(at: chrono::DateTime<chrono::Utc>, zone: &Z) -> String
where
    Z::Offset: std::fmt::Display,
{
    format!("goes through at {}", crate::core::his_clock(at, zone))
}

impl Core {
    /// The policy's times; read again at most every 2 s, and only when the policy files changed since.
    fn policy_times(&self) -> std::cell::RefMut<'_, Times> {
        let mut t = self.lapse_times.borrow_mut();
        let due = t.checked.is_none_or(|c| c.elapsed() >= std::time::Duration::from_secs(2));
        if due {
            t.checked = Some(std::time::Instant::now());
            let places = self.places();
            let stamp = places.stamp();
            if stamp != t.stamp {
                let scopes = places.sources().unwrap_or_default();
                let (mut by_call, mut at_most) = (BTreeMap::new(), BTreeMap::new());
                for (_, ds) in &scopes {
                    for d in ds {
                        if let crate::policy::syntax::DeclKind::Instance { fact, args } = &d.kind {
                            if let [crate::policy::syntax::Expr::Str(c), _] = args.as_slice() {
                                let n = match fact.as_str() {
                                    "lapse-after" => crate::policy::lapse_after(&scopes, c).map(|n| (&mut by_call, n)),
                                    "lapse-at-most" => crate::policy::lapse_at_most(&scopes, c).map(|n| (&mut at_most, n)),
                                    _ => None,
                                };
                                if let Some((into, n)) = n {
                                    into.insert(c.clone(), n);
                                }
                            }
                        }
                    }
                }
                *t = Times { stamp, checked: t.checked, by_call, at_most, key_again_after: crate::policy::key_again_after(&scopes), opener: crate::policy::opener(&scopes), unanswered_within: crate::policy::unanswered_within(&scopes), earlier_within: crate::policy::earlier_within(&scopes), earlier_lines: crate::policy::earlier_lines(&scopes), owner_model_within: crate::policy::owner_model_within(&scopes), owner_model_rebuild_after: crate::policy::owner_model_rebuild_after(&scopes) };
            }
        }
        t
    }

    /// The seconds the policy gives an owner's step of CALL.
    pub fn lapse_after(&self, call: &str) -> Option<u64> {
        self.policy_times().by_call.get(call).copied()
    }

    /// The seconds an owner's step of CALL on ticket T waits before it goes on by itself, now: the policy's
    /// `lapse-after`, and while he writes in the box of the ticket it waits on him in (T, or the thread a handler of his
    /// post is under; docket-core4's `writing_since`), its `lapse-at-most` when that is longer (E1b: his burst held 15
    /// s from his first post, up to 60 s while he types). The one reading `lapses_at` and the lapse round share.
    pub fn lapse_secs(&self, t: u64, call: &str) -> Option<u64> {
        let after = self.lapse_after(call)?;
        let m = &self.tickets.m;
        let writing = m.writing_since(t).is_some()
            || m.tickets.get(&t).and_then(|x| x.under).is_some_and(|u| m.writing_since(u).is_some())
            || m.tickets.get(&t).and_then(|x| x.form.get("to")?.as_str()).is_some_and(|a| self.writing.contains_key(a));
        match self.policy_times().at_most.get(call).copied() {
            Some(most) if writing => Some(after.max(most)),
            _ => Some(after),
        }
    }

    /// The seconds after which the policy takes a key account again that is out for its rate limit with no time
    /// named (`key-again-after`); none, it stays out until the owner clears it.
    pub fn key_again_after(&self) -> Option<u64> {
        self.policy_times().key_again_after
    }

    /// How far before his burst, in seconds, the brain reads his posts an agent has not answered
    /// (`unanswered-within`, `Machine::asked`); none, it reads none.
    pub fn unanswered_within(&self) -> Option<u64> {
        self.policy_times().unanswered_within
    }

    /// How far before his burst, in seconds, the brain reads his earlier posts as an index (`earlier-within`); none,
    /// it reads none.
    pub fn earlier_within(&self) -> Option<u64> {
        self.policy_times().earlier_within
    }

    /// How many of his earlier posts the brain's index lists (`earlier-lines`); none, the code's.
    pub fn earlier_lines(&self) -> Option<u64> {
        self.policy_times().earlier_lines
    }

    /// How far before the newest post of its update, in seconds, the owner model reads his posts when it is built whole
    /// (`owner-model-within`); none, no update of it is opened.
    pub fn owner_model_within(&self) -> Option<u64> {
        self.policy_times().owner_model_within
    }

    /// After how many misses the owner model is built whole again (`owner-model-rebuild-after`); none, no update of it
    /// is opened.
    pub fn owner_model_rebuild_after(&self) -> Option<u64> {
        self.policy_times().owner_model_rebuild_after
    }

    /// The window opener's rule as the policy states it; none, no window is opened (accounts/opener.rs).
    pub fn opener_times(&self) -> Option<crate::accounts::opener::Times> {
        self.policy_times().opener.clone()
    }

    /// When ticket T's owner's step goes on by itself, if it does: since it was owed, plus the policy's seconds.
    pub fn lapses_at(&self, t: u64) -> Option<String> {
        let x = self.tickets.m.tickets.get(&t)?;
        let Status::Awaits { agent, owed, since, .. } = &x.status else { return None };
        if agent != "owner" || owed.hold || self.tickets.m.slow(t) {
            return None;
        }
        self.tickets.m.call_of(t, &owed.call)?.default?;
        let secs = self.lapse_secs(t, &owed.call)?;
        let at = chrono::DateTime::parse_from_rfc3339(since).ok()?.to_utc() + chrono::Duration::seconds(secs as i64);
        Some(at.format("%Y-%m-%dT%H:%M:%SZ").to_string())
    }

    /// Each second (the follow round): the owner's steps whose time has passed, or whose window is closed, go on with
    /// their call's default, as the record's `lapsed` line. Reads only the kept set; nothing while it is empty.
    pub fn lapse_round(&mut self, now: chrono::DateTime<chrono::Utc>) {
        let owed = self.tickets.m.lapsing();
        let instant = std::time::Instant::now();
        for (t, step, call, default, since) in owed {
            let Some(w) = waited(&since, now) else { continue };
            let closed = self.tickets.m.tickets.get(&t).and_then(|x| x.form.get(FORM_BOX)?.as_str().map(String::from)).is_some_and(|b| self.boxes.closed(&b, instant));
            let Some(because) = because(closed, w, self.lapse_secs(t, &call)) else { continue };
            if self.tickets.m.slow(t) {
                continue;
            }
            match self.ticket_input(Input::Lapsed { t, step, head: json!({"head": default}), after: w.max(0) as u64, because: because.into() }) {
                Ok(said) => eprintln!("coordd: tickets: {said}"),
                Err(e) => eprintln!("coordd: tickets: lapse of step {step} of {t}: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_is_read_from_the_record_time() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-18T14:00:30Z").unwrap().to_utc();
        assert_eq!(waited("2026-11-18T14:00:00Z", now), Some(30));
        assert_eq!(waited("not a time", now), None);
    }

    #[test]
    fn a_closed_window_goes_on_at_once_and_else_the_time() {
        assert_eq!(because(true, 0, None), Some("closed"));
        assert_eq!(because(false, 29, Some(30)), None);
        assert_eq!(because(false, 30, Some(30)), Some("time"));
        assert_eq!(because(false, 999, None), None, "no lapse-after: only a closed window");
    }

    #[test]
    fn a_box_is_closed_once_no_watch_has_been_up_for_a_while() {
        let t0 = std::time::Instant::now();
        let mut b = Boxes::default();
        assert!(!b.closed("w1", t0 + BOX_GONE_AFTER * 2), "never seen: an older bridge, or before a restart");
        b.up("w1", t0);
        b.up("w1", t0);
        b.down("w1", t0);
        assert!(!b.closed("w1", t0 + BOX_GONE_AFTER * 2), "one watch still up");
        b.down("w1", t0);
        assert!(!b.closed("w1", t0 + BOX_GONE_AFTER / 2), "the bridge may come back");
        assert!(b.closed("w1", t0 + BOX_GONE_AFTER));
        b.up("w1", t0 + BOX_GONE_AFTER);
        assert!(!b.closed("w1", t0 + BOX_GONE_AFTER * 3), "it came back");
        // forgotten after a day, so the set stays small
        b.down("w1", t0);
        b.up("w2", t0 + BOX_FORGOTTEN_AFTER);
        assert!(!b.by_id.contains_key("w1"));
    }
}
