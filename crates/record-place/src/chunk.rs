//! A record cut into parts at line ends: closed parts never change as it grows, only the open one does.

/// A closed part's most bytes, unless one line alone is longer.
pub const PART_SIZE: usize = 1 << 20;

/// The closed parts and the open part of TEXT; a last line not yet ended is left out.
pub fn cut(text: &[u8], size: usize) -> (Vec<&[u8]>, &[u8]) {
    let whole = text.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    let mut closed = Vec::new();
    let (mut start, mut end) = (0, 0);
    while end < whole {
        let next = end + text[end..whole].iter().position(|b| *b == b'\n').unwrap() + 1;
        if next - start > size && end > start {
            closed.push(&text[start..end]);
            start = end;
        }
        end = next;
    }
    (closed, &text[start..whole])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::record;

    #[test]
    fn the_parts_put_together_are_the_whole_lines() {
        let text = record(500);
        let (closed, open) = cut(&text, 4096);
        assert!(closed.len() > 10);
        assert_eq!([closed.concat(), open.to_vec()].concat(), text);
        assert!(closed.iter().all(|p| p.len() <= 4096 && p.ends_with(b"\n")));
        assert!(open.len() <= 4096);
    }

    #[test]
    fn a_line_not_yet_ended_is_left_out() {
        let mut text = record(20);
        let ended = text.len();
        text.extend_from_slice(b"{\"half\":");
        let (closed, open) = cut(&text, 1000);
        assert_eq!(closed.concat().len() + open.len(), ended);
    }

    #[test]
    fn a_line_longer_than_a_part_is_a_part_of_its_own() {
        let text = [b"a\n".to_vec(), vec![b'b'; 50], b"\n".to_vec(), b"c\n".to_vec()].concat();
        let (closed, open) = cut(&text, 10);
        assert_eq!(closed, vec![&b"a\n"[..], &text[2..53]]);
        assert_eq!(open, b"c\n");
    }

    #[test]
    fn closed_parts_stay_as_the_record_grows() {
        let text = record(400);
        let (all, _) = cut(&text, 3000);
        let mut at = 0;
        while at < text.len() {
            let (some, _) = cut(&text[..at], 3000);
            assert_eq!(some[..], all[..some.len()], "cut at byte {at}");
            at += 997;
        }
    }

    #[test]
    fn a_megabyte_part_size_on_a_fixture_record() {
        let text = record(8000);
        let (closed, open) = cut(&text, PART_SIZE);
        assert!(closed.len() >= 2 && closed.iter().all(|p| p.len() <= PART_SIZE && p.len() > PART_SIZE - 400));
        assert_eq!([closed.concat(), open.to_vec()].concat(), text);
    }
}
