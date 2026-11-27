//! Functions of the caller's that a term may apply by name beside the declared types: pure, on strings and numbers.

use super::Ty;
use crate::eval::Value;
use std::rc::Rc;

/// A caller's functions: a declared type of the same name stands before one of them.
pub trait HostTable {
    /// The types of NAME's arguments and of its result, if NAME is in the table.
    fn sig(&self, name: &str) -> Option<(Vec<Ty>, Ty)>;
    /// NAME applied to ARGS, which are of the types `sig` gives.
    fn call(&self, name: &str, args: &[Value]) -> Result<Value, String>;
}

/// The table a specification's terms may apply, if any; two are the same when they are one table.
#[derive(Clone, Default)]
pub struct Host(pub Option<Rc<dyn HostTable>>);

impl Host {
    pub fn sig(&self, name: &str) -> Option<(Vec<Ty>, Ty)> {
        self.0.as_ref()?.sig(name)
    }

    pub fn call(&self, name: &str, args: &[Value]) -> Result<Value, String> {
        match &self.0 {
            Some(t) => t.call(name, args),
            None => Err(format!("{name} is no function here")),
        }
    }
}

impl PartialEq for Host {
    fn eq(&self, o: &Host) -> bool {
        match (&self.0, &o.0) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (a, b) => a.is_none() && b.is_none(),
        }
    }
}
