//! Protocol Buffers' wire form by hand, for the few field kinds REAPI's messages use.

/// A message being written: fields appended in any order.
#[derive(Default)]
pub struct Enc(Vec<u8>);

impl Enc {
    pub fn new() -> Self {
        Enc::default()
    }

    fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.0.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.0.push(v as u8);
    }

    fn key(&mut self, field: u32, wire: u8) {
        self.varint(u64::from(field) << 3 | u64::from(wire));
    }

    /// An integer field; zero is proto3's default and is left out.
    pub fn uint(mut self, field: u32, v: u64) -> Self {
        if v != 0 {
            self.key(field, 0);
            self.varint(v);
        }
        self
    }

    pub fn int(self, field: u32, v: i64) -> Self {
        self.uint(field, v as u64)
    }

    /// A bytes or string field; empty is the default and is left out.
    pub fn bytes(mut self, field: u32, b: &[u8]) -> Self {
        if !b.is_empty() {
            self.key(field, 2);
            self.varint(b.len() as u64);
            self.0.extend_from_slice(b);
        }
        self
    }

    pub fn str(self, field: u32, s: &str) -> Self {
        self.bytes(field, s.as_bytes())
    }

    /// A message field, written even when empty, since its presence can mean something.
    pub fn msg(mut self, field: u32, m: Enc) -> Self {
        self.key(field, 2);
        self.varint(m.0.len() as u64);
        self.0.extend(m.0);
        self
    }

    pub fn done(self) -> Vec<u8> {
        self.0
    }
}

/// A message read: its fields in order, integers and length-delimited bytes.
pub struct Msg<'a>(Vec<(u32, Field<'a>)>);

#[derive(Clone, Copy)]
enum Field<'a> {
    Int(u64),
    Bytes(&'a [u8]),
}

fn varint(b: &[u8], i: &mut usize) -> Result<u64, String> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *b.get(*i).ok_or("a varint cut short")?;
        *i += 1;
        v |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok(v);
        }
    }
    Err("a varint too long".into())
}

impl<'a> Msg<'a> {
    pub fn read(b: &'a [u8]) -> Result<Self, String> {
        let (mut out, mut i) = (Vec::new(), 0);
        while i < b.len() {
            let key = varint(b, &mut i)?;
            let field = (key >> 3) as u32;
            let f = match key & 7 {
                0 => Field::Int(varint(b, &mut i)?),
                1 | 5 => {
                    let n = if key & 7 == 1 { 8 } else { 4 };
                    let raw = b.get(i..i + n).ok_or("a fixed field cut short")?;
                    i += n;
                    Field::Int(raw.iter().rev().fold(0, |v, x| v << 8 | u64::from(*x)))
                }
                2 => {
                    let n = varint(b, &mut i)? as usize;
                    let raw = b.get(i..i.checked_add(n).ok_or("a length too long")?).ok_or("a field cut short")?;
                    i += n;
                    Field::Bytes(raw)
                }
                w => return Err(format!("wire type {w} is not read")),
            };
            out.push((field, f));
        }
        Ok(Msg(out))
    }

    /// The last value of integer field `field`, as proto3 reads a repeated scalar; 0 when absent.
    pub fn uint(&self, field: u32) -> u64 {
        self.0.iter().rev().find_map(|(f, v)| match v {
            Field::Int(n) if *f == field => Some(*n),
            _ => None,
        }).unwrap_or(0)
    }

    pub fn int(&self, field: u32) -> i64 {
        self.uint(field) as i64
    }

    /// Every value of bytes field `field`, in order.
    pub fn all(&self, field: u32) -> Vec<&'a [u8]> {
        self.0.iter().filter_map(|(f, v)| match v {
            Field::Bytes(b) if *f == field => Some(*b),
            _ => None,
        }).collect()
    }

    pub fn bytes(&self, field: u32) -> Option<&'a [u8]> {
        self.all(field).pop()
    }

    pub fn str(&self, field: u32) -> String {
        self.bytes(field).map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default()
    }

    pub fn msg(&self, field: u32) -> Result<Option<Msg<'a>>, String> {
        self.bytes(field).map(Msg::read).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_reads_back() {
        let b = Enc::new().str(1, "ab").int(2, 300).msg(3, Enc::new().uint(1, 7)).str(4, "x").str(4, "y").uint(5, 0).done();
        let m = Msg::read(&b).unwrap();
        assert_eq!((m.str(1), m.int(2), m.uint(5)), ("ab".to_string(), 300, 0));
        assert_eq!(m.msg(3).unwrap().unwrap().uint(1), 7);
        assert_eq!(m.all(4), [b"x", b"y"]);
        assert!(Msg::read(&[0x0a, 5, 1]).is_err(), "a field cut short");
    }
}
