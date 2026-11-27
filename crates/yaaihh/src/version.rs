//! A memoised part's version (Rune's `memo`): everything the part reads, hashed, so the part is built again exactly
//! when one of them changes. Types without `Hash` (Rune's widget states, coordd's records) go in by their `Debug`
//! text, written into the hasher without a string in between.

use std::fmt::{Debug, Write};
use std::hash::{DefaultHasher, Hash, Hasher};

pub struct Version(DefaultHasher);

/// The look's revision (look.rs): every part's version holds it, so a look that arrives redraws every memoised part in
/// the next frame, with no key of its own to remember.
pub static LOOK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Default for Version {
    fn default() -> Self {
        let mut h = DefaultHasher::new();
        LOOK.load(std::sync::atomic::Ordering::Relaxed).hash(&mut h);
        Version(h)
    }
}

impl Write for Version {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write(s.as_bytes());
        Ok(())
    }
}

/// A serialised value goes in as its bytes, never held as a whole (state.rs: the window's saved state).
impl std::io::Write for Version {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.write(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Version {
    /// A hash of what it is given alone, without the look's revision: an identity (a memo's id, the keeper's state),
    /// which a new look must not change (consult-5, N-412 post 2013). A part's version is `Version::default()`.
    pub fn plain() -> Self {
        Version(DefaultHasher::new())
    }

    pub fn add(&mut self, v: impl Hash) -> &mut Self {
        v.hash(&mut self.0);
        self
    }

    pub fn debug(&mut self, v: &impl Debug) -> &mut Self {
        let _ = write!(self, "{v:?}\u{1}");
        self
    }

    pub fn hasher(&mut self) -> &mut DefaultHasher {
        &mut self.0
    }

    pub fn finish(&self) -> u64 {
        self.0.finish()
    }
}
