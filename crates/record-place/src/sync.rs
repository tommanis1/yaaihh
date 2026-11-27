//! The writer that sends a record to a place, the restore and catch-up that read it back, and the handover between devices.

// The open part carries the lines before it, the send's number and a hash chain over every closed part's plain
// bytes from the record's id on: a restore or catch-up checks the closed parts it holds or fetches against it, so
// no part of another record, nor one changed or put back from another state, is read as this record's.

use crate::chunk::cut;
use crate::handover::Handover;
use crate::place::{Fetched, Head, MOST_PART, Part, PlaceError, RecordPlace, Text, text_name};
use crate::seal::{Kind, PlaceKey, RecordId, Sealer, new_record, record_of};
use ring::digest::{SHA256, digest};
use std::collections::BTreeSet;

/// The chain before a record's first closed part.
fn chain_start(id: &RecordId) -> [u8; 32] {
    digest(&SHA256, &[&b"record-place/RPS3/chain"[..], id].concat()).as_ref().try_into().unwrap()
}

/// The chain C goes on to past closed part PLAIN.
fn chain_next(c: &[u8; 32], plain: &[u8]) -> [u8; 32] {
    digest(&SHA256, &[&c[..], digest(&SHA256, plain).as_ref()].concat()).as_ref().try_into().unwrap()
}

fn lines(b: &[u8]) -> u64 {
    b.iter().filter(|c| **c == b'\n').count() as u64
}

/// Where the closed parts sent end: their bytes, lines and chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct At {
    bytes: usize,
    lines: u64,
    chain: [u8; 32],
}

/// The one writer of a place: it sends what was closed since its head, and the open part with its count, send number and chain.
#[derive(Debug)]
pub struct Sender<P: RecordPlace> {
    place: P,
    sealer: Sealer,
    size: usize,
    head: Head,
    sends: u64,
    at: At,
    open: usize,
    open_lines: u64,
    texts: BTreeSet<String>,
    pending: Vec<(String, Vec<u8>)>,
}

impl<P: RecordPlace> Sender<P> {
    /// The first writer of an empty place, with a new record's id; a place that holds a record is refused: [`Sender::resume`] from its restore.
    pub fn start(place: P, key: &PlaceKey, size: usize) -> Result<Sender<P>, PlaceError> {
        let head = place.head()?;
        if !head.rev.is_empty() {
            return Err(PlaceError::Verify("the place holds a record: restore it and go on from that".into()));
        }
        let id = new_record()?;
        let at = At { bytes: 0, lines: 0, chain: chain_start(&id) };
        Ok(Sender { place, sealer: key.record(&id), size, head, sends: 0, at, open: 0, open_lines: 0, texts: BTreeSet::new(), pending: Vec::new() })
    }

    /// The writer going on from R, restored from PLACE: its first send is refused if the place moved on since R was read.
    pub fn resume(place: P, key: &PlaceKey, size: usize, r: &Restored) -> Sender<P> {
        let at = At { bytes: r.closed_bytes, lines: r.closed_lines, chain: r.chain };
        let texts = r.texts.iter().map(|(n, _)| n.clone()).collect();
        Sender { place, sealer: key.record(&r.id), size, head: r.head.clone(), sends: r.sends, at, open: r.record.len() - r.closed_bytes, open_lines: r.lines - r.closed_lines, texts, pending: Vec::new() }
    }

    /// Where the place stood at this writer's last send.
    pub fn head(&self) -> &Head {
        &self.head
    }

    /// Refuses when another writer sent since this one's last send.
    pub fn check(&self) -> Result<(), PlaceError> {
        self.place.refuse_if_stale(&self.head)
    }

    /// The id of the record this writer sends.
    pub fn record(&self) -> RecordId {
        self.sealer.id()
    }

    /// The number of this writer's last send.
    pub fn sends(&self) -> u64 {
        self.sends
    }

    /// Whether the text NAME is at the place or goes with the next send.
    pub fn has_text(&self, name: &str) -> bool {
        self.texts.contains(name) || self.pending.iter().any(|(n, _)| n == name)
    }

    /// Whether texts wait for the next send.
    pub fn waiting(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The text NAME, PLAIN, sent with the next send, before any line that needs it.
    pub fn add_text(&mut self, name: &str, plain: Vec<u8>) -> Result<(), PlaceError> {
        text_name(name)?;
        if !self.has_text(name) {
            self.pending.push((name.to_string(), plain));
        }
        Ok(())
    }

    /// Sends RECORD's parts closed since the head, sealed, and its open part; refused when another writer sent since.
    pub fn send(&mut self, record: &[u8]) -> Result<Head, PlaceError> {
        let (closed, open) = cut(record, self.size);
        let have = self.head.closed as usize;
        if closed.len() < have || closed[..have].iter().map(|c| c.len()).sum::<usize>() != self.at.bytes {
            return Err(PlaceError::Order(format!("the record here does not hold the {have} closed parts sent")));
        }
        self.put(&closed[have..], open)
    }

    /// Sends the record from the end of the closed parts last sent: TAIL is all of it after them, so the whole record is not read again.
    pub fn send_tail(&mut self, tail: &[u8]) -> Result<Head, PlaceError> {
        let (closed, open) = cut(tail, self.size);
        self.put(&closed, open)
    }

    /// Where the closed parts sent end in the record.
    pub fn sent_to(&self) -> usize {
        self.at.bytes
    }

    /// How many of the record's lines the place holds, closed and open.
    pub fn sent_lines(&self) -> u64 {
        self.at.lines + self.open_lines
    }

    /// How many of the record's bytes the place holds, closed and open.
    pub fn sent(&self) -> usize {
        self.at.bytes + self.open
    }

    /// Sends NEW, the parts closed after those sent, and OPEN after them, with the texts waiting.
    fn put(&mut self, new: &[&[u8]], open: &[u8]) -> Result<Head, PlaceError> {
        let first = self.head.closed;
        // what a place refuses to keep is not sent: a line this long stays in the record here alone
        if let Some(l) = new.iter().map(|c| c.len()).chain([open.len()]).find(|l| *l as u64 + 128 > MOST_PART) {
            return Err(PlaceError::Order(format!("a part of {l} bytes is more than a place keeps ({MOST_PART})")));
        }
        let mut at = self.at;
        let mut sealed = Vec::with_capacity(new.len());
        for (i, c) in new.iter().enumerate() {
            sealed.push(Part { n: first + i as u64, sealed: self.sealer.seal(Kind::Closed, first + i as u64, c)? });
            at = At { bytes: at.bytes + c.len(), lines: at.lines + lines(c), chain: chain_next(&at.chain, c) };
        }
        let texts = self.pending.iter().map(|(n, p)| Ok(Text { name: n.clone(), sealed: self.sealer.seal(Kind::Text(n), 0, p)? })).collect::<Result<Vec<_>, PlaceError>>()?;
        let n = first + new.len() as u64;
        let plain = [&at.lines.to_be_bytes()[..], &(self.sends + 1).to_be_bytes(), &at.chain, open].concat();
        let open_part = Part { n, sealed: self.sealer.seal(Kind::Open, n, &plain)? };
        self.head = self.place.take_parts(&self.head, &sealed, &open_part, &texts)?;
        self.sends += 1;
        (self.at, self.open, self.open_lines) = (at, open.len(), lines(open));
        self.texts.extend(self.pending.drain(..).map(|(n, _)| n));
        Ok(self.head.clone())
    }

    /// Sends RECORD, whose last line hands the place over, and stops writing.
    pub fn hand_over(mut self, record: &[u8]) -> Result<Head, PlaceError> {
        if Handover::last_in(record).is_none() {
            return Err(PlaceError::Verify("the record's last line hands nothing over".into()));
        }
        self.send(record)
    }
}

/// What a fetch opened to: the bytes from the part asked on, the chain and lines past its closed parts, and what the open part says.
struct Opened {
    bytes: Vec<u8>,
    closed: usize,
    chain: [u8; 32],
    lines: u64,
    before: u64,
    sends: u64,
    told: [u8; 32],
}

/// FETCHED's parts of record SEALER's opened in order from part FROM, the chain going on from CHAIN.
fn opened(sealer: &Sealer, from: u64, mut chain: [u8; 32], f: &Fetched) -> Result<Opened, PlaceError> {
    let o = f.open.as_ref().ok_or_else(|| PlaceError::Verify("the place holds no open part".into()))?;
    if record_of(&o.sealed) != Some(sealer.id()) {
        return Err(PlaceError::Verify("the place holds another record".into()));
    }
    if f.head.closed != from + f.closed.len() as u64 {
        return Err(PlaceError::Verify(format!("{} closed parts given, the head says {}", from + f.closed.len() as u64, f.head.closed)));
    }
    let mut bytes = Vec::new();
    for (i, p) in f.closed.iter().enumerate() {
        if p.n != from + i as u64 {
            return Err(PlaceError::Verify(format!("part {} where {} comes", p.n, from + i as u64)));
        }
        let plain = sealer.open(Kind::Closed, p.n, &p.sealed)?;
        chain = chain_next(&chain, &plain);
        bytes.extend(plain);
    }
    let (lines, closed) = (lines(&bytes), bytes.len());
    let plain = sealer.open(Kind::Open, f.head.closed, &o.sealed)?;
    if plain.len() < 48 {
        return Err(PlaceError::Seal("the open part has no count".into()));
    }
    let (before, sends) = (u64::from_be_bytes(plain[..8].try_into().unwrap()), u64::from_be_bytes(plain[8..16].try_into().unwrap()));
    let told: [u8; 32] = plain[16..48].try_into().unwrap();
    bytes.extend(&plain[48..]);
    Ok(Opened { bytes, closed, chain, lines, before, sends, told })
}

/// A record restored from a place: its bytes, the head, its id, lines, send number, where its closed parts end, and its texts.
#[derive(Debug, Clone)]
pub struct Restored {
    pub record: Vec<u8>,
    pub head: Head,
    pub id: RecordId,
    pub lines: u64,
    pub sends: u64,
    pub closed_bytes: usize,
    pub closed_lines: u64,
    pub chain: [u8; 32],
    /// Each text by name, opened.
    pub texts: Vec<(String, Vec<u8>)>,
}

impl Restored {
    /// What was restored, said so a short one can be seen.
    pub fn said(&self) -> String {
        format!("{} lines, send {}", self.lines, self.sends)
    }
}

/// The whole record the place holds, refused unless it is record EXPECT (when given): every part opened, checked
/// against the chain and line count its open part holds. A place that lost its last sends (restored from a backup,
/// reset by hand) still gives a whole, older record: compare `sends` with the last one known to see it.
pub fn restore<P: RecordPlace>(place: &P, key: &PlaceKey, expect: Option<&RecordId>) -> Result<Restored, PlaceError> {
    let f = place.fetch_from(0)?;
    if f.head.rev.is_empty() {
        return Err(PlaceError::Verify("the place holds no record".into()));
    }
    let id = f.open.as_ref().and_then(|o| record_of(&o.sealed)).ok_or_else(|| PlaceError::Seal("the open part names no record".into()))?;
    if let Some(e) = expect.filter(|e| **e != id) {
        return Err(PlaceError::Verify(format!("the place holds record {}, not {}", hex(&id), hex(e))));
    }
    let sealer = key.record(&id);
    let o = opened(&sealer, 0, chain_start(&id), &f)?;
    if o.chain != o.told || o.lines != o.before {
        return Err(PlaceError::Verify("the closed parts are not the ones the open part was sent after".into()));
    }
    let texts = place.fetch_texts(&f.head)?.into_iter().map(|t| Ok((t.name.clone(), sealer.open(Kind::Text(&t.name), 0, &t.sealed)?))).collect::<Result<_, PlaceError>>()?;
    Ok(Restored { lines: lines(&o.bytes), closed_bytes: o.closed, closed_lines: o.lines, chain: o.chain, record: o.bytes, head: f.head, id, sends: o.sends, texts })
}

/// ID as hex.
pub fn hex(id: &RecordId) -> String {
    id.iter().map(|b| format!("{b:02x}")).collect()
}

/// What the place holds of record ID after HAVE, fetching only the parts HAVE does not hold whole; refused unless it goes on from HAVE.
pub fn catch_up<P: RecordPlace>(place: &P, key: &PlaceKey, id: &RecordId, size: usize, have: &[u8]) -> Result<Vec<u8>, PlaceError> {
    let (closed, _) = cut(have, size);
    let chain = closed.iter().fold(chain_start(id), |c, p| chain_next(&c, p));
    let f = place.fetch_from(closed.len() as u64)?;
    let o = opened(&key.record(id), closed.len() as u64, chain, &f)?;
    let held: u64 = closed.iter().map(|c| lines(c)).sum();
    let tail = &have[closed.iter().map(|c| c.len()).sum::<usize>()..];
    match o.bytes.strip_prefix(tail) {
        Some(new) if o.chain == o.told && held + o.lines == o.before => Ok(new.to_vec()),
        _ => Err(PlaceError::Verify("the place's record does not go on from the one here".into())),
    }
}

/// DEVICE takes the place over: the record (EXPECT's, when given) restored and checked to end with its handover after exactly that many lines, then claimed with a send.
pub fn take_over<P: RecordPlace>(place: P, key: &PlaceKey, size: usize, device: &str, expect: Option<&RecordId>) -> Result<(Sender<P>, Vec<u8>), PlaceError> {
    let r = restore(&place, key, expect)?;
    let h = Handover::last_in(&r.record).ok_or_else(|| PlaceError::Verify("the record's last line hands nothing over".into()))?;
    if h.device != device {
        return Err(PlaceError::Verify(format!("the record is handed to {}, not {device}", h.device)));
    }
    if h.line != r.lines - 1 {
        return Err(PlaceError::Verify(format!("handed at line {}, but {} lines come before the handover", h.line, r.lines - 1)));
    }
    let mut s = Sender::resume(place, key, size, &r);
    s.send(&r.record)?;
    Ok((s, r.record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalDirPlace;
    use crate::fixture::{copy_dir, key, record, scratch};

    fn handed(rec: &[u8], device: &str) -> Vec<u8> {
        [rec.to_vec(), Handover::new(device, lines(rec)).unwrap().line_text().into_bytes()].concat()
    }

    fn at(d: &std::path::Path) -> LocalDirPlace {
        LocalDirPlace::open(d).unwrap()
    }

    fn back(d: &std::path::Path) -> Restored {
        restore(&at(d), &key(), None).unwrap()
    }

    #[test]
    fn a_record_sent_as_it_grows_is_restored_whole() {
        let d = scratch("send");
        let rec = record(3000);
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        for end in [rec.len() / 5, rec.len() / 2, rec.len() - 3, rec.len()] {
            s.send(&rec[..end]).unwrap();
        }
        assert!(s.head().closed > 20);
        assert_eq!(back(&d).record, rec);
        assert_eq!(s.sent(), rec.len());
    }

    #[test]
    fn closed_parts_go_up_once_and_nothing_leaves_unsealed() {
        let d = scratch("once");
        let rec = record(3000);
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        s.send(&rec[..rec.len() / 2]).unwrap();
        let first = std::fs::read(d.join("parts").join(format!("{:012}.part", 0))).unwrap();
        s.send(&rec).unwrap();
        assert_eq!(std::fs::read(d.join("parts").join(format!("{:012}.part", 0))).unwrap(), first);
        for f in std::fs::read_dir(d.join("parts")).unwrap() {
            assert!(!std::fs::read(f.unwrap().path()).unwrap().windows(8).any(|w| w == b"\"text\":\""));
        }
        assert!(restore(&at(&d), &crate::Key::from_bytes([1; 32]).place("test"), None).is_err());
        assert!(restore(&at(&d), &crate::Key::from_bytes([7; 32]).place("another"), None).is_err(), "the same key file, another place's name");
    }

    #[test]
    fn a_second_writer_is_refused_and_the_first_goes_on() {
        let d = scratch("two");
        let rec = record(600);
        let mut a = Sender::start(at(&d), &key(), 20_000).unwrap();
        let mut b = Sender::start(at(&d), &key(), 20_000).unwrap();
        a.send(&rec[..rec.len() / 2]).unwrap();
        assert!(matches!(b.send(&rec), Err(PlaceError::Stale { .. })));
        a.send(&rec).unwrap();
        assert_eq!(back(&d).record, rec);
        assert!(Sender::start(at(&d), &key(), 20_000).is_err(), "a place that holds a record is gone on from, not started");
    }

    #[test]
    fn a_writer_going_on_from_a_restore_is_refused_when_another_sent_since() {
        let d = scratch("race");
        let rec = record(600);
        let mut a = Sender::start(at(&d), &key(), 20_000).unwrap();
        a.send(&rec[..rec.len() / 2]).unwrap();
        let r = back(&d);
        // between B's restore and its first send, A sends lines B never read
        a.send(&rec).unwrap();
        let mut b = Sender::resume(at(&d), &key(), 20_000, &r);
        assert!(matches!(b.send(&r.record), Err(PlaceError::Stale { .. })));
        assert_eq!(back(&d).record, rec, "A's lines are not lost");
    }

    #[test]
    fn catch_up_fetches_only_what_is_new_and_refuses_another_record() {
        let d = scratch("catch");
        let rec = record(3000);
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        s.send(&rec).unwrap();
        let id = s.record();
        let have = &rec[..rec.len() * 2 / 3 / 400 * 400];
        let have = &have[..have.iter().rposition(|b| *b == b'\n').unwrap() + 1];
        assert_eq!(catch_up(&at(&d), &key(), &id, 20_000, have).unwrap(), &rec[have.len()..]);
        let mut other = have.to_vec();
        other[have.len() - 5] = b'#';
        assert!(matches!(catch_up(&at(&d), &key(), &id, 20_000, &other), Err(PlaceError::Verify(_))));
        let mut early = have.to_vec();
        early[10] = b'#';
        assert!(matches!(catch_up(&at(&d), &key(), &id, 20_000, &early), Err(PlaceError::Verify(_))), "a closed part here unlike the place's");
    }

    #[test]
    fn a_record_here_ending_at_a_closed_part_is_not_spliced_onto_another_record() {
        let (da, db) = (scratch("splicea"), scratch("spliceb"));
        // two records of the same shape under the same key, the second unlike the first in its first line only
        let ra = record(3000);
        let mut rb = ra.clone();
        rb[2] = b'X';
        let mut a = Sender::start(at(&da), &key(), 20_000).unwrap();
        a.send(&ra).unwrap();
        Sender::start(at(&db), &key(), 20_000).unwrap().send(&rb).unwrap();
        let (closed, _) = cut(&ra, 20_000);
        let have = closed[..5].concat();
        assert!(catch_up(&at(&da), &key(), &a.record(), 20_000, &have).is_ok());
        assert!(matches!(catch_up(&at(&db), &key(), &a.record(), 20_000, &have), Err(PlaceError::Verify(_))), "another record's place");
        // the other record's parts put under this record's place: refused by their seal
        std::fs::copy(db.join("open.part"), da.join("open.part")).unwrap();
        assert!(catch_up(&at(&da), &key(), &a.record(), 20_000, &have).is_err());
    }

    #[test]
    fn a_restore_of_another_record_than_the_one_known_is_refused() {
        let (da, db) = (scratch("pina"), scratch("pinb"));
        let mut a = Sender::start(at(&da), &key(), 20_000).unwrap();
        a.send(&record(100)).unwrap();
        Sender::start(at(&db), &key(), 20_000).unwrap().send(&record(100)).unwrap();
        assert!(restore(&at(&da), &key(), Some(&a.record())).is_ok());
        let e = restore(&at(&db), &key(), Some(&a.record())).unwrap_err();
        assert!(matches!(&e, PlaceError::Verify(w) if w.contains("not ")), "{e}");
    }

    #[test]
    fn device_a_hands_over_to_device_b_and_a_coming_back_is_refused() {
        let d = scratch("hand");
        let rec = record(3000);
        let mut a = Sender::start(at(&d), &key(), 20_000).unwrap();
        a.send(&rec).unwrap();
        let handed = handed(&rec, "device-b");
        assert!(matches!(a.send(&rec[..10]), Err(PlaceError::Order(_))));
        a.hand_over(&handed).unwrap();
        let mut stale = Sender::resume(at(&d), &key(), 20_000, &back(&d));
        let (mut b, got) = take_over(at(&d), &key(), 20_000, "device-b", None).unwrap();
        assert_eq!(got, handed);
        let more = [handed.clone(), b"{\"after\":1}\n".to_vec()].concat();
        b.send(&more).unwrap();
        assert!(matches!(stale.send(&more), Err(PlaceError::Stale { .. })));
        assert_eq!(back(&d).record, more);
    }

    #[test]
    fn a_takeover_by_another_device_at_another_line_or_from_words_is_refused() {
        let d = scratch("refuse");
        let rec = record(50);
        Sender::start(at(&d), &key(), 2_000).unwrap().send(&handed(&rec, "device-b")).unwrap();
        assert!(matches!(take_over(at(&d), &key(), 2_000, "device-c", None), Err(PlaceError::Verify(_))));
        let wrong = [rec.clone(), Handover::new("device-b", 7).unwrap().line_text().into_bytes()].concat();
        let d2 = scratch("refuse2");
        Sender::start(at(&d2), &key(), 2_000).unwrap().send(&wrong).unwrap();
        assert!(matches!(take_over(at(&d2), &key(), 2_000, "device-b", None), Err(PlaceError::Verify(_))));
        let words = [rec.clone(), format!("{{\"line\":\"handed to device-b at line {}\"}}\n", lines(&rec)).into_bytes()].concat();
        let d3 = scratch("refuse3");
        Sender::start(at(&d3), &key(), 2_000).unwrap().send(&words).unwrap();
        assert!(matches!(take_over(at(&d3), &key(), 2_000, "device-b", None), Err(PlaceError::Verify(_))), "words in a line are no handover");
        assert!(Sender::start(at(&scratch("refuse4")), &key(), 2_000).unwrap().hand_over(&rec).is_err());
    }

    #[test]
    fn a_part_of_another_record_sealed_with_the_same_key_is_refused() {
        let (da, db) = (scratch("reca"), scratch("recb"));
        let (ra, rb) = (record(600), [b"{\"other\":1}\n".repeat(4000)].concat());
        let mut a = Sender::start(at(&da), &key(), 20_000).unwrap();
        let mut b = Sender::start(at(&db), &key(), 20_000).unwrap();
        a.send(&ra).unwrap();
        b.send(&rb).unwrap();
        assert_ne!(a.record(), b.record());
        let part0 = |d: &std::path::Path| d.join("parts").join(format!("{:012}.part", 0));
        std::fs::copy(part0(&db), part0(&da)).unwrap();
        assert!(matches!(restore(&at(&da), &key(), None), Err(PlaceError::Seal(_))));
    }

    #[test]
    fn a_place_rolled_back_restores_whole_but_says_its_older_send() {
        let (d, old) = (scratch("roll"), scratch("rollold"));
        let rec = record(3000);
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        s.send(&rec[..rec.len() / 3]).unwrap();
        s.send(&rec[..rec.len() / 2]).unwrap();
        copy_dir(&d, &old);
        s.send(&rec).unwrap();
        let (now, before) = (back(&d), back(&old));
        assert_eq!((now.sends, now.lines, now.record.as_slice()), (3, 3000, &rec[..]));
        assert_eq!(before.sends, 2);
        assert!(before.lines < now.lines);
        assert_eq!(now.said(), "3000 lines, send 3");
        let again = Sender::resume(at(&d), &key(), 20_000, &now);
        assert_eq!((again.record(), again.sends()), (s.record(), 3), "a writer going on keeps the record and its count");
        assert!(restore(&at(&scratch("none")), &key(), None).is_err());
    }

    #[test]
    fn a_tail_sent_after_a_resume_gives_the_same_place() {
        let (d, e) = (scratch("tail"), scratch("tailwhole"));
        let rec = record(3000);
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        s.send(&rec[..rec.len() / 3]).unwrap();
        let mut s = Sender::resume(at(&d), &key(), 20_000, &back(&d));
        for end in [rec.len() / 2, rec.len() * 3 / 4, rec.len()] {
            let from = s.sent_to();
            s.send_tail(&rec[from..end]).unwrap();
        }
        let mut w = Sender::start(at(&e), &key(), 20_000).unwrap();
        w.send(&rec).unwrap();
        assert_eq!(s.head().closed, w.head().closed);
        let r = back(&d);
        assert_eq!((r.record.as_slice(), r.lines, r.sends), (&rec[..], 3000, 4));
    }

    #[test]
    fn texts_go_up_once_with_the_send_and_come_back_with_the_restore() {
        let d = scratch("texts");
        let mut s = Sender::start(at(&d), &key(), 20_000).unwrap();
        s.add_text("abc", b"module t".to_vec()).unwrap();
        assert!(s.add_text("../x", vec![]).is_err());
        s.send(&record(10)).unwrap();
        s.add_text("abc", b"never sent again".to_vec()).unwrap();
        s.send(&record(20)).unwrap();
        let r = back(&d);
        assert_eq!(r.texts, [("abc".to_string(), b"module t".to_vec())]);
        assert!(Sender::resume(at(&d), &key(), 20_000, &r).has_text("abc"));
        assert!(!std::fs::read(d.join("texts/abc")).unwrap().windows(6).any(|w| w == b"module"));
    }
}
