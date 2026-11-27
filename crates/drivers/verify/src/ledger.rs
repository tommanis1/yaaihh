//! The run's budget: the bucket and its limits as a text, and who answers when a cost would pass them.

use docket_v2::check::names::Loaded;
use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::Value;

/// std.verify and the texts it imports, as built into this driver.
const STD: [&str; 4] = [
    include_str!("../../../../docket/std/verify.docket"),
    include_str!("../../../../docket/std/budgets.docket"),
    include_str!("../../../../docket/std/comms.docket"),
    include_str!("../../../../docket/std/hooks.docket"),
];

/// std.verify, its imports and the run's BUDGET text, read and checked.
pub fn load(budget: &str) -> Result<Loaded, String> {
    let texts = STD.iter().chain([&budget]).map(|t| docket_v2::syntax::read(t).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
    let mut l = docket_v2::check::names::load(texts).map_err(|e| e.to_string())?;
    l.rows = docket_v2::check::typing::check(&l).map_err(|es| es.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))?;
    Ok(l)
}

/// A text stating BUCKET and its LIMITS (dimension, most), as a project's policy states its own.
pub fn limits(bucket: &str, limits: &[(String, i64)]) -> String {
    let q = |t: &str| serde_json::Value::from(t).to_string();
    let mut text = format!("module verify.budget\n\nimport std.budgets as B\n\n+B.Bucket({}).\n", q(bucket));
    for (dim, most) in limits {
        text.push_str(&format!("+B.Limit({}, {}, {most}).\n", q(bucket), q(dim)));
    }
    text
}

/// The owner's place for a run with nobody to ask: a cost past a limit is denied, saying whose the limit is.
pub struct Owner {
    pub why: String,
}

/// The Effects of the texts std.verify imports, carried so their reactions load; only Request-Budget is asked here.
const IMPORTED: [&str; 12] = [
    "std.hooks.Start-Session",
    "std.hooks.Tool-Use",
    "std.hooks.Next-Moment",
    "std.hooks.Apply",
    "std.comms.Call-Text",
    "std.comms.Model",
    "std.comms.Read-As",
    "std.comms.Pick",
    "std.comms.TextOf",
    "std.budgets.Request-Budget",
    "std.budgets.Command",
    "std.budgets.Measure",
];

impl Driver for Owner {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("owner")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        IMPORTED.iter().map(|e| (e.to_string(), Vec::new())).collect()
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        match op {
            "std.budgets.Request-Budget" => {
                let denied = Value::Ctor { name: "std.budgets.Denied".into(), args: vec![Value::Text(self.why.clone())] };
                Reply::Answer { value: denied, costs: Vec::new(), events: Vec::new() }
            }
            op => Reply::Failed(format!("{op}: a verification run carries none of it")),
        }
    }
    fn dropped(&mut self, _: u64) {}
}
