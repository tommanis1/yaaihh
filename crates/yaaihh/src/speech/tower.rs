//! Speech on his tower's GPU (owner, post-v041-voice-gpu): with `[tower]` in speech.toml, a take's sound goes over
//! the tailnet to ops/voice/worker.py and its words come back; the take keeps every sample the tower has not settled
//! yet, so a dropped connection loses no word (N-1104.p504): it reconnects and sends them again from the last settled
//! one. A tower not reached at the start, or away longer than `fallback_ms`, hands the kept sound to this PC's own
//! engine, which takes the rest of the take. Wire: ops/voice/worker.py's head.
//!
//! A tower that keeps the connection but stops answering (a frozen VM, a path that drops everything) is away once it
//! has said nothing for `quiet_ms`: the worker answers a line at each look, about one a second of sound.
//!
//!   [tower]
//!   at = "tower-server:8765"   (the worker's HOST:PORT on the tailnet)
//!   fallback_ms = 1500         (how long the tower may be away mid-take)
//!   quiet_ms = 3000            (how long a connected tower may say nothing)
//!   token = ""                 (the worker config's token, when it has one)

use super::{Recogniser, Words};
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, channel, sync_channel};
use std::time::{Duration, Instant};

#[cfg(test)]
mod tests;

/// How many takes are on the tower now (the box's "Loading speech's model…" is not shown while one is).
pub static SERVING: AtomicUsize = AtomicUsize::new(0);

/// One take's place in a count such as SERVING, held while it lives: a take ending never clears another's.
struct Serving(&'static AtomicUsize);

impl Serving {
    fn on(count: &'static AtomicUsize) -> Serving {
        count.fetch_add(1, Ordering::Relaxed);
        Serving(count)
    }
}

impl Drop for Serving {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// The tower's speech as speech.toml's `[tower]` names it.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Tower {
    pub at: String,
    #[serde(default = "fallback_ms")]
    pub fallback_ms: u64,
    /// how long a take's end waits for the tower's last words
    #[serde(default = "finish_ms")]
    pub finish_ms: u64,
    #[serde(default = "quiet_ms")]
    pub quiet_ms: u64,
    #[serde(default)]
    pub token: String,
}

fn fallback_ms() -> u64 {
    1500
}

fn finish_ms() -> u64 {
    15_000
}

fn quiet_ms() -> u64 {
    3_000
}

#[derive(Deserialize)]
struct File {
    tower: Option<Tower>,
}

/// speech.toml in the config folder: its `[tower]`, none without the file or the table; a file that does not read is
/// said in words.
pub fn config() -> Result<Option<Tower>, String> {
    read(&crate::places::config_dir().join("speech.toml"))
}

/// PATH's `[tower]`: none when there is no such file; any other failure to read it, in words.
fn read(path: &Path) -> Result<Option<Tower>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} does not read: {e}", path.display())),
    };
    let f: File = toml::from_str(&text).map_err(|e| format!("{} does not read: {e}", path.display()))?;
    Ok(f.tower)
}

/// The engine a take falls back to, made only when needed (it loads this PC's model).
pub type Local<'a> = Box<dyn FnOnce() -> Result<Box<dyn Recogniser + 'a>, String> + Send + 'a>;

/// What the tower said, one JSON line each.
#[derive(Debug, PartialEq)]
enum Came {
    Settled(String, u64),
    Tail(String),
    /// a look that changed nothing: the tower is there
    Heard,
    End,
    /// the worker said why it stopped: no reconnect, this PC's engine at once
    Refused(String),
    /// the connection went
    Broken(String),
}

fn came(line: &str) -> Came {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return Came::Refused(format!("the tower said {line:?}")) };
    let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    if let Some(why) = text("error") {
        Came::Refused(why)
    } else if v.get("end").is_some() {
        Came::End
    } else if let (Some(t), Some(upto)) = (text("settled"), v.get("upto").and_then(|x| x.as_u64())) {
        Came::Settled(t, upto)
    } else if let Some(t) = text("tail") {
        Came::Tail(t)
    } else if v.get("heard").is_some_and(|x| x.is_u64()) {
        Came::Heard
    } else {
        Came::Refused(format!("the tower said {line:?}"))
    }
}

fn frame(kind: u8, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(5 + body.len());
    f.push(kind);
    f.extend_from_slice(&(body.len() as u32).to_le_bytes());
    f.extend_from_slice(body);
    f
}

fn audio(pcm: &[i16]) -> Vec<u8> {
    frame(b'A', &pcm.iter().flat_map(|s| s.to_le_bytes()).collect::<Vec<u8>>())
}

/// One connection: frames go out on a thread of their own, so a slow network never holds the take; lines come back on
/// another. Both queues are bounded. Dropping it closes the socket.
struct Link {
    out: SyncSender<Vec<u8>>,
    came: Receiver<Came>,
}

const DIAL: Duration = Duration::from_millis(500);
const REDIAL: Duration = Duration::from_millis(250);
/// the most sound one frame carries: a second
const PIECE: usize = super::RATE as usize;
/// the most sound a take keeps for the tower: a minute; past it the tower has fallen behind, and this PC has the take
const KEPT_MAX: usize = 60 * super::RATE as usize;
/// the frames waiting to go out (a minute's kept pieces fit); one more and the connection is behind
const OUT_FRAMES: usize = 128;
/// the tower's lines waiting to be read; past them its reading thread waits
const LINES: usize = 256;
/// the longest one frame may take to go out
const WRITE: Duration = Duration::from_secs(2);

fn dial(at: &str) -> Result<Link, String> {
    let addrs = at.to_socket_addrs().map_err(|e| format!("{at} is not found: {e}"))?;
    let mut last = format!("{at} is not found");
    for a in addrs {
        match TcpStream::connect_timeout(&a, DIAL) {
            Ok(s) => return link(s),
            Err(e) => last = format!("{at}: {e}"),
        }
    }
    Err(last)
}

fn link(s: TcpStream) -> Result<Link, String> {
    let _ = s.set_nodelay(true);
    let _ = s.set_write_timeout(Some(WRITE));
    let read = s.try_clone().map_err(|e| e.to_string())?;
    let (out, frames) = sync_channel::<Vec<u8>>(OUT_FRAMES);
    let (tell, lines) = sync_channel(LINES);
    let broke = tell.clone();
    std::thread::Builder::new()
        .name("speech-tower-in".into())
        .spawn(move || {
            for line in BufReader::new(read).lines() {
                match line {
                    Ok(l) if l.trim().is_empty() => {}
                    Ok(l) => {
                        let _ = tell.send(came(&l));
                    }
                    Err(e) => {
                        let _ = tell.send(Came::Broken(e.to_string()));
                        return;
                    }
                }
            }
            let _ = tell.send(Came::Broken("the tower closed the connection".into()));
        })
        .map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("speech-tower-out".into())
        .spawn(move || {
            let mut s = s;
            for f in frames {
                if let Err(e) = s.write_all(&f) {
                    let _ = broke.try_send(Came::Broken(e.to_string()));
                    break;
                }
            }
            // the take let go of the link: both directions end, so the reading thread ends too
            let _ = s.shutdown(Shutdown::Both);
        })
        .map_err(|e| e.to_string())?;
    Ok(Link { out, came: lines })
}

/// A take on the tower, with this PC's engine behind it.
pub struct OnTower<'a> {
    cfg: Tower,
    project: String,
    link: Option<Link>,
    /// when the connected tower last said something (or was joined)
    heard_at: Instant,
    serving: Option<Serving>,
    dialing: Option<Receiver<Result<Link, String>>>,
    dialed: Option<Instant>,
    /// since when the tower is not reached, and why
    away: Option<(Instant, String)>,
    /// the take's sound from sample `upto` on: what the tower has not settled yet
    kept: Vec<i16>,
    upto: u64,
    /// the end of the settled words, for the tower's prompt after a reconnect
    said: String,
    tail: String,
    ending: bool,
    local: Option<Box<dyn Recogniser + 'a>>,
    make_local: Option<Local<'a>>,
}

fn pcm16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

impl<'a> OnTower<'a> {
    /// A take for PROJECT: on the tower when it answers now, else on LOCAL from the start.
    pub fn start(cfg: Tower, project: &str, local: Local<'a>) -> Result<OnTower<'a>, String> {
        let mut t = OnTower {
            cfg,
            project: project.to_string(),
            link: None,
            heard_at: Instant::now(),
            serving: None,
            dialing: None,
            dialed: None,
            away: None,
            kept: Vec::new(),
            upto: 0,
            said: String::new(),
            tail: String::new(),
            ending: false,
            local: None,
            make_local: Some(local),
        };
        match dial(&t.cfg.at) {
            Ok(l) => t.joined(l),
            Err(why) => {
                t.go_local(&why)?;
            }
        }
        Ok(t)
    }

    /// Whether this PC's engine has the take.
    pub fn is_local(&self) -> bool {
        self.local.is_some()
    }

    fn joined(&mut self, l: Link) {
        let mut head = serde_json::json!({"v": 1, "project": self.project, "from": self.upto, "before": self.said, "resent": self.kept.len()});
        if !self.cfg.token.is_empty() {
            head["token"] = self.cfg.token.clone().into();
        }
        self.link = Some(l);
        self.heard_at = Instant::now();
        self.serving.get_or_insert_with(|| Serving::on(&SERVING));
        self.put(frame(b'H', head.to_string().as_bytes()));
        let pieces: Vec<Vec<u8>> = self.kept.chunks(PIECE).map(audio).collect();
        for f in pieces {
            self.put(f);
        }
        if self.ending {
            self.put(frame(b'E', &[]));
        }
    }

    /// FRAME to the tower; a connection that cannot take it (OUT_FRAMES behind, or gone) is away from now.
    fn put(&mut self, f: Vec<u8>) {
        if let Some(l) = &self.link
            && l.out.try_send(f).is_err()
        {
            self.broke("the tower's connection fell behind".into());
        }
    }

    fn broke(&mut self, why: String) {
        self.link = None;
        self.away.get_or_insert((Instant::now(), why));
    }

    /// The take moves to this PC's engine: fed the kept sound, its words from here on. Why the tower is not used goes in
    /// the failure if that engine cannot start either.
    fn go_local(&mut self, why: &str) -> Result<Words, String> {
        self.link = None;
        self.dialing = None;
        self.serving = None;
        let make = self.make_local.take().ok_or("speech's engine was already taken")?;
        let mut local = make().map_err(|e| format!("The tower's speech is not reached ({why}), and this PC's: {e}"))?;
        let pcm: Vec<f32> = self.kept.drain(..).map(|s| s as f32 / 32767.0).collect();
        let w = if pcm.is_empty() { Words::default() } else { local.feed(&pcm)? };
        self.local = Some(local);
        self.tail = w.tail.clone();
        Ok(w)
    }

    /// What came from the tower so far, into W; true at its end.
    fn drain(&mut self, w: &mut Words, wait: Duration) -> Result<bool, String> {
        let Some(l) = &self.link else { return Ok(false) };
        let mut got: Vec<Came> = match l.came.recv_timeout(wait) {
            Ok(c) => vec![c],
            Err(RecvTimeoutError::Timeout) => Vec::new(),
            Err(RecvTimeoutError::Disconnected) => vec![Came::Broken("the tower's connection went".into())],
        };
        got.extend(l.came.try_iter());
        if got.is_empty() && self.heard_at.elapsed() >= Duration::from_millis(self.cfg.quiet_ms) {
            self.broke("the tower stopped answering".into());
            return Ok(false);
        }
        for c in got {
            // the tower is back only once it answers: one that takes the connection and closes it stays away
            if matches!(c, Came::Settled(..) | Came::Tail(_) | Came::Heard) {
                self.away = None;
            }
            if !matches!(c, Came::Broken(_)) {
                self.heard_at = Instant::now();
            }
            match c {
                Came::Settled(text, upto) => {
                    let n = upto.saturating_sub(self.upto).min(self.kept.len() as u64) as usize;
                    self.kept.drain(..n);
                    self.upto += n as u64;
                    self.said.push_str(&text);
                    // the worker's prompt keeps only its last 150 characters (ops/voice/prompt.py)
                    let cut = (self.said.len().saturating_sub(150)..=self.said.len()).find(|i| self.said.is_char_boundary(*i)).unwrap_or(0);
                    self.said.drain(..cut);
                    w.settled.push_str(&text);
                }
                Came::Tail(t) => self.tail = t,
                Came::Heard => {}
                Came::End => return Ok(true),
                Came::Refused(why) => {
                    let lw = self.go_local(&why)?;
                    w.settled.push_str(&lw.settled);
                    return Ok(false);
                }
                Came::Broken(why) => {
                    self.broke(why);
                    break;
                }
            }
        }
        Ok(false)
    }

    /// With the tower away: dial again now and then; past `fallback_ms`, this PC's engine.
    fn keep_up(&mut self, w: &mut Words) -> Result<(), String> {
        if self.link.is_some() || self.local.is_some() {
            return Ok(());
        }
        if let Some(r) = &self.dialing
            && let Ok(got) = r.try_recv()
        {
            self.dialing = None;
            match got {
                Ok(l) => {
                    self.joined(l);
                    return Ok(());
                }
                Err(why) => self.away = Some((self.away.as_ref().map_or_else(Instant::now, |a| a.0), why)),
            }
        }
        let (since, why) = self.away.clone().unwrap_or_else(|| (Instant::now(), "the tower is away".into()));
        if since.elapsed() >= Duration::from_millis(self.cfg.fallback_ms) {
            let lw = self.go_local(&why)?;
            w.settled.push_str(&lw.settled);
            return Ok(());
        }
        if self.dialing.is_none() && self.dialed.is_none_or(|t| t.elapsed() >= REDIAL) {
            let (send, r) = channel();
            let at = self.cfg.at.clone();
            std::thread::spawn(move || {
                let _ = send.send(dial(&at));
            });
            self.dialing = Some(r);
            self.dialed = Some(Instant::now());
        }
        Ok(())
    }
}

impl Recogniser for OnTower<'_> {
    fn feed(&mut self, pcm: &[f32]) -> Result<Words, String> {
        if let Some(l) = &mut self.local {
            return l.feed(pcm);
        }
        let s: Vec<i16> = pcm.iter().map(|x| pcm16(*x)).collect();
        self.kept.extend_from_slice(&s);
        let mut w = Words::default();
        if self.kept.len() > KEPT_MAX {
            let lw = self.go_local("the tower fell a minute behind")?;
            return Ok(Words { settled: lw.settled, tail: self.tail.clone() });
        }
        for piece in s.chunks(PIECE) {
            self.put(audio(piece));
        }
        self.drain(&mut w, Duration::ZERO)?;
        self.keep_up(&mut w)?;
        w.tail = self.tail.clone();
        Ok(w)
    }

    fn finish(&mut self) -> Result<Words, String> {
        let mut w = Words::default();
        if self.local.is_none() {
            self.ending = true;
            self.put(frame(b'E', &[]));
            let until = Instant::now() + Duration::from_millis(self.cfg.finish_ms);
            loop {
                if self.drain(&mut w, Duration::from_millis(20))? {
                    self.serving = None;
                    return Ok(Words { settled: w.settled, tail: String::new() });
                }
                if self.local.is_some() {
                    break;
                }
                if Instant::now() >= until {
                    let lw = self.go_local("the tower did not finish the take in time")?;
                    w.settled.push_str(&lw.settled);
                    break;
                }
                self.keep_up(&mut w)?;
                if self.link.is_none() && self.local.is_none() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
        let lw = self.local.as_mut().map_or_else(|| Ok(Words::default()), |l| l.finish())?;
        w.settled.push_str(&lw.settled);
        Ok(Words { settled: w.settled, tail: String::new() })
    }
}
