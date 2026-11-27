//! The tower's client against a mock worker on this machine: it speaks the wire of ops/voice/worker.py and settles
//! one word per second of sound, `wN ` for second N counted from the take's start, the last part second at the end.

use super::*;
use std::io::Read;
use std::net::TcpListener;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

const SECOND: u64 = super::super::RATE as u64;

/// The mock: the headers it was given; its first connection is cut after CUT samples; with ONCE it answers no further
/// connection (the tower gone for good).
struct Mock {
    at: String,
    heads: Arc<Mutex<Vec<serde_json::Value>>>,
    /// the longest sound frame it was sent, in bytes
    longest: Arc<AtomicUsize>,
}

fn mock(cut: Option<u64>, once: bool) -> Mock {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = l.local_addr().unwrap().to_string();
    let heads = Arc::new(Mutex::new(Vec::new()));
    let longest = Arc::new(AtomicUsize::new(0));
    let (h, most) = (heads.clone(), longest.clone());
    std::thread::spawn(move || {
        for (n, s) in l.incoming().enumerate() {
            let (h, most, first) = (h.clone(), most.clone(), n == 0);
            let s = s.unwrap();
            let t = std::thread::spawn(move || serve(s, h, most, cut.filter(|_| first)));
            if once {
                // the listener goes with this thread: the next dial is refused
                let _ = t.join();
                return;
            }
        }
    });
    Mock { at, heads, longest }
}

fn read_frame(s: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    s.read_exact(&mut head).ok()?;
    let mut body = vec![0u8; u32::from_le_bytes(head[1..].try_into().unwrap()) as usize];
    s.read_exact(&mut body).ok()?;
    Some((head[0], body))
}

fn serve(mut s: TcpStream, heads: Arc<Mutex<Vec<serde_json::Value>>>, longest: Arc<AtomicUsize>, cut: Option<u64>) {
    let mut w = s.try_clone().unwrap();
    let (mut at, mut got) = (0u64, 0u64);
    let mut say = |v: serde_json::Value| writeln!(w, "{v}").unwrap();
    while let Some((kind, body)) = read_frame(&mut s) {
        match kind {
            b'H' => {
                let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
                at = v["from"].as_u64().unwrap();
                heads.lock().unwrap().push(v);
            }
            b'A' => {
                longest.fetch_max(body.len(), Ordering::Relaxed);
                for _ in 0..body.len() / 2 {
                    at += 1;
                    got += 1;
                    if at % SECOND == 0 {
                        say(serde_json::json!({"settled": format!("w{} ", at / SECOND - 1), "upto": at}));
                    }
                }
                say(serde_json::json!({"tail": format!("t{}", at / SECOND)}));
                if cut.is_some_and(|c| got >= c) {
                    // the network drops: what was written still arrives, then the end
                    let _ = s.shutdown(Shutdown::Write);
                    while read_frame(&mut s).is_some() {}
                    return;
                }
            }
            b'E' => {
                if at % SECOND != 0 {
                    say(serde_json::json!({"settled": format!("w{}", at / SECOND), "upto": at}));
                }
                say(serde_json::json!({"end": true}));
                return;
            }
            _ => panic!("no frame kind {kind}"),
        }
    }
}

/// This PC's engine, stood in for: says `L<samples it was fed>` at the end; counts how often it was made.
struct Here(usize);

impl Recogniser for Here {
    fn feed(&mut self, pcm: &[f32]) -> Result<Words, String> {
        self.0 += pcm.len();
        Ok(Words::default())
    }
    fn finish(&mut self) -> Result<Words, String> {
        Ok(Words { settled: format!("L{}", self.0), tail: String::new() })
    }
}

fn here(made: &Arc<AtomicUsize>) -> Local<'static> {
    let made = made.clone();
    Box::new(move || {
        made.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(Here(0)) as Box<dyn Recogniser>)
    })
}

fn tower(at: &str, fallback_ms: u64) -> Tower {
    Tower { at: at.into(), fallback_ms, finish_ms: 5_000, quiet_ms: 3_000, token: String::new() }
}

/// PIECES of 100 ms of sound fed, then the end: every settled word, and each tail as it changed.
fn take(t: &mut OnTower, pieces: usize) -> (String, Vec<String>) {
    let (mut words, mut tails) = (String::new(), Vec::new());
    for _ in 0..pieces {
        let w = t.feed(&[0.1; 1_600]).unwrap();
        words.push_str(&w.settled);
        if tails.last() != Some(&w.tail) {
            tails.push(w.tail);
        }
    }
    let end = t.finish().unwrap();
    words.push_str(&end.settled);
    assert_eq!(end.tail, "", "nothing is open after the end");
    (words, tails)
}

/// A port nothing listens on: the tower is off.
fn nobody() -> String {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string()
}

/// Sound sent in pieces comes back as the tower settles it, in order and whole, with the project in the take's head;
/// this PC's engine is never made.
#[test]
fn a_take_on_the_tower_round_trips_in_pieces() {
    let m = mock(None, false);
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&m.at, 1_500), "research", here(&made)).unwrap();
    let (words, tails) = take(&mut t, 25);
    assert_eq!(words, "w0 w1 w2");
    assert!(tails.iter().all(|t| ["", "t0", "t1", "t2"].contains(&t.as_str())), "the open words stream as they come: {tails:?}");
    let heads = m.heads.lock().unwrap();
    assert_eq!((heads[0]["v"].as_u64(), heads[0]["project"].as_str(), heads[0]["from"].as_u64()), (Some(1), Some("research"), Some(0)));
    assert_eq!(made.load(Ordering::Relaxed), 0);
    assert!(!t.is_local());
}

/// The connection cut mid-take loses no word and doubles none: the take reconnects and sends again what the tower
/// had not settled, from its last settled sample, with its words so far for the prompt.
#[test]
fn a_dropped_connection_loses_no_word() {
    let m = mock(Some(SECOND + SECOND / 2), false);
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&m.at, 5_000), "research", here(&made)).unwrap();
    let words = take(&mut t, 25).0;
    assert_eq!(words, "w0 w1 w2");
    let heads = m.heads.lock().unwrap();
    assert_eq!(heads.len(), 2, "one reconnect");
    assert_eq!((heads[1]["from"].as_u64(), heads[1]["before"].as_str()), (Some(SECOND), Some("w0 ")));
    assert!(heads[1]["resent"].as_u64().is_some_and(|n| n >= SECOND / 2), "it says how much it sends again: {}", heads[1]);
    assert!(heads[0].get("token").is_none(), "no token unless speech.toml has one");
    assert_eq!(made.load(Ordering::Relaxed), 0, "the tower came back in time: this PC's engine was never made");
}

/// No tower at the start: this PC's engine has the whole take.
#[test]
fn no_tower_at_the_start_runs_here() {
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&nobody(), 1_500), "research", here(&made)).unwrap();
    assert!(t.is_local());
    assert_eq!(take(&mut t, 10).0, "L16000");
    assert_eq!(made.load(Ordering::Relaxed), 1);
}

/// The tower gone for good mid-take: past `fallback_ms`, this PC's engine is fed exactly the sound the tower had not
/// settled, so the words the tower gave stand and nothing after them is lost.
#[test]
fn a_tower_gone_mid_take_hands_its_unsettled_sound_here() {
    let m = mock(Some(SECOND + SECOND / 2), true);
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&m.at, 100), "research", here(&made)).unwrap();
    let words = take(&mut t, 25).0;
    assert_eq!(words, format!("w0 L{}", 25 * 1_600 - SECOND as usize));
    assert_eq!(made.load(Ordering::Relaxed), 1);
}

/// A tower that takes each connection and closes it unanswered (this PC not in its `allow`) is away all the same:
/// past `fallback_ms` this PC's engine has the whole take.
#[test]
fn a_tower_that_closes_unanswered_falls_back() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for s in l.incoming() {
            drop(s);
        }
    });
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&at, 100), "research", here(&made)).unwrap();
    let (mut words, mut fed) = (String::new(), 0);
    let until = Instant::now() + Duration::from_secs(5);
    while !t.is_local() && Instant::now() < until {
        words.push_str(&t.feed(&[0.1; 1_600]).unwrap().settled);
        fed += 1_600;
        std::thread::sleep(Duration::from_millis(10));
    }
    words.push_str(&t.finish().unwrap().settled);
    assert_eq!(words, format!("L{fed}"));
    assert_eq!(made.load(Ordering::Relaxed), 1);
}

/// Neither the tower nor this PC's engine: one failure, in words, with both reasons.
#[test]
fn neither_is_said_with_both_reasons() {
    let none: Local<'static> = Box::new(|| Err("its model is not on this machine yet".into()));
    let why = OnTower::start(tower(&nobody(), 1_500), "research", none).err().unwrap();
    assert!(why.starts_with("The tower's speech is not reached ("), "{why}");
    assert!(why.ends_with("and this PC's: its model is not on this machine yet"), "{why}");
}

/// The worker's lines read as the wire says; one that does not read is a refusal, never a word.
#[test]
fn the_workers_lines_read() {
    assert_eq!(came(r#"{"settled": " yaaihh", "upto": 16000}"#), Came::Settled(" yaaihh".into(), 16_000));
    assert_eq!(came(r#"{"tail": "tov"}"#), Came::Tail("tov".into()));
    assert_eq!(came(r#"{"end": true}"#), Came::End);
    assert_eq!(came(r#"{"heard": 32000}"#), Came::Heard);
    assert_eq!(came(r#"{"error": "CUDA out of memory"}"#), Came::Refused("CUDA out of memory".into()));
    assert!(matches!(came("not json"), Came::Refused(_)));
    assert_eq!(audio(&[1, -2]), vec![b'A', 4, 0, 0, 0, 1, 0, 0xfe, 0xff]);
    assert_eq!(pcm16(2.0), i16::MAX);
}

/// speech.toml's `[tower]` reads with its defaults.
#[test]
fn speech_toml_reads() {
    let f: File = toml::from_str("[tower]\nat = \"tower-server:8765\"\n").unwrap();
    assert_eq!(f.tower, Some(Tower { at: "tower-server:8765".into(), fallback_ms: 1_500, finish_ms: 15_000, quiet_ms: 3_000, token: String::new() }));
}

/// No speech.toml is no tower; one that is there and cannot be read is said, never taken for none.
#[test]
fn a_speech_toml_that_cannot_be_read_is_said() {
    let dir = std::env::temp_dir().join(format!("yaaihh-speech-toml-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(read(&dir.join("none.toml")), Ok(None));
    let why = read(&dir).unwrap_err();
    assert!(why.contains("does not read"), "a folder where the file should be: {why}");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A tower that takes the connection and the sound and never answers (a frozen VM, a path that drops everything):
/// away after `quiet_ms`, and this PC's engine has the take long before the end's wait.
#[test]
fn a_tower_that_stops_answering_falls_back_mid_take() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = l.local_addr().unwrap().to_string();
    let held = Arc::new(Mutex::new(Vec::new()));
    let h = held.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            // kept open, never read, never answered
            h.lock().unwrap().push(s.unwrap());
        }
    });
    let made = Arc::new(AtomicUsize::new(0));
    let mut cfg = tower(&at, 100);
    cfg.quiet_ms = 300;
    let mut t = OnTower::start(cfg, "research", here(&made)).unwrap();
    assert!(!t.is_local(), "it took the connection");
    let (started, mut fed) = (Instant::now(), 0);
    while !t.is_local() && started.elapsed() < Duration::from_secs(3) {
        t.feed(&[0.1; 1_600]).unwrap();
        fed += 1_600;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(t.is_local(), "still waiting on a silent tower after {:?}", started.elapsed());
    assert_eq!(t.finish().unwrap().settled, format!("L{fed}"), "every sample went to this PC's engine");
    assert_eq!(made.load(Ordering::Relaxed), 1);
}

/// A tower that answers but never settles cannot make the take keep sound without end: past a minute kept, this
/// PC's engine has it.
#[test]
fn a_tower_that_never_settles_keeps_at_most_a_minute() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let at = l.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let mut s = s.unwrap();
            let mut w = s.try_clone().unwrap();
            std::thread::spawn(move || {
                while let Some((kind, _)) = read_frame(&mut s) {
                    if kind == b'A' && writeln!(w, "{}", serde_json::json!({"tail": "still open"})).is_err() {
                        return;
                    }
                }
            });
        }
    });
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&at, 1_500), "research", here(&made)).unwrap();
    let second = vec![0.1; SECOND as usize];
    for _ in 0..60 {
        t.feed(&second).unwrap();
    }
    assert!(!t.is_local(), "a minute is kept");
    t.feed(&second).unwrap();
    assert!(t.is_local(), "past a minute, this PC's engine");
    assert_eq!(t.finish().unwrap().settled, format!("L{}", 61 * SECOND));
}

/// One feed of a long stretch of sound goes out in pieces of at most a second: the worker refuses a frame past 16 s.
#[test]
fn a_long_feed_goes_out_in_pieces() {
    let m = mock(None, false);
    let made = Arc::new(AtomicUsize::new(0));
    let mut t = OnTower::start(tower(&m.at, 1_500), "research", here(&made)).unwrap();
    let mut words = t.feed(&vec![0.1; 20 * SECOND as usize]).unwrap().settled;
    words.push_str(&t.finish().unwrap().settled);
    assert_eq!(words.split_whitespace().count(), 20, "{words}");
    assert_eq!(m.longest.load(Ordering::Relaxed), PIECE * 2);
}

/// Each take holds its own place in the count of takes on the tower: one ending leaves the other's standing.
#[test]
fn a_take_ending_leaves_anothers_place() {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let (a, b) = (Serving::on(&COUNT), Serving::on(&COUNT));
    drop(a);
    assert_eq!(COUNT.load(Ordering::Relaxed), 1);
    drop(b);
    assert_eq!(COUNT.load(Ordering::Relaxed), 0);
}
