//! The driver interface and the capability check at the boundary (spec 10, 11, 12): what crosses to a driver.

use super::value::{Instance, Value};

/// A driver's answer to an Effect it carries out (8.3 step 5).
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// The answer now, with each cost it measured, by its dimensions (10), and the Events it performed from
    /// outside while carrying it out (8.2 step 1), in order, before the run goes on.
    Answer { value: Value, costs: Vec<(String, i64)>, events: Vec<Instance> },
    /// Not yet: the run rests until the answer comes back by the `effect` line's place (`Machine::answer`); the
    /// daemon's frame waits for it, not this crate.
    Later,
    /// It could not be carried out, and why: the machine performs `Fault(why)` in the run (12), and writes no
    /// `answered` line.
    Failed(String),
}

/// What carries out Effects for runs, and vouches for the parties of its connections (8.8).
pub trait Driver {
    /// The driver as it names itself when it connects: the fields before a `Lets` line's point and power (12),
    /// e.g. a harness and a version.
    fn name(&self) -> Vec<Value>;
    /// Its name as it carries for `party`: a driver for parties of several kinds (several harnesses) names itself
    /// by that party's, so the `Lets` lines of its kind are the ones read for it (12).
    fn name_for(&self, _party: &Value) -> Vec<Value> {
        self.name()
    }
    /// The Effects it carries out, by qualified name, each with the dimensions of cost it measures (10).
    fn effects(&self) -> Vec<(String, Vec<String>)>;
    /// The Events it may perform from outside, by qualified name (8.2 step 1); any other is refused by source.
    fn events(&self) -> Vec<String>;
    /// Carry out Effect `op` with `args`, written at place `line` of the record. An answer given `Later` goes
    /// back by `line`. Replay (11) is the machine's: a driver keeps no answers for it.
    fn carry(&mut self, op: &str, args: &[Value], line: u64) -> Reply;
    /// The run that rested on `line`, answered `Later`, is cancelled: no answer is wanted for it any more.
    fn dropped(&mut self, line: u64);
}

/// The loaded `Lets` lines, each as its field values: the driver's own fields, then the point, then the power (12).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lets(pub Vec<Vec<Value>>);

impl Lets {
    /// Whether `Lets(d̄, point, power)` is a line of the loaded text; point and power compared as values alike.
    pub fn lets(&self, driver: &[Value], point: &Value, power: &Value) -> bool {
        self.0.iter().any(|l| l.len() == driver.len() + 2 && l[..driver.len()] == *driver && l[driver.len()] == *point && l[driver.len() + 1] == *power)
    }

    /// The check at the boundary: value `v` may cross to `driver` at `point` only if every power its guarded
    /// constructors need, by their loaded declarations (`needs`), is let there. `Err` names the first power that is
    /// not, for the `Fault` the run is given.
    pub fn crossing(&self, driver: &[Value], point: &Value, v: &Value, needs: &dyn Fn(&str) -> Option<String>) -> Result<(), String> {
        let mut powers = Vec::new();
        v.powers(needs, &mut powers);
        match powers.iter().find(|w| !self.lets(driver, point, &Value::Text(w.to_string()))) {
            None => Ok(()),
            Some(w) => {
                let d: Vec<String> = driver.iter().map(Value::to_string).collect();
                Err(format!("Lets({}, {point}, {w:?}) is not a line of the loaded text: {v} does not cross", d.join(", ")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Value {
        Value::text(s)
    }

    #[test]
    fn a_guarded_value_crosses_only_where_its_power_is_let() {
        let lets = Lets(vec![vec![t("stand-in"), t("1"), t("before_check"), t("Go")], vec![t("stand-in"), t("1"), t("before_check"), t("Skip")]]);
        let d = [t("stand-in"), t("1")];
        // the declarations say Skip and Stop are guarded; a value built by anyone has no say
        let needs = |n: &str| matches!(n, "Skip" | "Stop").then(|| n.to_string());
        assert!(lets.crossing(&d, &t("before_check"), &Value::ctor("Skip", vec![t("no")]), &needs).is_ok());
        let e = lets.crossing(&d, &t("before_check"), &Value::ctor("Stop", vec![t("no")]), &needs).unwrap_err();
        assert!(e.contains("\"Stop\"") && e.contains("before_check"), "{e}");
        assert!(lets.crossing(&[t("stand-in"), t("2")], &t("before_check"), &Value::ctor("Skip", vec![t("no")]), &needs).is_err(), "another version is let nothing");
        assert!(lets.crossing(&d, &t("after"), &Value::ctor("Skip", vec![t("no")]), &needs).is_err(), "another point is let nothing");
        assert!(lets.crossing(&d, &t("after"), &Value::ctor("Go", vec![]), &needs).is_ok(), "an unguarded value needs nothing");
    }
}
