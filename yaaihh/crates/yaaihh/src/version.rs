//! A memoised part's version (Rune's `memo`): everything the part reads, hashed, so the part is built again exactly
//! when one of them changes. Types without `Hash` (Rune's widget states, coordd's records) go in by their `Debug`
//! text, written into the hasher without a string in between.

use std::fmt::{Debug, Write};
use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(Default)]
pub struct Version(DefaultHasher);

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
