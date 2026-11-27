//! Each part sealed with ChaCha20-Poly1305 under a key of its own record and place, derived from the one key file by HKDF.

// A part's AAD binds the format, its record, kind and number (and a text's name): none can be moved, swapped or
// relabelled. The subkey binds the place's name and the record's id, so a part of another place or record does not
// open at all, even under the same key file. ring's key schedule is not ours to zero; the key file's bytes are.

use crate::place::PlaceError;
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::hkdf::{HKDF_SHA256, Prk, Salt};
use ring::rand::{SecureRandom, SystemRandom};

/// A sealed part's first bytes: the format's name and version.
pub const MAGIC: &[u8; 4] = b"RPS3";

/// Which record a part belongs to: chosen at random at the record's first send, kept in each sealed part.
pub type RecordId = [u8; 16];

/// A new record's id.
pub fn new_record() -> Result<RecordId, PlaceError> {
    let mut id = [0u8; 16];
    SystemRandom::new().fill(&mut id).map_err(|_| PlaceError::Seal("no random bytes".into()))?;
    Ok(id)
}

/// BYTES set to zero in a way the compiler keeps.
fn wipe(bytes: &mut [u8]) {
    for b in bytes.iter_mut() {
        // SAFETY: B is a valid, aligned, exclusive reference
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

/// The key file's 32 bytes: never printed, zeroed when dropped. Only [`Key::place`] uses it.
pub struct Key([u8; 32]);

impl Drop for Key {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(..)")
    }
}

impl Key {
    /// The key of these bytes.
    pub fn from_bytes(b: [u8; 32]) -> Key {
        Key(b)
    }

    /// A new random key.
    pub fn generate() -> Result<Key, PlaceError> {
        let mut k = Key([0; 32]);
        SystemRandom::new().fill(&mut k.0).map_err(|_| PlaceError::Seal("no random bytes".into()))?;
        Ok(k)
    }

    /// The key in FILE: a regular file of 32 bytes, this user's, that nobody else may read or write (mode 0600 or 0400).
    pub fn from_file(file: &std::path::Path) -> Result<Key, PlaceError> {
        use std::io::Read;
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let at = |w: String| PlaceError::Seal(format!("{}: {w}", file.display()));
        let io = |e: std::io::Error| PlaceError::Io(format!("{}: {e}", file.display()));
        let mut f = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(file).map_err(io)?;
        let m = f.metadata().map_err(io)?;
        // SAFETY: geteuid has no preconditions
        let me = unsafe { libc::geteuid() };
        if !m.is_file() {
            return Err(at("a key is a regular file".into()));
        }
        if m.mode() & 0o077 != 0 {
            return Err(at(format!("mode {:04o}: a key file is readable by its owner only (chmod 600)", m.mode() & 0o7777)));
        }
        if m.uid() != me {
            return Err(at(format!("owned by uid {}, not this user's {me}", m.uid())));
        }
        if m.len() != 32 {
            return Err(at("a key is 32 bytes".into()));
        }
        let mut k = Key([0; 32]);
        f.read_exact(&mut k.0).map_err(io)?;
        Ok(k)
    }

    /// The key of the place named NAME: what seals every record kept there.
    pub fn place(&self, name: &str) -> PlaceKey {
        let prk = Salt::new(HKDF_SHA256, b"record-place/RPS3/key-file").extract(&self.0);
        let len = (name.len() as u64).to_be_bytes();
        let info = [&MAGIC[..], b"place", &len[..], name.as_bytes()];
        let okm = prk.expand(&info, HKDF_SHA256).expect("one hash's length");
        PlaceKey(Prk::from(okm), name.to_string())
    }
}

/// The key of one place, from which each record's subkey is derived; holds no raw key bytes of ours.
#[derive(Clone)]
pub struct PlaceKey(Prk, String);

impl std::fmt::Debug for PlaceKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PlaceKey({:?}, ..)", self.1)
    }
}

impl PlaceKey {
    /// The sealer of record ID kept at this place.
    pub fn record(&self, id: &RecordId) -> Sealer {
        let info = [&MAGIC[..], b"record", &id[..]];
        let okm = self.0.expand(&info, &CHACHA20_POLY1305).expect("a ChaCha20 key's length");
        Sealer { key: LessSafeKey::new(UnboundKey::from(okm)), id: *id }
    }
}

/// What seals and opens the parts of one record at one place.
pub struct Sealer {
    key: LessSafeKey,
    id: RecordId,
}

impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Sealer({:02x?}, ..)", self.id)
    }
}

/// Whether a part is closed, and so final, the open one that is replaced, or a text the record is read under, by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind<'a> {
    Closed,
    Open,
    Text(&'a str),
}

const HEAD: usize = MAGIC.len() + 16 + NONCE_LEN;

impl Sealer {
    /// The record this sealer is of.
    pub fn id(&self) -> RecordId {
        self.id
    }

    fn aad(&self, kind: Kind, n: u64) -> Vec<u8> {
        let (k, name): (u8, &[u8]) = match kind {
            Kind::Closed => (b'c', b""),
            Kind::Open => (b'o', b""),
            Kind::Text(t) => (b't', t.as_bytes()),
        };
        [&MAGIC[..], &self.id, &[k], &n.to_be_bytes(), &(name.len() as u64).to_be_bytes(), name].concat()
    }

    /// PLAIN sealed as part N of KIND: magic, the record's id, a random nonce, the ciphertext and its tag.
    pub fn seal(&self, kind: Kind, n: u64, plain: &[u8]) -> Result<Vec<u8>, PlaceError> {
        let mut nonce = [0u8; NONCE_LEN];
        SystemRandom::new().fill(&mut nonce).map_err(|_| PlaceError::Seal("no random bytes".into()))?;
        let mut buf = plain.to_vec();
        self.key.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad(kind, n)), &mut buf).map_err(|_| PlaceError::Seal("could not seal".into()))?;
        Ok([&MAGIC[..], &self.id, &nonce, &buf].concat())
    }

    /// The plain bytes of SEALED, refused unless it is part N of KIND of this record at this place, unchanged.
    pub fn open(&self, kind: Kind, n: u64, sealed: &[u8]) -> Result<Vec<u8>, PlaceError> {
        let bad = || PlaceError::Seal(format!("part {n} does not open as {kind:?} part {n} of this record at this place"));
        if record_of(sealed).as_ref() != Some(&self.id) {
            return Err(bad());
        }
        let nonce: [u8; NONCE_LEN] = sealed[MAGIC.len() + 16..HEAD].try_into().unwrap();
        let mut buf = sealed[HEAD..].to_vec();
        let plain = self.key.open_in_place(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad(kind, n)), &mut buf).map_err(|_| bad())?;
        Ok(plain.to_vec())
    }
}

/// The record SEALED says it belongs to, read before it is opened ([`Sealer::open`] checks it).
pub fn record_of(sealed: &[u8]) -> Option<RecordId> {
    (sealed.len() >= HEAD && &sealed[..MAGIC.len()] == MAGIC).then(|| sealed[MAGIC.len()..MAGIC.len() + 16].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{key, scratch};
    use std::os::unix::fs::PermissionsExt;

    const R: RecordId = [1; 16];

    #[test]
    fn a_part_opens_as_it_was_sealed() {
        let s = key().record(&R).seal(Kind::Closed, 3, b"one\ntwo\n").unwrap();
        assert_eq!(key().record(&R).open(Kind::Closed, 3, &s).unwrap(), b"one\ntwo\n");
        assert_eq!(record_of(&s), Some(R));
        assert!(!s.windows(3).any(|w| w == b"one"));
    }

    #[test]
    fn another_key_place_record_number_kind_or_a_changed_byte_is_refused() {
        let s = key().record(&R).seal(Kind::Closed, 3, b"one\n").unwrap();
        assert!(Key::from_bytes([8; 32]).place("test").record(&R).open(Kind::Closed, 3, &s).is_err());
        assert!(Key::from_bytes([7; 32]).place("another").record(&R).open(Kind::Closed, 3, &s).is_err(), "the same key file, another place");
        assert!(key().record(&[2; 16]).open(Kind::Closed, 3, &s).is_err());
        assert!(key().record(&R).open(Kind::Closed, 4, &s).is_err());
        assert!(key().record(&R).open(Kind::Open, 3, &s).is_err());
        let t = key().record(&R).seal(Kind::Text("a"), 0, b"x").unwrap();
        assert!(key().record(&R).open(Kind::Text("b"), 0, &t).is_err());
        let mut c = s.clone();
        let last = c.len() - 1;
        c[last] ^= 1;
        assert!(key().record(&R).open(Kind::Closed, 3, &c).is_err());
        let mut relabelled = s.clone();
        relabelled[4..20].copy_from_slice(&[2; 16]);
        assert!(key().record(&[2; 16]).open(Kind::Closed, 3, &relabelled).is_err(), "a part of one record relabelled as another's");
        assert!(key().record(&R).open(Kind::Closed, 3, &s[..10]).is_err());
    }

    #[test]
    fn the_same_part_sealed_twice_differs() {
        assert_ne!(key().record(&R).seal(Kind::Open, 0, b"a\n").unwrap(), key().record(&R).seal(Kind::Open, 0, b"a\n").unwrap());
        assert_ne!(new_record().unwrap(), new_record().unwrap());
    }

    #[test]
    fn a_key_file_is_32_bytes_this_users_and_nobody_elses() {
        let d = scratch("key");
        let put = |name: &str, b: &[u8], mode: u32| {
            std::fs::write(d.join(name), b).unwrap();
            std::fs::set_permissions(d.join(name), std::fs::Permissions::from_mode(mode)).unwrap();
        };
        put("k", &[9u8; 32], 0o600);
        let k = Key::from_file(&d.join("k")).unwrap();
        assert_eq!(format!("{k:?}"), "Key(..)");
        put("open", &[9u8; 32], 0o644);
        let e = Key::from_file(&d.join("open")).unwrap_err().to_string();
        assert!(e.contains("mode 0644"), "{e}");
        put("group", &[9u8; 32], 0o640);
        assert!(Key::from_file(&d.join("group")).is_err());
        put("short", &[9u8; 31], 0o600);
        assert!(Key::from_file(&d.join("short")).is_err());
        std::os::unix::fs::symlink(d.join("k"), d.join("link")).unwrap();
        assert!(Key::from_file(&d.join("link")).is_err(), "a link is not followed");
        assert!(Key::generate().is_ok());
    }

    #[test]
    fn a_dropped_key_leaves_zeros() {
        let mut k = std::mem::ManuallyDrop::new(Key::from_bytes([9; 32]));
        // SAFETY: K is dropped once, here, and only its bytes are read after
        unsafe { std::ptr::drop_in_place(&mut *k) };
        assert_eq!(k.0, [0; 32]);
    }
}
