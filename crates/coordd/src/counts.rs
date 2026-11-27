//! What coordd's time goes to, a minute at a time (paper-research N-101 p1797): per handled request kind, emit kind and
//! clock round, how many and the total milliseconds, the wait for the core lock among them shown apart. Once a
//! minute one line in coordd's log with the five largest by time, and none in a minute where nothing passed 100 ms.
//! It goes to the log and nowhere else.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per kind: how many, the total time, the time of it spent waiting for the core lock, and the time it held the lock
/// (a round's total also counts what it does off the lock: the follow round's transcript reads; a2h-speed N-560 p39).
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Count {
    pub n: u64,
    pub total: Duration,
    pub wait: Duration,
    pub held: Duration,
}

static COUNTS: Mutex<Option<HashMap<String, Count>>> = Mutex::new(None);

/// Bytes read from transcripts this minute, by reader (paper-research N-101 p1823).
static BYTES: Mutex<Option<HashMap<&'static str, u64>>> = Mutex::new(None);

/// Note N bytes read from a transcript by READER.
pub fn read(reader: &'static str, n: u64) {
    if n == 0 {
        return;
    }
    let mut g = BYTES.lock().unwrap_or_else(|e| e.into_inner());
    *g.get_or_insert_with(HashMap::new).entry(reader).or_default() += n;
}

/// The follow round's hold this minute, by part (paper-research N-412 p1506: which part of its 17–20 s a minute).
static PARTS: Mutex<Option<HashMap<&'static str, Duration>>> = Mutex::new(None);

/// The follow rounds this minute, and of them those that found the machine changed since the round before (what is
/// due is then read again, a pass over every ticket: `Tickets::machine_due`); with the hand-over's own counts.
static ROUNDS: Mutex<Rounds> = Mutex::new(Rounds { all: 0, changed: 0, hand: Hand { skipped: 0, ran: 0, none: 0, swept: 0, missed: 0 } });

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Rounds {
    all: u64,
    changed: u64,
    hand: Hand,
}

/// The hand-over this minute (paper-research N-1446 p3381): the rounds it was skipped (nothing it reads had changed:
/// `Core::hand_key`), the rounds it ran and of them those that handed nothing, and the sweeps (run with nothing
/// changed, now and then) and of them those that handed something: an input `hand_key` does not count, which would
/// otherwise wait for the next sweep.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Hand {
    pub skipped: u64,
    pub ran: u64,
    pub none: u64,
    pub swept: u64,
    pub missed: u64,
}

/// One follow round, CHANGED when the machine had moved since the round before.
pub fn round_seen(changed: bool) {
    let mut g = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    g.all += 1;
    g.changed += changed as u64;
}

/// One follow round's hand-over: skipped (`ran` false), or run, as a SWEEP when nothing had changed, handing to
/// HANDED agents.
pub fn hand_round(ran: bool, sweep: bool, handed: usize) {
    let mut g = ROUNDS.lock().unwrap_or_else(|e| e.into_inner());
    let h = &mut g.hand;
    if !ran {
        h.skipped += 1;
        return;
    }
    h.ran += 1;
    h.none += (handed == 0) as u64;
    h.swept += sweep as u64;
    h.missed += (sweep && handed > 0) as u64;
}

/// What the core holds as it stands (paper-research N-1446 p2299: whether the heap in use flattens once the ring is
/// full): the watchers' ring's events and their bytes as JSON (`Core::emit`), and the model requests `Sent` keeps
/// (`Core::add_sent`). Not counts of the minute: the line says them as they are at its end.
static RING_HELD: AtomicU64 = AtomicU64::new(0);
static RING_BYTES: AtomicU64 = AtomicU64::new(0);
static SENT_HELD: AtomicU64 = AtomicU64::new(0);
/// The ring's sizing this minute (its one serialization an emit, under the core lock: paper-research N-1446 p2301):
/// how many events, and their time in nanoseconds.
static SIZED: AtomicU64 = AtomicU64::new(0);
static SIZING_NS: AtomicU64 = AtomicU64::new(0);

/// The ring after an emit: EVENTS held, BYTES their length as JSON; SIZING the time the emit's event took to size.
pub fn ring_now(events: usize, bytes: u64, sizing: Duration) {
    RING_HELD.store(events as u64, Ordering::Relaxed);
    RING_BYTES.store(bytes, Ordering::Relaxed);
    SIZED.fetch_add(1, Ordering::Relaxed);
    SIZING_NS.fetch_add(sizing.as_nanos() as u64, Ordering::Relaxed);
}

/// The minute's sizing, its count and time, the counts started again.
fn take_sizing() -> (u64, Duration) {
    (SIZED.swap(0, Ordering::Relaxed), Duration::from_nanos(SIZING_NS.swap(0, Ordering::Relaxed)))
}

/// `Sent` after a request is kept: N requests.
pub fn sent_now(n: usize) {
    SENT_HELD.store(n as u64, Ordering::Relaxed);
}

/// The line's piece for what the core holds: EVENTS and BYTES of the watchers' ring, the minute's sizing of SIZED
/// events in SIZING, SENT requests.
pub(crate) fn kept_line(events: usize, bytes: u64, sized: u64, sizing: Duration, sent: usize) -> String {
    format!(
        "; watchers' ring holds {events} events, {} MB as JSON; ring sizing {:.2} ms for {sized} events this minute; Sent holds {sent} requests",
        bytes / 1_000_000,
        sizing.as_secs_f64() * 1000.0
    )
}

/// Time marks through one hold: each [`Marks::mark`] counts the time since the last mark to the part it names.
pub struct Marks(Instant);

impl Marks {
    pub fn new() -> Marks {
        Marks(Instant::now())
    }

    /// The time since the last mark (or the start) counted to WHAT.
    pub fn mark(&mut self, what: &'static str) {
        let now = Instant::now();
        let mut g = PARTS.lock().unwrap_or_else(|e| e.into_inner());
        *g.get_or_insert_with(HashMap::new).entry(what).or_default() += now - self.0;
        self.0 = now;
    }
}

/// The parts' piece of the line: the six largest, when the largest took 100 ms or more this minute.
fn parts_line(parts: HashMap<&'static str, Duration>, rounds: Rounds) -> String {
    let mut v: Vec<(&str, Duration)> = parts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    if v.first().is_none_or(|(_, d)| *d < Duration::from_millis(100)) {
        return String::new();
    }
    let p: Vec<String> = v.iter().take(6).map(|(k, d)| format!("{k} {} ms", d.as_millis())).collect();
    let h = rounds.hand;
    format!(
        "; clock follow's hold by part: {}; {} of {} rounds after a machine change; hand-over skipped {} (nothing changed), run {}, of them {} handed nothing; swept {}, of them {} handed something",
        p.join(", "),
        rounds.changed,
        rounds.all,
        h.skipped,
        h.ran,
        h.none,
        h.swept,
        h.missed
    )
}

tokio::task_local! {
    /// The core lock's wait inside the timed piece running on this task (`Shared::lock` adds to it).
    static WAITED: std::cell::Cell<Duration>;
    /// The kind of the timed piece or clock round running on this task: who holds the lock when it is held long.
    static KIND: std::cell::RefCell<String>;
}

/// A hold of the core lock this long or longer is a line in coordd's log naming its holder (a2h-speed, N-518 p23:
/// who keeps every watch waiting 7–12 s after a coord-switch).
pub const HELD_LONG: Duration = Duration::from_secs(1);

/// The core lock held: a hold of [`HELD_LONG`] or more says, when it ends, how long and by what kind.
pub struct Held<'a, T> {
    g: tokio::sync::MutexGuard<'a, T>,
    at: Instant,
    /// Where the lock was taken: said for a hold no kind names (paper-research N-560 p29, 2).
    place: &'static std::panic::Location<'static>,
}

impl<T> std::ops::Deref for Held<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.g
    }
}

impl<T> std::ops::DerefMut for Held<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.g
    }
}

impl<T> Drop for Held<'_, T> {
    fn drop(&mut self) {
        let d = self.at.elapsed();
        // every hold counted to its kind, for the minute's line; a hold no kind names is counted as such
        let mut g = COUNTS.lock().unwrap_or_else(|e| e.into_inner());
        g.get_or_insert_with(HashMap::new).entry(holder()).or_default().held += d;
        drop(g);
        if d >= HELD_LONG {
            eprintln!("{}", held_line(d, &holder_at(self.place)));
        }
    }
}

/// The kind running on this task, or what says there is none.
fn holder() -> String {
    KIND.try_with(|k| k.borrow().clone()).ok().filter(|k| !k.is_empty()).unwrap_or_else(|| "an untimed piece".into())
}

/// [`holder`], and where the lock was taken when no kind runs: `an untimed piece (coordd/src/main.rs:933)`.
fn holder_at(place: &std::panic::Location<'_>) -> String {
    let h = holder();
    if KIND.try_with(|k| !k.borrow().is_empty()).unwrap_or(false) { h } else { format!("{h} ({}:{})", place.file().rsplit_once("crates/").map_or(place.file(), |(_, f)| f), place.line()) }
}

fn held_line(d: Duration, kind: &str) -> String {
    format!("coordd: the lock held {} ms by {kind}", d.as_millis())
}

/// Add one KIND that took TOTAL, WAIT of it for the lock.
pub fn add(kind: &str, total: Duration, wait: Duration) {
    let mut g = COUNTS.lock().unwrap_or_else(|e| e.into_inner());
    let c = g.get_or_insert_with(HashMap::new).entry(kind.to_string()).or_default();
    c.n += 1;
    c.total += total;
    c.wait += wait;
}

/// Note a lock wait of D in the timed piece this task runs, if it runs one.
pub fn waited(d: Duration) {
    let _ = WAITED.try_with(|w| w.set(w.get() + d));
}

/// Take lock M, its wait counted for what waits, a long hold said with its holder ([`Held`]).
/// Where it is taken is kept for the line ([`holder_at`]): the caller's, or its caller's when that is `#[track_caller]`.
#[track_caller]
pub fn lock<T>(m: &tokio::sync::Mutex<T>) -> impl std::future::Future<Output = Held<'_, T>> {
    let place = std::panic::Location::caller();
    async move {
        let at = Instant::now();
        let g = m.lock().await;
        waited(at.elapsed());
        Held { g, at: Instant::now(), place }
    }
}

/// Run F as one KIND, timed, its lock waits counted apart.
pub async fn timed<T>(kind: &str, f: impl std::future::Future<Output = T>) -> T {
    let at = Instant::now();
    let (out, wait) = KIND
        .scope(
            std::cell::RefCell::new(kind.to_string()),
            WAITED.scope(std::cell::Cell::new(Duration::ZERO), async {
                let out = f.await;
                (out, WAITED.with(|w| w.get()))
            }),
        )
        .await;
    add(kind, at.elapsed(), wait);
    out
}

/// Run F (a clock's loop) with a place for its lock waits, so each of its rounds (`Round`) counts them apart.
pub async fn scoped<T>(f: impl std::future::Future<Output = T>) -> T {
    KIND.scope(std::cell::RefCell::new(String::new()), WAITED.scope(std::cell::Cell::new(Duration::ZERO), f)).await
}

/// One round of a clock's loop run under `scoped`, counted as its kind when it is dropped (at the round's end, or at a
/// `continue` out of it).
pub struct Round {
    kind: &'static str,
    at: Instant,
}

impl Round {
    pub fn new(kind: &'static str) -> Round {
        let _ = WAITED.try_with(|w| w.set(Duration::ZERO));
        let _ = KIND.try_with(|k| *k.borrow_mut() = kind.to_string());
        Round { kind, at: Instant::now() }
    }
}

impl Drop for Round {
    fn drop(&mut self) {
        let wait = WAITED.try_with(|w| w.replace(Duration::ZERO)).unwrap_or_default();
        add(self.kind, self.at.elapsed(), wait);
    }
}

/// Time a piece of synchronous work as KIND (no lock wait inside: it already holds what it needs).
pub fn timed_sync<T>(kind: &str, f: impl FnOnce() -> T) -> T {
    let at = Instant::now();
    let out = f();
    add(kind, at.elapsed(), Duration::ZERO);
    out
}

/// The minute's line: the five largest by time, or none when nothing passed 100 ms; the counts start again.
pub fn take_line() -> Option<String> {
    let all = COUNTS.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let bytes = BYTES.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let parts = PARTS.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let rounds = std::mem::take(&mut *ROUNDS.lock().unwrap_or_else(|e| e.into_inner()));
    line(all, bytes).map(|l| format!("{l}{}", parts_line(parts, rounds)))
}

/// The memory part of the line, read now: `/proc/self/smaps` walks every mapping (about 27 ms on research's coordd
/// at 1.2 GB), so the minute's task reads it on the blocking pool, holding no lock, not on a worker.
fn memory_now() -> String {
    let read = |f: &str| std::fs::read_to_string(f).unwrap_or_default();
    let m = memory(&read("/proc/self/status"), &read("/proc/self/smaps"));
    format!("{m}{}", malloc_info_now().map(|x| allocator(&x)).unwrap_or_default())
}

/// glibc's own account of its arenas (`malloc_info(3)`'s XML), read now; None where it cannot be read or this is not
/// glibc. It takes each arena's lock in turn while it counts that arena's free chunks.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub(crate) fn malloc_info_now() -> Option<String> {
    let mut buf: *mut libc::c_char = std::ptr::null_mut();
    let mut len: libc::size_t = 0;
    // SAFETY: open_memstream sets BUF and LEN at fclose; BUF is malloc'd by it and freed here once copied
    unsafe {
        let f = libc::open_memstream(&mut buf, &mut len);
        if f.is_null() {
            return None;
        }
        let rc = libc::malloc_info(0, f);
        libc::fclose(f);
        if buf.is_null() {
            return None;
        }
        let s = String::from_utf8_lossy(std::slice::from_raw_parts(buf as *const u8, len)).into_owned();
        libc::free(buf as *mut libc::c_void);
        (rc == 0).then_some(s)
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub(crate) fn malloc_info_now() -> Option<String> {
    None
}

/// The allocator's part of the line (paper-research N-412 p1506, 2: "freed memory kept in arenas" read, not guessed),
/// from malloc_info's XML: how many arenas; what they hold from the system, of it what is in use and what is free
/// and kept (fast and other free chunks, the top chunk among them); what lies in blocks mapped alone; and the five
/// arenas that hold most, each as held/free MB. Empty when the XML has no arena.
pub(crate) fn allocator(xml: &str) -> String {
    let attr = |l: &str, k: &str| l.split(&format!("{k}=\"")).nth(1).and_then(|r| r.split('"').next()).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
    let mut arenas: Vec<(u64, u64)> = Vec::new(); // (held from the system, free in it), bytes
    let mut mapped = 0;
    let mut cur: Option<(u64, u64)> = None;
    for l in xml.lines().map(str::trim) {
        if l.starts_with("<heap nr=") {
            cur = Some((0, 0));
        } else if l.starts_with("</heap>") {
            arenas.extend(cur.take());
        } else if let Some(c) = cur.as_mut() {
            if l.starts_with("<total type=\"fast\"") || l.starts_with("<total type=\"rest\"") {
                c.1 += attr(l, "size");
            } else if l.starts_with("<system type=\"current\"") {
                c.0 = attr(l, "size");
            }
        } else if l.starts_with("<total type=\"mmap\"") {
            mapped = attr(l, "size");
        }
    }
    if arenas.is_empty() {
        return String::new();
    }
    let mb = |b: u64| b / (1024 * 1024);
    let (held, free) = arenas.iter().fold((0, 0), |(h, f), (a, b)| (h + a, f + b));
    let mut big = arenas.clone();
    big.sort_by(|a, b| b.0.cmp(&a.0));
    let top: Vec<String> = big.iter().take(5).map(|(h, f)| format!("{}/{}", mb(*h), mb(*f))).collect();
    format!(
        "; malloc: {} arenas hold {} MB, {} MB in use, {} MB free and kept; {} MB in blocks mapped alone; largest arenas held/free MB {}",
        arenas.len(),
        mb(held),
        mb(held.saturating_sub(free)),
        mb(free),
        mb(mapped),
        top.join(", ")
    )
}

/// The line's last part (paper-research N-560 p56: so a switch has a before and an after): the resident size, and of
/// it the main heap, which is what the core's own data grows, apart from the threads' arenas that fill to a plateau.
/// From STATUS (`/proc/self/status`) and SMAPS (`/proc/self/smaps`); empty where neither reads.
fn memory(status: &str, smaps: &str) -> String {
    let kb = |l: &str| l.split_whitespace().nth(1).and_then(|n| n.parse::<u64>().ok());
    let Some(rss) = status.lines().find(|l| l.starts_with("VmRSS:")).and_then(kb) else {
        return String::new();
    };
    let mut heap = None;
    let mut in_heap = false;
    for l in smaps.lines() {
        if l.split_whitespace().next().is_some_and(|w| w.contains('-') && w.chars().all(|c| c.is_ascii_hexdigit() || c == '-')) {
            in_heap = l.trim_end().ends_with("[heap]");
        } else if in_heap && l.starts_with("Rss:") {
            heap = kb(l);
        }
    }
    match heap {
        Some(h) => format!("; resident {} MB, main heap {} MB", rss / 1024, h / 1024),
        None => format!("; resident {} MB", rss / 1024),
    }
}

/// The line, with the transcripts' bytes by reader after it when any reader read 1 MB or more (that alone writes it).
fn line(all: HashMap<String, Count>, bytes: HashMap<&'static str, u64>) -> Option<String> {
    let mut v: Vec<(String, Count)> = all.into_iter().collect();
    // by the larger of its time and its hold: a hold no timed piece names counts too
    let big = |c: &Count| c.total.max(c.held);
    v.sort_by(|a, b| big(&b.1).cmp(&big(&a.1)).then(a.0.cmp(&b.0)));
    let mut b: Vec<(&str, u64)> = bytes.into_iter().filter(|(_, n)| *n >= 1_000_000).collect();
    b.sort_by(|x, y| y.1.cmp(&x.1).then(x.0.cmp(y.0)));
    if v.first().is_none_or(|(_, c)| big(c) < Duration::from_millis(100)) && b.is_empty() {
        return None;
    }
    let parts: Vec<String> = v
        .iter()
        .filter(|(_, c)| big(c) >= Duration::from_millis(100))
        .take(5)
        .map(|(k, c)| format!("{k} {}x {} ms (lock wait {} ms{})", c.n, c.total.as_millis(), c.wait.as_millis(), if c.held.is_zero() { String::new() } else { format!(", held {} ms", c.held.as_millis()) }))
        .collect();
    let mut out = format!("coordd: costs this minute: {}", if parts.is_empty() { "none over 100 ms".to_string() } else { parts.join("; ") });
    if !b.is_empty() {
        let r: Vec<String> = b.iter().map(|(k, n)| format!("{k} {} MB", n / 1_000_000)).collect();
        out.push_str(&format!("; transcripts read: {}", r.join(", ")));
    }
    Some(out)
}

/// Write the minute's line to coordd's log, every minute, for as long as coordd runs.
pub fn every_minute() {
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            // the minute's sizing is taken each minute, a line written or not
            let (sized, sizing) = take_sizing();
            if let Some(l) = take_line() {
                let m = tokio::task::spawn_blocking(memory_now).await.unwrap_or_default();
                let k = kept_line(RING_HELD.load(Ordering::Relaxed) as usize, RING_BYTES.load(Ordering::Relaxed), sized, sizing, SENT_HELD.load(Ordering::Relaxed) as usize);
                eprintln!("{l}{m}{k}");
            }
        }
    });
}

/// An event's kind, as `coord watch` names it.
pub fn body_kind(b: &coord_types::Body) -> &'static str {
    use coord_types::Body::*;
    match b {
        Snapshot(_) => "snapshot",
        Agent(_) => "agent",
        Mail(_) => "mail",
        Delivered { .. } => "delivered",
        View(_) => "view",
        Refusals(_) => "refusals",
        Ticket(_) => "ticket",
        TicketChange { .. } => "ticketchange",
        Mentioned { .. } => "mentioned",
        Notices(_) => "notices",
        OwnerModel(_) => "ownermodel",
        Resumed { .. } => "resumed",
        Look(_) => "look",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_names_the_five_largest_and_is_silent_under_100_ms() {
        let ms = Duration::from_millis;
        let mut all = HashMap::new();
        all.insert("req show".to_string(), Count { n: 3, total: ms(99), wait: ms(0), held: ms(0) });
        assert_eq!(line(all.clone(), HashMap::new()), None, "nothing passed 100 ms");
        assert!(line(all.clone(), HashMap::from([("usage", 9_000_000_000)])).unwrap().ends_with("none over 100 ms; transcripts read: usage 9000 MB"));
        for (i, k) in ["req ls", "emit ticket", "loop hand", "req post", "loop clocks", "req mail"].iter().enumerate() {
            all.insert(k.to_string(), Count { n: 1, total: ms(100 + 10 * i as u64), wait: ms(5), held: ms(0) });
        }
        let l = line(all, HashMap::from([("span", 999_999), ("background", 2_000_000)])).unwrap();
        assert!(l.ends_with("; transcripts read: background 2 MB"), "{l}");
        assert!(l.starts_with("coordd: costs this minute: req mail 1x 150 ms (lock wait 5 ms); loop clocks 1x 140 ms"), "{l}");
        assert_eq!(l.matches(" ms (lock wait").count(), 5, "five only: {l}");
        assert!(!l.contains("req show") && !l.contains("req ls"), "{l}");
    }

    /// a2h-speed (paper-research N-560 p39): the minute's line says how long each kind held the lock, apart from its
    /// whole time (a follow round's total also counts its transcript reads off the lock); a hold no kind names shows.
    #[test]
    fn the_line_says_how_long_each_kind_held_the_lock() {
        let ms = Duration::from_millis;
        let mut all = HashMap::new();
        all.insert("clock follow (each second)".to_string(), Count { n: 80, total: ms(8000), wait: ms(900), held: ms(1200) });
        all.insert("an untimed piece".to_string(), Count { n: 0, total: ms(0), wait: ms(0), held: ms(300) });
        all.insert("req show".to_string(), Count { n: 2, total: ms(150), wait: ms(1), held: ms(0) });
        let l = line(all, HashMap::new()).unwrap();
        assert!(l.starts_with("coordd: costs this minute: clock follow (each second) 80x 8000 ms (lock wait 900 ms, held 1200 ms); an untimed piece 0x 0 ms (lock wait 0 ms, held 300 ms); req show 2x 150 ms (lock wait 1 ms)"), "{l}");
    }

    /// docket-core8 (paper-research N-412 p1506): the follow round's hold by part, the six largest, none under 100 ms.
    #[test]
    fn the_follow_rounds_hold_is_said_by_part() {
        let ms = Duration::from_millis;
        let rounds = |all, changed| Rounds { all, changed, ..Rounds::default() };
        assert_eq!(parts_line(HashMap::from([("views", ms(90))]), rounds(60, 2)), "");
        let p = HashMap::from([("views", ms(9000)), ("hand_clerk", ms(4000)), ("a", ms(1)), ("b", ms(2)), ("c", ms(3)), ("d", ms(4)), ("e", ms(5))]);
        assert_eq!(parts_line(p, rounds(120, 45)), "; clock follow's hold by part: views 9000 ms, hand_clerk 4000 ms, e 5 ms, d 4 ms, c 3 ms, b 2 ms; 45 of 120 rounds after a machine change; hand-over skipped 0 (nothing changed), run 0, of them 0 handed nothing; swept 0, of them 0 handed something");
    }

    /// paper-research N-1446 p3381: the hand-over's rounds, skipped or run, and the sweeps that found what the key
    /// missed, said on the minute's line.
    #[test]
    fn the_hand_over_rounds_are_counted() {
        // the static counter itself, taken as the minute's line takes it (`hand_round` is called by main's round only)
        std::mem::take(&mut *ROUNDS.lock().unwrap());
        hand_round(false, false, 0);
        hand_round(false, false, 0);
        hand_round(true, false, 0);
        hand_round(true, false, 2);
        hand_round(true, true, 0);
        hand_round(true, true, 1);
        let got = std::mem::take(&mut *ROUNDS.lock().unwrap()).hand;
        assert_eq!(got, Hand { skipped: 2, ran: 4, none: 2, swept: 2, missed: 1 });
        let l = parts_line(HashMap::from([("hand_over", Duration::from_millis(150))]), Rounds { hand: got, ..Rounds::default() });
        assert!(l.ends_with("hand-over skipped 2 (nothing changed), run 4, of them 2 handed nothing; swept 2, of them 1 handed something"), "{l}");
    }

    /// a2h-speed (paper-research N-560 p56): the minute's line ends with the resident size and the main heap's part.
    #[test]
    fn the_line_ends_with_the_resident_size_and_the_main_heap() {
        let status = "Name:\tcoordd\nVmHWM:\t 1218492 kB\nVmRSS:\t 1198628 kB\nRssAnon:\t 1177388 kB\n";
        let smaps = "5c8352f9b000-5c8368056000 rw-p 00000000 00:00 0                          [heap]\nSize:             344300 kB\nRss:              343040 kB\n78685c000000-78685dfea000 rw-p 00000000 00:00 0 \nSize:              32680 kB\nRss:               31744 kB\n";
        assert_eq!(memory(status, smaps), "; resident 1170 MB, main heap 335 MB");
        assert_eq!(memory(status, ""), "; resident 1170 MB");
        assert_eq!(memory("", smaps), "");
    }

    /// docket-core8 (paper-research N-412 p1506, 2): the allocator's part, from malloc_info's XML as glibc 2.3x writes
    /// it: two arenas, their free chunks (fast and rest) against what each holds, the blocks mapped alone at the end.
    #[test]
    fn the_line_says_what_the_arenas_hold_and_keep_free() {
        let xml = "<malloc version=\"1\">\n<heap nr=\"0\">\n<sizes>\n<size from=\"17\" to=\"32\" total=\"64\" count=\"2\"/>\n</sizes>\n<total type=\"fast\" count=\"2\" size=\"1048576\"/>\n<total type=\"rest\" count=\"9\" size=\"9437184\"/>\n<system type=\"current\" size=\"262144000\"/>\n<system type=\"max\" size=\"262144000\"/>\n<aspace type=\"total\" size=\"262144000\"/>\n<aspace type=\"mprotect\" size=\"262144000\"/>\n</heap>\n<heap nr=\"1\">\n<sizes>\n</sizes>\n<total type=\"fast\" count=\"0\" size=\"0\"/>\n<total type=\"rest\" count=\"40\" size=\"62914560\"/>\n<system type=\"current\" size=\"67108864\"/>\n<system type=\"max\" size=\"67108864\"/>\n<aspace type=\"total\" size=\"67108864\"/>\n<aspace type=\"mprotect\" size=\"67108864\"/>\n</heap>\n<total type=\"fast\" count=\"2\" size=\"1048576\"/>\n<total type=\"rest\" count=\"49\" size=\"72351744\"/>\n<total type=\"mmap\" count=\"3\" size=\"20971520\"/>\n<system type=\"current\" size=\"329252864\"/>\n<system type=\"max\" size=\"329252864\"/>\n<aspace type=\"total\" size=\"329252864\"/>\n<aspace type=\"mprotect\" size=\"329252864\"/>\n</malloc>\n";
        assert_eq!(allocator(xml), "; malloc: 2 arenas hold 314 MB, 244 MB in use, 70 MB free and kept; 20 MB in blocks mapped alone; largest arenas held/free MB 250/10, 64/60");
        assert_eq!(allocator(""), "");
        // this process's own, read as the minute's task reads it: some arena, and the text the line carries
        let own = malloc_info_now().expect("malloc_info on glibc");
        assert!(allocator(&own).starts_with("; malloc: "), "{own}");
    }

    #[tokio::test]
    async fn a_hold_is_counted_to_the_kind_that_holds_it() {
        let lock = tokio::sync::Mutex::new(());
        let kind = format!("req held-{}", std::process::id());
        scoped(async {
            let _r = Round::new(Box::leak(kind.clone().into_boxed_str()));
            let g = Held { g: lock.lock().await, at: Instant::now(), place: std::panic::Location::caller() };
            std::thread::sleep(Duration::from_millis(30));
            drop(g);
        })
        .await;
        let held = COUNTS.lock().unwrap().as_ref().and_then(|m| m.get(&kind).map(|c| c.held)).unwrap_or_default();
        assert!(held >= Duration::from_millis(30), "{held:?}");
    }

    #[tokio::test]
    async fn a_timed_piece_counts_its_lock_wait_apart() {
        let lock = std::sync::Arc::new(tokio::sync::Mutex::new(()));
        let held = lock.clone().lock_owned().await;
        let l2 = lock.clone();
        let kind = format!("req test-{}", std::process::id());
        let run = tokio::spawn({
            let kind = kind.clone();
            async move {
                timed(&kind, async {
                    let at = Instant::now();
                    let _g = l2.lock().await;
                    waited(at.elapsed());
                    tokio::time::sleep(Duration::from_millis(20)).await;
                })
                .await
            }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(held);
        run.await.unwrap();
        let c = COUNTS.lock().unwrap().as_ref().unwrap()[&kind];
        assert_eq!(c.n, 1);
        assert!(c.wait >= Duration::from_millis(40), "{c:?}");
        assert!(c.total >= c.wait + Duration::from_millis(20), "{c:?}");
    }

    #[tokio::test]
    async fn a_long_hold_names_the_timed_piece_or_the_round_that_holds_the_lock() {
        assert_eq!(holder(), "an untimed piece");
        assert_eq!(timed("request busy", async { holder() }).await, "request busy");
        let rounds = scoped(async {
            let before = holder();
            let r = Round::new("clock follow (each second)");
            let during = holder();
            drop(r);
            let r = Round::new("clock usage (each 5 s)");
            let next = holder();
            drop(r);
            (before, during, next)
        })
        .await;
        assert_eq!(rounds, ("an untimed piece".into(), "clock follow (each second)".into(), "clock usage (each 5 s)".into()));
        // a request inside a clock's task names itself, and the round again after it
        assert_eq!(scoped(async { let _r = Round::new("clock follow (each second)"); (timed("request hook", async { holder() }).await, holder()) }).await, ("request hook".into(), "clock follow (each second)".into()));
        assert_eq!(held_line(Duration::from_millis(7166), "clock follow (each second)"), "coordd: the lock held 7166 ms by clock follow (each second)");
        let m = tokio::sync::Mutex::new(5);
        let mut g = lock(&m).await;
        // taken where no kind runs: the line says where (paper-research N-560 p29, 2); a kind needs no place
        let (line, said) = (line!() - 2, holder_at(g.place));
        assert!(said.starts_with("an untimed piece (") && said.ends_with(&format!("src/counts.rs:{line})")), "{said}");
        assert_eq!(timed("request busy", async { holder_at(g.place) }).await, "request busy");
        *g += 1;
        drop(g);
        assert_eq!(*m.lock().await, 6, "the guard reads and writes through");
    }
}
