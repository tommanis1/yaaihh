//! Speech into the one message box, recognised on his machine (the owner, N-541.p13; paper-research N-554). The engine
//! side: a take runs on a thread of its own, from a sound source through 16 kHz mono to a recogniser, and sends what it
//! heard as `Heard`; the box (docket-edge2's, N-554.p5) puts settled words in at the caret and draws the tail as a
//! composition.
//!
//! - **The seam:** `start(key)` gives the take's id; `stop()` ends it; `poll()` gives what came, each with the box it was
//!   started from and its take, so words never land in another box and a late word of a take he dropped is dropped.
//! - **Sound** never touches the disk; it leaves the exe only for his own tower over the tailnet, when speech.toml
//!   names it (tower.rs). Only text leaves the take thread. No log line holds recognised words.
//! - **The microphone** is opened only by a take, which only his press starts. Until the engine and its model are in,
//!   a take runs the stand-in: set words at set times over silence (`YAAIHH_SPEECH_STANDIN`), no device opened.

pub mod model;
#[cfg(windows)]
pub mod mic;
#[cfg(feature = "speech-engine")]
pub mod engine;
pub mod tower;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What a take heard, in the order it heard it.
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    /// The words not settled yet, whole each time: it replaces the tail before it.
    Tail(String),
    /// Words that will not change, to go in at the caret.
    Settled(String),
    /// The take is over: his press, the source's end, or a failure just before it.
    End,
    /// Why there was no take, or why it stopped, in words for the line at the box. An `End` always follows.
    Failed(String),
}

/// The rate every recogniser takes: 16 kHz, mono, f32 in [-1, 1].
pub const RATE: u32 = 16_000;

/// The processor floor of the engine's build (GGML_NATIVE off, AVX2, FMA and F16C on; N-554.p7): without them it would
/// stop on its first instruction, so a take says so in words instead.
pub const FLOOR: &str = "AVX2, FMA and F16C";

/// Whether this processor runs the engine's build.
pub fn processor_ok() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") && std::arch::is_x86_feature_detected!("f16c")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// What a recogniser has made of the sound fed so far: words newly settled, and the tail now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Words {
    pub settled: String,
    pub tail: String,
}

/// A recogniser: fed 16 kHz mono, it says what settled and what is still open.
pub trait Recogniser: Send {
    fn feed(&mut self, pcm: &[f32]) -> Result<Words, String>;
    /// The take is over: everything left settles.
    fn finish(&mut self) -> Result<Words, String>;
}

/// A recogniser that shows its whole text each time, settled part and open part, made into `Words`: what its settled
/// part grew by since the last time, and its open part as the tail.
#[derive(Default)]
pub struct Said {
    settled: String,
}

impl Said {
    pub fn next(&mut self, settled: &str, open: &str) -> Words {
        let grew = match settled.strip_prefix(self.settled.as_str()) {
            Some(more) => more.to_string(),
            // a settled part rewritten (it should never be): what went in stays, and only words past it go in
            None if settled.len() > self.settled.len() && settled.is_char_boundary(self.settled.len()) => settled[self.settled.len()..].to_string(),
            None => String::new(),
        };
        if !grew.is_empty() {
            self.settled = settled.to_string();
        }
        Words { settled: grew, tail: open.to_string() }
    }

    /// The take is over: the open part settles as it stands.
    pub fn last(&mut self, settled: &str, open: &str) -> Words {
        let mut w = self.next(settled, "");
        w.settled.push_str(open);
        self.settled.push_str(open);
        w
    }
}

/// The stand-in recogniser (paper-research N-554.p1): set words at set times of sound fed, whatever the sound is.
/// Each step is (seconds of sound, words that settle then, the tail from then on).
pub struct StandIn {
    steps: Vec<(f32, String, String)>,
    fed: usize,
    next: usize,
}

impl StandIn {
    pub fn new(steps: Vec<(f32, String, String)>) -> StandIn {
        StandIn { steps, fed: 0, next: 0 }
    }

    /// From `YAAIHH_SPEECH_STANDIN`'s form: steps split by `;`, each `SECONDS|SETTLED|TAIL`.
    pub fn parse(s: &str) -> Result<StandIn, String> {
        let mut steps = Vec::new();
        for step in s.split(';').filter(|x| !x.trim().is_empty()) {
            let mut parts = step.splitn(3, '|');
            let at = parts.next().unwrap_or("").trim().parse::<f32>().map_err(|_| format!("not a time in the stand-in's step {step:?}"))?;
            steps.push((at, parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string()));
        }
        Ok(StandIn::new(steps))
    }
}

impl Recogniser for StandIn {
    fn feed(&mut self, pcm: &[f32]) -> Result<Words, String> {
        self.fed += pcm.len();
        let now = self.fed as f32 / RATE as f32;
        let mut w = Words::default();
        let mut moved = false;
        while let Some((at, settled, tail)) = self.steps.get(self.next)
            && *at <= now
        {
            w.settled.push_str(settled);
            w.tail = tail.clone();
            self.next += 1;
            moved = true;
        }
        if !moved && self.next > 0 {
            w.tail = self.steps[self.next - 1].2.clone();
        }
        Ok(w)
    }

    fn finish(&mut self) -> Result<Words, String> {
        // what was open settles as it stands; steps not reached never were said
        let tail = if self.next > 0 { self.steps[self.next - 1].2.clone() } else { String::new() };
        self.next = self.steps.len();
        Ok(Words { settled: tail, tail: String::new() })
    }
}

/// Sound as a device gives it, any rate and channel count, made 16 kHz mono: each frame's channels averaged, then each
/// output sample the mean of the input samples in its span (a box filter that keeps what lies under 8 kHz, which is
/// all a recogniser reads); from a device slower than 16 kHz each sample is held. Counted in whole samples, so a
/// second in is a second out, and fed in pieces of any size it gives what one whole feed would.
pub struct ToMono16k {
    rate: u64,
    channels: usize,
    /// mono samples taken, and output samples given, so far
    taken: u64,
    given: u64,
    sum: f32,
    n: u32,
    /// a frame cut by the end of a piece
    part: Vec<f32>,
}

impl ToMono16k {
    pub fn new(rate: u32, channels: u16) -> ToMono16k {
        ToMono16k { rate: rate.max(1) as u64, channels: channels.max(1) as usize, taken: 0, given: 0, sum: 0.0, n: 0, part: Vec::new() }
    }

    fn take(&mut self, s: f32, out: &mut Vec<f32>) {
        self.sum += s;
        self.n += 1;
        self.taken += 1;
        let due = self.taken * RATE as u64 / self.rate;
        if due > self.given {
            let mean = self.sum / self.n as f32;
            for _ in self.given..due {
                out.push(mean);
            }
            self.given = due;
            self.sum = 0.0;
            self.n = 0;
        }
    }

    /// Interleaved samples in; 16 kHz mono out.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<f32> {
        let c = self.channels;
        let mut out = Vec::with_capacity(interleaved.len() / c * RATE as usize / self.rate as usize + 2);
        let mut rest = interleaved;
        if !self.part.is_empty() {
            let need = (c - self.part.len()).min(rest.len());
            self.part.extend_from_slice(&rest[..need]);
            rest = &rest[need..];
            if self.part.len() == c {
                let s = self.part.iter().sum::<f32>() / c as f32;
                self.part.clear();
                self.take(s, &mut out);
            }
        }
        let whole = rest.len() / c * c;
        for frame in rest[..whole].chunks_exact(c) {
            self.take(frame.iter().sum::<f32>() / c as f32, &mut out);
        }
        self.part.extend_from_slice(&rest[whole..]);
        out
    }
}

/// Where a take's sound comes from: interleaved f32 at a rate, in pieces.
pub trait Source {
    fn rate(&self) -> u32;
    fn channels(&self) -> u16;
    /// The next piece, waiting at most WAIT; `Ok(None)` when nothing came yet, `Err` when the source ended (with why,
    /// or empty for a plain end).
    fn next(&mut self, wait: Duration) -> Result<Option<Vec<f32>>, String>;
    /// The pieces it has already given and the take has not read yet, taken once at his stop without waiting: what he
    /// said before it, fed before the take finishes (consult-2, N-585.p20). Only what is there then: a device still
    /// giving cannot keep a take from ending (N-585.p24).
    fn queued(&mut self) -> Vec<Vec<f32>> {
        Vec::new()
    }
}

/// Sound made in code (tests, the stand-in): pieces of it, as fast as asked, or paced as a device would give them.
pub struct Samples {
    rate: u32,
    channels: u16,
    left: std::collections::VecDeque<Vec<f32>>,
    paced: bool,
}

impl Samples {
    /// SAMPLES (interleaved) given in pieces of PIECE frames; PACED: each piece only after its own length of time.
    pub fn new(rate: u32, channels: u16, samples: Vec<f32>, piece: usize, paced: bool) -> Samples {
        let per = piece.max(1) * channels.max(1) as usize;
        Samples { rate, channels, left: samples.chunks(per).map(<[f32]>::to_vec).collect(), paced }
    }

    /// SECONDS of silence at 48 kHz stereo, in 10 ms pieces, paced: the stand-in's take, with no device opened.
    pub fn silence(seconds: f32, paced: bool) -> Samples {
        Samples::new(48_000, 2, vec![0.0; (seconds * 48_000.0) as usize * 2], 480, paced)
    }
}

impl Source for Samples {
    fn rate(&self) -> u32 {
        self.rate
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn next(&mut self, wait: Duration) -> Result<Option<Vec<f32>>, String> {
        let Some(p) = self.left.pop_front() else { return Err(String::new()) };
        if self.paced {
            std::thread::sleep(Duration::from_secs_f64(p.len() as f64 / self.channels as f64 / self.rate as f64).min(wait));
        }
        Ok(Some(p))
    }
}

/// One take, run to its end on the calling thread: SOURCE through `ToMono16k` into REC, each change sent as `Heard`
/// on SEND, until STOP is set or the source ends; then what is left settles and `End` goes. A failure is said once,
/// then `End`.
pub fn run_take(source: &mut dyn Source, rec: &mut dyn Recogniser, stop: &AtomicBool, send: &mut dyn FnMut(Heard)) {
    let mut mono = ToMono16k::new(source.rate(), source.channels());
    let mut tail = String::new();
    let said = |w: Words, send: &mut dyn FnMut(Heard), tail: &mut String| {
        if !w.settled.is_empty() {
            send(Heard::Settled(w.settled));
        }
        if w.tail != *tail {
            *tail = w.tail.clone();
            send(Heard::Tail(w.tail));
        }
    };
    let failed = loop {
        if stop.load(Ordering::Relaxed) {
            // what he said before his stop and the take had not read yet still goes in
            let mut failed = None;
            for piece in source.queued() {
                let pcm = mono.push(&piece);
                if pcm.is_empty() {
                    continue;
                }
                match rec.feed(&pcm) {
                    Ok(w) => said(w, send, &mut tail),
                    Err(e) => {
                        failed = Some(e);
                        break;
                    }
                }
            }
            break failed;
        }
        match source.next(Duration::from_millis(50)) {
            Ok(Some(piece)) => {
                let pcm = mono.push(&piece);
                if pcm.is_empty() {
                    continue;
                }
                match rec.feed(&pcm) {
                    Ok(w) => said(w, send, &mut tail),
                    Err(e) => break Some(e),
                }
            }
            Ok(None) => {}
            Err(why) if why.is_empty() => break None,
            Err(why) => break Some(why),
        }
    };
    match failed {
        Some(why) => send(Heard::Failed(why)),
        None => match rec.finish() {
            Ok(w) => said(w, send, &mut tail),
            Err(e) => send(Heard::Failed(e)),
        },
    }
    send(Heard::End);
}

/// The take running, if one is: its stop.
struct Running {
    stop: Arc<AtomicBool>,
}

struct Speech {
    running: Option<Running>,
    send: Sender<(String, u64, Heard)>,
    came: Receiver<(String, u64, Heard)>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

static TAKES: AtomicU64 = AtomicU64::new(0);

fn speech() -> &'static Mutex<Speech> {
    static S: std::sync::OnceLock<Mutex<Speech>> = std::sync::OnceLock::new();
    S.get_or_init(|| {
        let (send, came) = channel();
        Mutex::new(Speech { running: None, send, came, wake: None })
    })
}

/// The window woken to `poll`: for what speech has besides a take's words (the model's fetch).
pub fn wake() {
    let w = speech().lock().unwrap().wake.clone();
    if let Some(w) = w {
        w();
    }
}

/// Whether this build takes speech at all: the engine is in it, or the stand-in is set. Without, no mic is shown.
pub fn shown() -> bool {
    cfg!(feature = "speech-engine") || std::env::var_os("YAAIHH_SPEECH_STANDIN").is_some()
}

/// Whether the engine's model is still being loaded (the first take of a run): the box says so until it is in.
pub fn model_loading() -> bool {
    #[cfg(feature = "speech-engine")]
    {
        std::env::var_os("YAAIHH_SPEECH_STANDIN").is_none() && !engine::LOADED.load(Ordering::Relaxed) && tower::SERVING.load(Ordering::Relaxed) == 0
    }
    #[cfg(not(feature = "speech-engine"))]
    {
        false
    }
}

/// Whether a press must first offer the model: a build with the engine, not the stand-in, no tower, and the model not
/// here.
pub fn needs_model() -> bool {
    cfg!(feature = "speech-engine") && std::env::var_os("YAAIHH_SPEECH_STANDIN").is_none() && offers_model(&tower::config(), model_here())
}

fn model_here() -> bool {
    model::present(&model::folder(), &model::PARAKEET).is_some()
}

/// Whether a press offers this PC's model, given speech.toml's TOWER: never with a tower (its takes go there, this
/// PC's engine only behind it) or a speech.toml that does not read (the press says why).
fn offers_model(tower: &Result<Option<tower::Tower>, String>, here: bool) -> bool {
    matches!(tower, Ok(None)) && !here
}

/// Why this PC's own engine cannot run a take, if it cannot: the processor below its floor, or no model yet.
#[cfg(any(feature = "speech-engine", test))]
fn here_lacks(processor: bool, here: bool) -> Option<String> {
    if !processor {
        Some(format!("this processor lacks {FLOOR}"))
    } else if !here {
        Some("its model is not on this machine".into())
    } else {
        None
    }
}

/// The window's waker: each `Heard` wakes it to `poll`.
pub fn set_waker(wake: Arc<dyn Fn() + Send + Sync>) {
    speech().lock().unwrap().wake = Some(wake);
}

/// Start a take for the box KEY of PROJECT (its id, for the tower's vocab); its id. A take already running is stopped
/// first (one microphone, one take).
pub fn start(key: &str, project: &str) -> Result<u64, String> {
    let (with, source) = parts(project)?;
    let mut s = speech().lock().unwrap();
    if let Some(r) = s.running.take() {
        r.stop.store(true, Ordering::Relaxed);
    }
    let take = TAKES.fetch_add(1, Ordering::Relaxed) + 1;
    let stop = Arc::new(AtomicBool::new(false));
    s.running = Some(Running { stop: stop.clone() });
    let (send, wake, key) = (s.send.clone(), s.wake.clone(), key.to_string());
    std::thread::Builder::new()
        .name("speech".into())
        .spawn(move || {
            let mut out = |h: Heard| {
                let _ = send.send((key.clone(), take, h));
                if let Some(w) = &wake {
                    w();
                }
            };
            match source() {
                Ok(mut src) => {
                    let ran = with(&mut |rec| run_take(src.as_mut(), rec, &stop, &mut out));
                    if let Err(why) = ran {
                        out(Heard::Failed(why));
                        out(Heard::End);
                    }
                }
                Err(why) => {
                    out(Heard::Failed(why));
                    out(Heard::End);
                }
            }
        })
        .map_err(|e| format!("speech: no thread for the take: {e}"))?;
    Ok(take)
}

/// Stop the take running, if any: what it heard settles, then its `End` comes through `poll`.
pub fn stop() {
    if let Some(r) = speech().lock().unwrap().running.take() {
        r.stop.store(true, Ordering::Relaxed);
    }
}

/// What came since the last poll: (box, take, what it heard).
pub fn poll() -> Vec<(String, u64, Heard)> {
    let s = speech().lock().unwrap();
    s.came.try_iter().collect()
}

/// A take's source, made on the take's own thread (a device's stream stays on the thread that opened it).
type SourceMaker = Box<dyn FnOnce() -> Result<Box<dyn Source>, String> + Send>;

/// Runs a take with its recogniser, made on the take's own thread (the engine's stream borrows a session there); why
/// there is none, in words.
type WithRecogniser = Box<dyn FnOnce(&mut dyn FnMut(&mut dyn Recogniser)) -> Result<(), String> + Send>;

/// The recogniser and the source a take runs: the stand-in over silence while `YAAIHH_SPEECH_STANDIN` is set; else the
/// engine over the microphone, in a build with `speech-engine`: on the tower when speech.toml names one, with this PC's
/// own engine behind it.
fn parts(project: &str) -> Result<(WithRecogniser, SourceMaker), String> {
    if let Ok(script) = std::env::var("YAAIHH_SPEECH_STANDIN") {
        let mut rec = StandIn::parse(&script)?;
        let last = rec.steps.last().map_or(1.0, |s| s.0) + 0.5;
        let with: WithRecogniser = Box::new(move |go| {
            go(&mut rec);
            Ok(())
        });
        return Ok((with, Box::new(move || Ok(Box::new(Samples::silence(last, true)) as Box<dyn Source>))));
    }
    // with a tower the processor matters only if the take falls back here
    if !processor_ok() && matches!(tower::config(), Ok(None)) {
        return Err(format!("Speech needs {FLOOR}."));
    }
    engine_parts(project)
}

#[cfg(feature = "speech-engine")]
fn engine_parts(project: &str) -> Result<(WithRecogniser, SourceMaker), String> {
    let with: WithRecogniser = match tower::config()? {
        Some(cfg) => {
            let project = project.to_string();
            Box::new(move |go| {
                // this PC's session only once the take falls back to it: its model is not loaded otherwise
                let mut session = None;
                let slot = &mut session;
                let local: tower::Local = Box::new(move || {
                    if let Some(why) = here_lacks(processor_ok(), model_here()) {
                        return Err(why);
                    }
                    Ok(Box::new(engine::Engine::new(slot.insert(engine::session()?))?) as Box<dyn Recogniser>)
                });
                let mut rec = tower::OnTower::start(cfg, &project, local)?;
                go(&mut rec);
                Ok(())
            })
        }
        None => Box::new(|go| {
            let mut session = engine::session()?;
            let mut rec = engine::Engine::new(&mut session)?;
            go(&mut rec);
            Ok(())
        }),
    };
    #[cfg(windows)]
    let source: SourceMaker = Box::new(|| Ok(Box::new(mic::Mic::open()?) as Box<dyn Source>));
    #[cfg(not(windows))]
    let source: SourceMaker = Box::new(|| Err("Speech hears only on Windows.".into()));
    Ok((with, source))
}

#[cfg(not(feature = "speech-engine"))]
fn engine_parts(_project: &str) -> Result<(WithRecogniser, SourceMaker), String> {
    Err("Speech is not in this build yet.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, channels: u16, hz: f32, seconds: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n).flat_map(|i| std::iter::repeat_n((2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin() * 0.5, channels as usize)).collect()
    }

    fn crossings(x: &[f32]) -> usize {
        x.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count()
    }

    /// With a tower in speech.toml a press goes to it: no model is offered and no processor floor holds it; this PC's
    /// engine behind it says why it cannot take over (the floor first: below it the engine would stop the window).
    #[test]
    fn a_tower_takes_the_press_without_this_pcs_model_or_processor() {
        let t = tower::Tower { at: "tower:8765".into(), fallback_ms: 1_500, finish_ms: 15_000, quiet_ms: 3_000, token: String::new() };
        assert!(!offers_model(&Ok(Some(t)), false), "a tower: nothing to fetch first");
        assert!(offers_model(&Ok(None), false));
        assert!(!offers_model(&Ok(None), true));
        assert!(!offers_model(&Err("speech.toml does not read".into()), false), "the press says why instead");
        assert_eq!(here_lacks(false, true).as_deref(), Some("this processor lacks AVX2, FMA and F16C"));
        assert_eq!(here_lacks(false, false).as_deref(), Some("this processor lacks AVX2, FMA and F16C"));
        assert_eq!(here_lacks(true, false).as_deref(), Some("its model is not on this machine"));
        assert_eq!(here_lacks(true, true), None);
    }

    /// A whole-text recogniser's growth settles once, its open part is the tail, and at the end the open part settles;
    /// a rewritten settled part never takes back what went in.
    #[test]
    fn a_growing_text_settles_only_what_is_new() {
        let mut s = Said::default();
        assert_eq!(s.next("", "hel"), Words { settled: "".into(), tail: "hel".into() });
        assert_eq!(s.next("Hello", " wor"), Words { settled: "Hello".into(), tail: " wor".into() });
        assert_eq!(s.next("Hello", " world"), Words { settled: "".into(), tail: " world".into() });
        assert_eq!(s.next("Hello world,", " how"), Words { settled: " world,".into(), tail: " how".into() });
        assert_eq!(s.next("Hallo world, how", ""), Words { settled: " how".into(), tail: "".into() });
        assert_eq!(s.last("Hallo world, how", " are you"), Words { settled: " are you".into(), tail: "".into() });
        assert_eq!(s.next("Hallo world, how are you", ""), Words::default());
    }

    /// A stop feeds what the source already gave before the take finishes: words said just before it are not lost, and
    /// a device that keeps giving cannot keep the take from ending.
    #[test]
    fn a_stop_feeds_what_the_source_already_gave() {
        struct Gave(Vec<Vec<f32>>);
        impl Source for Gave {
            fn rate(&self) -> u32 {
                RATE
            }
            fn channels(&self) -> u16 {
                1
            }
            fn next(&mut self, _: Duration) -> Result<Option<Vec<f32>>, String> {
                Ok(None)
            }
            fn queued(&mut self) -> Vec<Vec<f32>> {
                // a device that keeps giving: what is there at the stop, and as much again each time it is asked
                self.0.clone()
            }
        }
        let stop = AtomicBool::new(true);
        let mut got = Vec::new();
        let mut rec = StandIn::new(vec![(0.5, "said before the stop".into(), String::new())]);
        run_take(&mut Gave(vec![vec![0.0; 8_000], vec![0.0; 8_000]]), &mut rec, &stop, &mut |h| got.push(h));
        assert_eq!(got, vec![Heard::Settled("said before the stop".into()), Heard::End]);
    }

    /// A device's 48 kHz stereo, 44.1 kHz mono and 16 kHz come out 16 kHz mono, a tone keeping its pitch, and pieces of
    /// any size giving what one whole feed gives.
    #[test]
    fn sound_is_made_16k_mono_whatever_the_device_gives() {
        for (rate, ch) in [(48_000, 2), (44_100, 1), (16_000, 1), (96_000, 4)] {
            let x = tone(rate, ch, 440.0, 1.0);
            let whole = ToMono16k::new(rate, ch).push(&x);
            assert!((whole.len() as i64 - 16_000).abs() <= 1, "{rate}/{ch}: {}", whole.len());
            let c = crossings(&whole);
            assert!((878..=882).contains(&c), "{rate}/{ch}: a 440 Hz tone keeps its pitch: {c} crossings");
            let mut m = ToMono16k::new(rate, ch);
            let mut pieces = Vec::new();
            for (i, p) in x.chunks(37 + (rate as usize % 11)).enumerate() {
                pieces.extend(m.push(p));
                if i % 5 == 0 {
                    pieces.extend(m.push(&[]));
                }
            }
            assert_eq!(pieces, whole, "{rate}/{ch}: pieces cut mid-frame give the same");
        }
        let quiet = ToMono16k::new(48_000, 2).push(&[0.0; 9600]);
        assert!(quiet.len() == 1600 && quiet.iter().all(|s| *s == 0.0));
        // a device slower than 16 kHz: each sample held
        let up = ToMono16k::new(8_000, 1).push(&[0.25, -0.5]);
        assert_eq!(up, vec![0.25, 0.25, -0.5, -0.5]);
    }

    fn take(script: &str, seconds: f32, stop_after: Option<usize>) -> Vec<Heard> {
        let mut rec = StandIn::parse(script).unwrap();
        let mut src = Samples::silence(seconds, false);
        let stop = AtomicBool::new(false);
        let mut got = Vec::new();
        run_take(&mut src, &mut rec, &stop, &mut |h| {
            got.push(h);
            if Some(got.len()) == stop_after {
                stop.store(true, Ordering::Relaxed);
            }
        });
        got
    }

    /// The stand-in's words come as the seam says: the tail whole each time it changes, settled words once, what was
    /// open settling at the end, then `End`; a take stopped early ends at once with what was open settled.
    #[test]
    fn a_take_sends_tails_settled_words_and_its_end() {
        let script = "0.5||I think;1.0|I think |it is wa;1.5|it is |wau";
        assert_eq!(
            take(script, 2.0, None),
            vec![
                Heard::Tail("I think".into()),
                Heard::Settled("I think ".into()),
                Heard::Tail("it is wa".into()),
                Heard::Settled("it is ".into()),
                Heard::Tail("wau".into()),
                Heard::Settled("wau".into()),
                Heard::Tail(String::new()),
                Heard::End,
            ]
        );
        assert_eq!(take(script, 2.0, Some(1)), vec![Heard::Tail("I think".into()), Heard::Settled("I think".into()), Heard::Tail(String::new()), Heard::End]);
        assert_eq!(take("", 0.3, None), vec![Heard::End], "nothing said: only the end");
    }

    /// A recogniser's failure is said once, in words, and the end follows; so is a source that ends with a why.
    #[test]
    fn a_failure_is_said_once_then_the_end() {
        struct Broken;
        impl Recogniser for Broken {
            fn feed(&mut self, _: &[f32]) -> Result<Words, String> {
                Err("the model could not be read".into())
            }
            fn finish(&mut self) -> Result<Words, String> {
                unreachable!()
            }
        }
        let stop = AtomicBool::new(false);
        let mut got = Vec::new();
        run_take(&mut Samples::silence(1.0, false), &mut Broken, &stop, &mut |h| got.push(h));
        assert_eq!(got, vec![Heard::Failed("the model could not be read".into()), Heard::End]);
        struct Gone;
        impl Source for Gone {
            fn rate(&self) -> u32 {
                48_000
            }
            fn channels(&self) -> u16 {
                1
            }
            fn next(&mut self, _: Duration) -> Result<Option<Vec<f32>>, String> {
                Err("the microphone went away".into())
            }
        }
        let mut got = Vec::new();
        run_take(&mut Gone, &mut StandIn::new(vec![]), &stop, &mut |h| got.push(h));
        assert_eq!(got, vec![Heard::Failed("the microphone went away".into()), Heard::End]);
    }
}
