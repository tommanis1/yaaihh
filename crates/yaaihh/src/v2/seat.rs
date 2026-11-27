//! His seat at the engine: the window driver's socket (`[v2] window`), where his words on a thread go and his
//! decisions on what waits for him (asked.rs). The driver seats him at the connection, made when the window starts.

use super::desk::HIM;
use super::lines::Lines;
use super::V2;
use crate::{App, Pending};
use coord_types::Request;
use coordd_v2_link::pipe::Connect;
use serde_json::{json, Value as Json};
use std::collections::{BTreeMap, VecDeque};

/// What a line written was, so its answer goes where it belongs.
enum Out {
    /// His words from a box, on thread T (its id): the box's answer.
    Words(Pending, String),
    Decision(u64),
}

/// The connection to the window driver, what is out on it, and what the driver said of each decision.
pub struct Seat {
    lines: Lines,
    out: VecDeque<Out>,
    /// Each decision sent: None once sent and while taken (the record's answer then ends the question), else why not.
    pub decided: BTreeMap<u64, Option<String>>,
    /// The driver's word with nothing asked: why it would not keep this window's connection.
    refused: Option<String>,
}

impl Seat {
    /// Connected at once, so he is seated as his window starts.
    pub fn new(connect: Connect, wake: rune::Waker) -> Seat {
        let mut lines = Lines::new(connect, wake);
        lines.open();
        Seat { lines, out: VecDeque::new(), decided: BTreeMap::new(), refused: None }
    }

    /// Connected again if it is not (the engine came up again).
    pub fn sit(&mut self) {
        self.lines.open();
    }

    /// Whether he is seated, or why not.
    pub fn state(&self) -> Result<(), String> {
        match self.lines.down() {
            None => Ok(()),
            Some(down) => Err(self.refused.clone().unwrap_or_else(|| down.to_string())),
        }
    }

    /// Line `j` written, kept as OUT until its answer comes; Err why it could not be.
    fn send(&mut self, j: &Json, out: Out) -> Result<(), String> {
        if !self.lines.send(&j.to_string()) {
            return Err(self.lines.down().unwrap_or("not connected").to_string());
        }
        self.out.push_back(out);
        Ok(())
    }

    /// The answers that came: each box's, with whether it was taken; each decision's kept here; whether any came.
    fn take(&mut self) -> (Vec<(Pending, bool, String)>, bool) {
        let (came, ended) = self.lines.drain();
        let mut boxes = Vec::new();
        for l in &came {
            let j: Json = serde_json::from_str(l).unwrap_or_else(|e| json!({"no": format!("not JSON: {e}")}));
            let no = j.get("no").map(|w| w.as_str().map_or_else(|| w.to_string(), str::to_string));
            match self.out.pop_front() {
                Some(Out::Words(what, t)) => boxes.push((what, no.is_none(), no.unwrap_or_else(|| format!("Posted on {t}.")))),
                Some(Out::Decision(line)) => drop(self.decided.insert(line, no)),
                None => self.refused = no,
            }
        }
        // what was out when the connection ended may have been taken: his message goes once its copy is in the record
        if ended {
            let why = self.lines.down().unwrap_or_default().to_string();
            for o in self.out.drain(..) {
                match o {
                    Out::Words(what, _) => boxes.push((what, false, format!("Unknown if sent ({why}); check the thread first"))),
                    Out::Decision(line) => drop(self.decided.insert(line, Some(why.clone()))),
                }
            }
        }
        (boxes, !came.is_empty() || ended)
    }
}

fn seat_of(s: &mut App) -> Option<&mut Seat> {
    s.ext.try_get_mut::<V2>()?.seat.as_mut()
}

/// REQ from a box of the engine's project, sent as his words; false, and why in the notice, when it cannot go.
pub(crate) fn request(s: &mut App, req: &Request, what: Pending) -> bool {
    let sent = words(s, req).and_then(|(t, j)| {
        if !matches!(what, Pending::TicketDone { .. }) {
            return Err("only a thread's box sends to the engine".into());
        }
        seat_of(s).ok_or("no window driver in [v2]")?.send(&j, Out::Words(what, t))
    });
    s.notice = sent.err().map(|why| format!("not sent: {why}"));
    s.notice.is_none()
}

/// His words of REQ as the driver takes them, with the thread's id; Err when it is not a post on a thread of the engine's,
/// or would reach no one.
fn words(s: &App, req: &Request) -> Result<(String, Json), String> {
    let Request::Post { t, text, to, also, .. } = req else { return Err("not a post on a thread".into()) };
    let e = s.ext.try_get::<V2>().ok_or("no engine")?;
    let v = e.at.and_then(|pi| s.projects[pi].live.tickets.values().find(|v| v.id == *t)).ok_or_else(|| format!("{t} is not a thread of the engine's"))?;
    let on = v.ctx.get("thread").and_then(Json::as_str).ok_or_else(|| format!("{t} has no name in the engine"))?;
    // he is never one it is for: by the threads' name for him, or the party the record seated
    let seated = e.map.as_ref().and_then(|m| e.desk.owner(m.as_ref().ok()?));
    let other = |w: &&str| *w != HIM && Some(*w) != seated.as_deref();
    let mut to: Vec<&str> = to.iter().chain(also).map(String::as_str).filter(other).collect();
    // none named: whom his words on it went to first, else the agent that opened it
    if to.is_empty() {
        to.extend(v.form.get("to").and_then(Json::as_str).filter(|w| !w.is_empty()).or(v.opener.as_deref()).filter(other));
    }
    if to.is_empty() {
        return Err("no agent here yet; reply to an agent's post".into());
    }
    Ok((t.clone(), json!({"words": {"text": text, "on": on, "to": to, "at": chrono::Utc::now().timestamp()}})))
}

/// His decision ANSWER on the question at LINE, sent to the driver; why not, kept for the question, when it cannot go.
pub fn decide(s: &mut App, line: u64, answer: Json) {
    let Some(seat) = seat_of(s) else { return };
    let sent = seat.send(&json!({"decision": {"line": line, "answer": answer}}), Out::Decision(line));
    seat.decided.insert(line, sent.err());
}

/// What the driver answered, given to the boxes and kept for the decisions; whether anything came.
pub fn drain(s: &mut App) -> bool {
    let Some((boxes, came)) = seat_of(s).map(Seat::take) else { return false };
    for (what, ok, out) in boxes {
        crate::threads::replied(s, what, ok, out);
    }
    came
}

#[cfg(all(test, unix))]
mod tests {
    use super::{Out, Seat};
    use docket_driver_window::{Config, Window};
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    /// The driver in a folder of its own for test NAME, vouching for every connection as WHO; its folder.
    fn driver(name: &str, who: &'static str) -> (Window, PathBuf) {
        let d = std::env::temp_dir().join(format!("yaaihh-seat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let c = json!({"owner": "o1", "socket": d.join("w.sock"), "words": d.join("words"), "posts": {"event": "p.Put"}, "written": {"event": "p.When"}});
        (Window::new(Config::read(&c.to_string()).unwrap(), Box::new(move |_: &std::os::unix::net::UnixStream| Some(who.into()))).unwrap(), d)
    }

    fn seat(d: &Path) -> Seat {
        Seat::new(crate::engine_v2::Reach::Socket(d.join("w.sock")).connect(), std::sync::Arc::new(|| {}))
    }

    /// Until DONE, taking what came; the test fails after a while.
    fn until(seat: &mut Seat, mut done: impl FnMut(&mut Seat) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !done(seat) {
            assert!(Instant::now() < end, "nothing came");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A window the daemon vouches for as another party is not seated, and says the driver's why.
    #[test]
    fn a_window_vouched_for_as_another_party_is_not_seated_and_says_why() {
        let (mut w, d) = driver("other", "w1");
        let mut seat = seat(&d);
        assert_eq!(seat.state(), Ok(()), "connected");
        until(&mut seat, |seat| {
            assert_eq!(w.ready(), (vec![], vec![]), "not seated");
            seat.take();
            seat.state().is_err()
        });
        assert!(seat.state().unwrap_err().contains("w1 is not o1"), "{:?}", seat.state());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Words out when the connection ends are not said to be unsent: whether the engine had them is not known.
    #[test]
    fn words_out_when_the_connection_ends_are_not_known_to_have_gone() {
        let (w, d) = driver("ended", "o1");
        let mut seat = seat(&d);
        seat.send(&json!({"words": {"text": "a note", "on": "t1", "to": ["w1"], "at": 0}}), Out::Words(crate::Pending::Sent, "N-1".into())).unwrap();
        drop(w);
        let mut told = Vec::new();
        until(&mut seat, |seat| {
            told.extend(seat.take().0.into_iter().map(|(_, ok, out)| (ok, out)));
            !told.is_empty()
        });
        assert_eq!(told.len(), 1);
        assert!(!told[0].0 && told[0].1.starts_with("Unknown if sent") && told[0].1.ends_with("check the thread first"), "{told:?}");
        let _ = std::fs::remove_dir_all(&d);
    }
}
