//! A driver whose Effects a party answers over the socket and whose Events a party performs there, named by a file.

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;

/// One driver of the `--by-socket` file: its name, the Effects it carries, the Events it may perform.
#[derive(Debug, Default, PartialEq)]
pub struct Hand {
    pub name: String,
    pub effects: Vec<String>,
    pub events: Vec<String>,
}

impl Hand {
    /// The drivers of `text`, one line `NAME effect|event QUALIFIED-NAME` each, a name's lines one driver; `#` a note.
    pub fn read(text: &str) -> Result<Vec<Hand>, String> {
        let mut out: Vec<Hand> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let words: Vec<&str> = line.split('#').next().unwrap_or("").split_whitespace().collect();
            let (name, kind, what) = match words[..] {
                [] => continue,
                [name, kind, what] => (name, kind, what.to_string()),
                _ => return Err(format!("line {}: want NAME effect|event NAME", n + 1)),
            };
            let at = match out.iter().position(|h| h.name == name) {
                Some(i) => i,
                None => {
                    out.push(Hand { name: name.into(), ..Hand::default() });
                    out.len() - 1
                }
            };
            match kind {
                "effect" => out[at].effects.push(what),
                "event" => out[at].events.push(what),
                _ => return Err(format!("line {}: {kind} is neither effect nor event", n + 1)),
            }
        }
        Ok(out)
    }
}

impl Driver for Hand {
    fn name(&self) -> Vec<Value> {
        vec![Value::text(&self.name)]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        self.effects.iter().map(|e| (e.clone(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        self.events.clone()
    }
    fn carry(&mut self, _: &str, _: &[Value], _: u64) -> Reply {
        Reply::Later
    }
    fn dropped(&mut self, _: u64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_one_name_make_one_driver() {
        let t = "w effect a.E # note\n\n# only a note\nw event a.V\nh effect a.F\n";
        let hs = Hand::read(t).unwrap();
        assert_eq!(hs.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["w", "h"]);
        assert_eq!((hs[0].effects.clone(), hs[0].events.clone()), (vec!["a.E".to_string()], vec!["a.V".to_string()]));
        assert!(Hand::read("w carries a.E").is_err());
        assert!(Hand::read("w effect").is_err());
    }
}
