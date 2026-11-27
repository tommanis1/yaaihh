//! What a place that keeps a record's sealed parts does.

use std::fmt;

/// Where a place stands: how many closed parts it holds, and a token that changes at every taking.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Head {
    pub closed: u64,
    pub rev: String,
}

/// One sealed part and its number in the order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub n: u64,
    pub sealed: Vec<u8>,
}

/// The most bytes a sealed part or text may be: a place holding a larger one is refused before it is read, and none is sent.
/// A part is at most the part size unless one line alone is longer, so this bounds a line too.
pub const MOST_PART: u64 = 64 << 20;

/// The file at PATH, refused unread when it is larger than MOST bytes.
pub fn read_most(path: &std::path::Path, most: u64) -> Result<Vec<u8>, PlaceError> {
    use std::io::Read;
    let io = |e: std::io::Error| PlaceError::Io(format!("{}: {e}", path.display()));
    let f = std::fs::File::open(path).map_err(io)?;
    let size = f.metadata().map_err(io)?.len();
    if size > most {
        return Err(PlaceError::Io(format!("{}: {size} bytes, more than the {most} a place keeps in one file", path.display())));
    }
    let mut b = Vec::with_capacity(size as usize);
    f.take(most + 1).read_to_end(&mut b).map_err(io)?;
    if b.len() as u64 > most {
        return Err(PlaceError::Io(format!("{}: grew past {most} bytes while read", path.display())));
    }
    Ok(b)
}

/// A file the record is read under (the texts a `loaded` line names), kept at the place by NAME beside the parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub name: String,
    pub sealed: Vec<u8>,
}

/// Whether NAME may name a text at a place: 1 to 128 of `0-9a-z._-`, not starting with `.` or `-`, so it is a file name anywhere.
pub fn text_name(name: &str) -> Result<(), PlaceError> {
    let ok = !name.is_empty() && name.len() <= 128 && !name.starts_with(['.', '-']) && name.bytes().all(|b| b.is_ascii_digit() || b.is_ascii_lowercase() || b"._-".contains(&b));
    if ok { Ok(()) } else { Err(PlaceError::Order(format!("{name:?} is not a text's name"))) }
}

/// What `fetch_from` gives: the head, the closed parts from the one asked on, and the open part.
#[derive(Clone, Debug, Default)]
pub struct Fetched {
    pub head: Head,
    pub closed: Vec<Part>,
    pub open: Option<Part>,
}

/// Why a place refused or failed.
#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// Another writer took a send since BASE: this one must stop.
    Stale { base: Head, now: Head },
    /// The parts do not follow on from the place's last one.
    Order(String),
    /// The place could not be read or written.
    Io(String),
    /// A part did not open with the key: changed, moved or sealed with another key.
    Seal(String),
    /// What was restored is not what it should be.
    Verify(String),
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlaceError::Stale { base, now } => write!(f, "stale: sent from {} parts at {:?}, the place is at {} parts at {:?}", base.closed, base.rev, now.closed, now.rev),
            PlaceError::Order(w) => write!(f, "out of order: {w}"),
            PlaceError::Io(w) => write!(f, "place: {w}"),
            PlaceError::Seal(w) => write!(f, "sealed part: {w}"),
            PlaceError::Verify(w) => write!(f, "restore: {w}"),
        }
    }
}

impl std::error::Error for PlaceError {}

/// A place a record's parts are sent to and fetched from.
pub trait RecordPlace {
    /// The place's head now.
    fn head(&self) -> Result<Head, PlaceError>;

    /// Refuses unless the place still stands at BASE, so only the writer that took the last send sends again.
    fn refuse_if_stale(&self, base: &Head) -> Result<(), PlaceError> {
        let now = self.head()?;
        if &now == base { Ok(()) } else { Err(PlaceError::Stale { base: base.clone(), now }) }
    }

    /// Appends CLOSED after BASE's parts, adds TEXTS and replaces the open part, all or nothing; refuses when BASE is not the head.
    fn take_parts(&mut self, base: &Head, closed: &[Part], open: &Part, texts: &[Text]) -> Result<Head, PlaceError>;

    /// The texts kept at the place as it stood at HEAD.
    fn fetch_texts(&self, head: &Head) -> Result<Vec<Text>, PlaceError>;

    /// The closed parts from FROM on, in order, with the open part and the head they stand at.
    fn fetch_from(&self, from: u64) -> Result<Fetched, PlaceError>;
}

/// A boxed place is a place, so a caller can pick one at run time.
impl<P: RecordPlace + ?Sized> RecordPlace for Box<P> {
    fn head(&self) -> Result<Head, PlaceError> {
        (**self).head()
    }
    fn refuse_if_stale(&self, base: &Head) -> Result<(), PlaceError> {
        (**self).refuse_if_stale(base)
    }
    fn take_parts(&mut self, base: &Head, closed: &[Part], open: &Part, texts: &[Text]) -> Result<Head, PlaceError> {
        (**self).take_parts(base, closed, open, texts)
    }
    fn fetch_texts(&self, head: &Head) -> Result<Vec<Text>, PlaceError> {
        (**self).fetch_texts(head)
    }
    fn fetch_from(&self, from: u64) -> Result<Fetched, PlaceError> {
        (**self).fetch_from(from)
    }
}

/// Refuses unless CLOSED are numbered on from BASE's count and OPEN comes right after them; gives the new count.
pub fn check_order(base: &Head, closed: &[Part], open: &Part) -> Result<u64, PlaceError> {
    for (i, p) in closed.iter().enumerate() {
        if p.n != base.closed + i as u64 {
            return Err(PlaceError::Order(format!("part {} where {} comes", p.n, base.closed + i as u64)));
        }
    }
    let count = base.closed + closed.len() as u64;
    if open.n != count {
        return Err(PlaceError::Order(format!("open part {} after {count} closed", open.n)));
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(n: u64) -> Part {
        Part { n, sealed: vec![] }
    }

    /// A place whose own staleness check says so, and whose head may not be asked.
    struct Own;

    impl RecordPlace for Own {
        fn head(&self) -> Result<Head, PlaceError> {
            panic!("the default check asked the head")
        }
        fn refuse_if_stale(&self, _: &Head) -> Result<(), PlaceError> {
            Err(PlaceError::Io("its own check".into()))
        }
        fn take_parts(&mut self, _: &Head, _: &[Part], _: &Part, _: &[Text]) -> Result<Head, PlaceError> {
            unreachable!()
        }
        fn fetch_texts(&self, _: &Head) -> Result<Vec<Text>, PlaceError> {
            unreachable!()
        }
        fn fetch_from(&self, _: u64) -> Result<Fetched, PlaceError> {
            unreachable!()
        }
    }

    #[test]
    fn a_boxed_place_checks_staleness_its_own_way() {
        let b: Box<dyn RecordPlace + Send> = Box::new(Own);
        assert_eq!(b.refuse_if_stale(&Head::default()), Err(PlaceError::Io("its own check".into())));
    }

    #[test]
    fn a_text_name_is_a_plain_file_name() {
        assert!(text_name(&"ab12".repeat(16)).is_ok());
        for bad in ["", "-x", ".x", "../x", "a/b", "A", "a b", "a\n"] {
            assert!(text_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn parts_must_follow_the_head() {
        let base = Head { closed: 2, rev: "r".into() };
        assert_eq!(check_order(&base, &[part(2), part(3)], &part(4)), Ok(4));
        assert!(matches!(check_order(&base, &[part(3)], &part(4)), Err(PlaceError::Order(_))));
        assert!(matches!(check_order(&base, &[part(2)], &part(2)), Err(PlaceError::Order(_))));
        assert_eq!(check_order(&base, &[], &part(2)), Ok(2));
    }
}
