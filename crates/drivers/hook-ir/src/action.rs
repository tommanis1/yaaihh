//! std.hooks' `HookAction` as a driver reads it from the value that crosses to it in `Apply`.

use docket_v2::run::value::Value;

/// The module whose constructors these are, as the engine prefixes their names.
const MODULE: &str = "std.hooks.";

/// One answer to a moment (std.hooks, section 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Allow,
    Deny(String),
    Modify(String),
    Inject(String),
    Answer(String),
    KeepGoing(String),
    Suspend(String),
}

impl Action {
    /// The action of value `v`, a `HookAction` constructor with its one text field (Allow with none), named as the
    /// engine names a library's constructor: by its module, `std.hooks.Deny`.
    pub fn of(v: &Value) -> Result<Action, String> {
        let Value::Ctor { name, args, .. } = v else { return Err(format!("{v} is not a HookAction")) };
        let Some(name) = name.strip_prefix(MODULE) else { return Err(format!("{v} is not a HookAction")) };
        let text = || match args.as_slice() {
            [Value::Text(t)] => Ok(t.clone()),
            _ => Err(format!("{v}: a HookAction's one field is a text")),
        };
        Ok(match name {
            "Allow" if args.is_empty() => Action::Allow,
            "Deny" => Action::Deny(text()?),
            "Modify" => Action::Modify(text()?),
            "Inject" => Action::Inject(text()?),
            "Answer" => Action::Answer(text()?),
            "KeepGoing" => Action::KeepGoing(text()?),
            "Suspend" => Action::Suspend(text()?),
            _ => return Err(format!("{v} is not a HookAction")),
        })
    }

    /// Its constructor's name as std.hooks declares it, and its text field.
    pub fn named(&self) -> (&'static str, Option<&str>) {
        match self {
            Action::Allow => ("Allow", None),
            Action::Deny(t) => ("Deny", Some(t)),
            Action::Modify(t) => ("Modify", Some(t)),
            Action::Inject(t) => ("Inject", Some(t)),
            Action::Answer(t) => ("Answer", Some(t)),
            Action::KeepGoing(t) => ("KeepGoing", Some(t)),
            Action::Suspend(t) => ("Suspend", Some(t)),
        }
    }

    /// The value the engine builds for it.
    pub fn value(&self) -> Value {
        let (name, arg) = self.named();
        Value::ctor(&format!("{MODULE}{name}"), arg.map(Value::text).into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_reads_back_from_its_value() {
        let t = || "x".to_string();
        for a in [Action::Allow, Action::Deny(t()), Action::Modify(t()), Action::Inject(t()), Action::Answer(t()), Action::KeepGoing(t()), Action::Suspend(t())] {
            assert_eq!(Action::of(&a.value()).unwrap(), a);
        }
        assert!(Action::of(&Value::text("Allow")).is_err());
        assert!(Action::of(&Value::ctor("std.hooks.Deny", vec![])).is_err());
        assert!(Action::of(&Value::ctor("Allow", vec![])).is_err(), "a name without its module is another's");
        assert!(Action::of(&Value::ctor("Go", vec![])).is_err());
    }
}
