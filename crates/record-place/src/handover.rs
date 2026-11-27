//! The line a record ends with when its place is handed to another device: `{"handover":{"device":D,"line":N}}`, alone.

// It is read only as that exact line, written by [`Handover::line_text`]: words inside another line's text, or a
// line of any other shape, hand nothing over.

use crate::place::PlaceError;

/// The place handed to DEVICE after the record's first LINE lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    pub device: String,
    pub line: u64,
}

const START: &str = "{\"handover\":{\"device\":\"";
const MID: &str = "\",\"line\":";
const END: &str = "}}";

/// Whether D may name a device: 1 to 64 of `A-Za-z0-9._-`, not starting with `-` or `.`.
pub fn device_name(d: &str) -> Result<(), PlaceError> {
    let ok = (1..=64).contains(&d.len()) && !d.starts_with(['-', '.']) && d.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    if ok { Ok(()) } else { Err(PlaceError::Verify(format!("{d:?} is not a device's name"))) }
}

impl Handover {
    /// The handover to DEVICE after LINE lines; refused for a device name that is not plain.
    pub fn new(device: &str, line: u64) -> Result<Handover, PlaceError> {
        device_name(device)?;
        Ok(Handover { device: device.to_string(), line })
    }

    /// The record line that says it, with its newline.
    pub fn line_text(&self) -> String {
        format!("{START}{}{MID}{}{END}\n", self.device, self.line)
    }

    /// The handover LINE (without its newline) is, if it is exactly a handover line.
    pub fn read(line: &[u8]) -> Option<Handover> {
        let rest = std::str::from_utf8(line).ok()?.strip_prefix(START)?.strip_suffix(END)?;
        let (device, n) = rest.split_once(MID)?;
        let plain = !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && (n == "0" || !n.starts_with('0'));
        if !plain {
            return None;
        }
        Handover::new(device, n.parse().ok()?).ok()
    }

    /// The handover in RECORD's last whole line, if that line is one.
    pub fn last_in(record: &[u8]) -> Option<Handover> {
        let body = record[..record.iter().rposition(|b| *b == b'\n')? + 1].strip_suffix(b"\n")?;
        Handover::read(&body[body.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1)..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handover_line_reads_back_and_nothing_else_does() {
        let h = Handover::new("laptop-2", 4120).unwrap();
        let rec = format!("{{\"a\":1}}\n{}", h.line_text());
        assert_eq!(Handover::last_in(rec.as_bytes()), Some(h.clone()));
        assert_eq!(Handover::last_in(b"{\"a\":1}\n"), None);
        // the words inside another line, as an agent's answer or a note could carry them, hand nothing over
        for spoof in [
            "{\"text\":\"handed to laptop-2 at line 4120\"}\n".to_string(),
            format!("{{\"answered\":{{\"value\":{:?}}}}}\n", h.line_text().trim_end()),
            "{\"handover\":{\"device\":\"laptop-2\",\"line\":4120},\"x\":1}\n".into(),
            "{\"handover\":{\"device\":\"laptop-2\",\"line\":04120}}\n".into(),
            "{\"handover\":{\"device\":\"a\\\"b\",\"line\":1}}\n".into(),
        ] {
            assert_eq!(Handover::last_in(format!("{{\"a\":1}}\n{spoof}").as_bytes()), None, "{spoof}");
        }
        assert!(Handover::new("-x", 1).is_err() && Handover::new("a b", 1).is_err());
    }
}
