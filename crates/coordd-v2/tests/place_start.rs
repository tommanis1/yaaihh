//! The start on a record place, its checks and the standing a change asks, on folders: no daemon.

use coordd_v2::sync::{Place, Tick, begin_at, known, tick};
use record_place::{Fetched, Head, Key, LocalDirPlace, Part, PlaceError, PlaceKey, RecordPlace, Sender, Text, restore};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("place-start-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn key() -> PlaceKey {
    Key::from_bytes([5; 32]).place("record")
}

fn lines(from: usize, to: usize) -> Vec<u8> {
    (from..to).flat_map(|i| format!("{{\"line\":{i},\"text\":\"{}\"}}\n", "y".repeat(100)).into_bytes()).collect()
}

/// Why a start was refused; it must have been.
fn refused<T>(r: Result<T, String>) -> String {
    r.err().expect("refused")
}

fn local(d: &Path) -> Place {
    Box::new(LocalDirPlace::open(d).unwrap())
}

/// A folder place that runs THEN once, right after its first whole fetch: another writer acting between a restore and the claim after it.
struct Racing {
    at: LocalDirPlace,
    then: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl RecordPlace for Racing {
    fn head(&self) -> Result<Head, PlaceError> {
        self.at.head()
    }
    fn take_parts(&mut self, base: &Head, closed: &[Part], open: &Part, texts: &[Text]) -> Result<Head, PlaceError> {
        self.at.take_parts(base, closed, open, texts)
    }
    fn fetch_texts(&self, head: &Head) -> Result<Vec<Text>, PlaceError> {
        self.at.fetch_texts(head)
    }
    fn fetch_from(&self, from: u64) -> Result<Fetched, PlaceError> {
        let f = self.at.fetch_from(from)?;
        if let Some(then) = self.then.lock().unwrap().take() {
            then();
        }
        Ok(f)
    }
}

#[test]
fn a_start_on_a_place_another_writer_sends_to_between_its_restore_and_its_claim_is_refused_and_loses_nothing() {
    let d = scratch("race");
    let (p, rec) = (d.join("place"), d.join("b/record.jsonl"));
    let first = lines(0, 50);
    let mut a = Sender::start(LocalDirPlace::open(&p).unwrap(), &key(), 2_000).unwrap();
    a.send(&first).unwrap();
    let more = [first.clone(), lines(50, 53)].concat();
    let then = Box::new(move || drop(a.send(&more).unwrap()));
    let racing: Place = Box::new(Racing { at: LocalDirPlace::open(&p).unwrap(), then: Mutex::new(Some(then)) });
    let e = refused(begin_at(racing, &key(), 2_000, None, &rec));
    assert!(e.contains("stale"), "{e}");
    assert_eq!(restore(&LocalDirPlace::open(&p).unwrap(), &key(), None).unwrap().record, [first, lines(50, 53)].concat(), "the other writer's lines are still there");
}

#[test]
fn a_place_rolled_back_or_holding_another_record_or_emptied_is_refused_at_start() {
    let d = scratch("pins");
    let (p, old, rec) = (d.join("place"), d.join("old"), d.join("a/record.jsonl"));
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::write(&rec, lines(0, 30)).unwrap();
    let (s, said) = begin_at(local(&p), &key(), 2_000, None, &rec).unwrap();
    assert_eq!(said, "a new place");
    let id = s.record();
    drop(s);
    record_place_copy(&p, &old);
    std::fs::write(&rec, lines(0, 60)).unwrap();
    let (s, _) = begin_at(local(&p), &key(), 2_000, None, &rec).unwrap();
    assert_eq!(known(&rec).unwrap().unwrap().sends, s.sends());
    drop(s);
    let back = refused(begin_at(local(&old), &key(), 2_000, None, &rec));
    assert!(back.contains("rolled back"), "{back}");
    let other = d.join("other");
    Sender::start(LocalDirPlace::open(&other).unwrap(), &key(), 2_000).unwrap().send(&lines(0, 60)).unwrap();
    let swapped = refused(begin_at(local(&other), &key(), 2_000, None, &rec));
    assert!(swapped.contains("not "), "{swapped}");
    let pinned = refused(begin_at(local(&p), &key(), 2_000, Some([0; 16]), &rec));
    assert!(pinned.contains("pins"), "{pinned}");
    let empty = refused(begin_at(local(&d.join("empty")), &key(), 2_000, None, &rec));
    assert!(empty.contains("was reset"), "{empty}");
    assert!(begin_at(local(&p), &key(), 2_000, Some(id), &rec).is_ok());
}

fn record_place_copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        if e.file_type().unwrap().is_dir() { record_place_copy(&e.path(), &to.join(e.file_name())) } else { std::fs::copy(e.path(), to.join(e.file_name())).unwrap(); }
    }
}

#[test]
fn the_texts_a_record_is_read_under_are_restored_with_it_on_a_new_device() {
    let d = scratch("texts");
    let (p, ra, rb) = (d.join("place"), d.join("a/record.jsonl"), d.join("b/record.jsonl"));
    let texts = vec![("t.docket".to_string(), "module t\nFact Note Identified by String\n".to_string())];
    let hash = coordd_v2::codec::hex(&coordd_v2::texts::load(&texts).unwrap().hash);
    coordd_v2::texts::keep(&ra, &hash, &texts).unwrap();
    std::fs::write(&ra, lines(0, 10)).unwrap();
    drop(begin_at(local(&p), &key(), 2_000, None, &ra).unwrap());
    let (_, said) = begin_at(local(&p), &key(), 2_000, None, &rb).unwrap();
    assert!(said.starts_with("restored"), "{said}");
    assert_eq!(coordd_v2::texts::kept(&rb, &coordd_v2::texts::load(&texts).unwrap().hash).unwrap().hash, coordd_v2::texts::load(&texts).unwrap().hash);
}

fn tagged(tag: &str, from: usize, to: usize) -> Vec<u8> {
    (from..to).flat_map(|i| format!("{{\"line\":{i},\"by\":\"{tag}\"}}\n").into_bytes()).collect()
}

#[test]
fn a_takeover_inside_the_check_window_leaves_every_line_of_both_writers_accounted_for() {
    let d = scratch("window");
    let (p, ra, rb) = (d.join("place"), d.join("a/record.jsonl"), d.join("b/record.jsonl"));
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::write(&ra, tagged("both", 0, 10)).unwrap();
    let (mut a, _) = begin_at(local(&p), &key(), 2_000, None, &ra).unwrap();
    // B restores the 10 lines and writes 5 of its own; A, before its next check, takes 3
    let (mut b, said) = begin_at(local(&p), &key(), 2_000, None, &rb).unwrap();
    assert!(said.starts_with("restored"), "{said}");
    std::fs::write(&rb, [tagged("both", 0, 10), tagged("b", 10, 15)].concat()).unwrap();
    assert_eq!(tick(&mut b, &rb), Tick::Ours);
    let a_has = [tagged("both", 0, 10), tagged("a", 10, 13)].concat();
    std::fs::write(&ra, &a_has).unwrap();
    let Tick::Taken(why) = tick(&mut a, &ra) else { panic!("A's send was taken") };
    assert!(why.contains("lines 10 to 12 of this record (3 lines) were never sent"), "{why}");
    // B's 15 lines are at the place, A's 3 in A's record alone: none dropped, none spliced into the other's
    let at = restore(&LocalDirPlace::open(&p).unwrap(), &key(), None).unwrap().record;
    assert_eq!(at, std::fs::read(&rb).unwrap());
    assert_eq!(std::fs::read(&ra).unwrap(), a_has);
    assert!(!at.windows(8).any(|w| w == b"\"by\":\"a\""));
    // A started again names them too, and does not go on
    let again = refused(begin_at(local(&p), &key(), 2_000, None, &ra));
    assert!(again.contains("does not go on") && again.contains("lines 10 to 12 of this record (3 lines) were never sent"), "{again}");
}

#[test]
fn a_place_out_of_reach_is_said_and_one_back_is_ours_again() {
    let d = scratch("reach");
    let (p, gone, rec) = (d.join("place"), d.join("gone"), d.join("a/record.jsonl"));
    std::fs::create_dir_all(d.join("a")).unwrap();
    std::fs::write(&rec, lines(0, 10)).unwrap();
    let (mut s, _) = begin_at(local(&p), &key(), 2_000, None, &rec).unwrap();
    assert_eq!(tick(&mut s, &rec), Tick::Ours);
    std::fs::rename(&p, &gone).unwrap();
    std::fs::write(&rec, lines(0, 12)).unwrap();
    assert!(matches!(tick(&mut s, &rec), Tick::Unreachable(_)));
    std::fs::rename(&gone, &p).unwrap();
    assert_eq!(tick(&mut s, &rec), Tick::Ours);
    assert_eq!(restore(&LocalDirPlace::open(&p).unwrap(), &key(), None).unwrap().record, lines(0, 12));
    let mut other = Sender::resume(LocalDirPlace::open(&p).unwrap(), &key(), 2_000, &restore(&LocalDirPlace::open(&p).unwrap(), &key(), None).unwrap());
    other.send(&lines(0, 13)).unwrap();
    assert!(matches!(tick(&mut s, &rec), Tick::Taken(_)));
}
