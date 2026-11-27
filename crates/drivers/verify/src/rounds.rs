//! The caller's side of a refused Seal: the runner challenges the plan with why, and the verifier's run begins again,
//! for as long as the bucket pays for it.

use crate::Refused;
use docket_v2::norm::Core;
use docket_v2::run::machine::{Machine, RunId, State};
use docket_v2::run::value::{Instance, Value};

fn v(ty: &str, args: &[&str]) -> Instance {
    Instance { ty: format!("{}.{ty}", crate::MODULE), args: args.iter().map(|a| Value::text(a)).collect() }
}

/// VERIFIER's Run-Verification of CHANGE's INTENT under BUCKET, begun again after each refused Seal; the last run.
/// No count ends it: a run whose cost passes the bucket's limits ends with the budget's `Err`, and no Seal is refused.
pub fn verify(m: &mut Machine<Core>, refused: &Refused, runner: &str, verifier: &str, change: &str, intent: &str, bucket: &str) -> Result<RunId, String> {
    loop {
        let r = m.start(Value::text(verifier), Some(0), "", v("Run-Verification", &[verifier, change, intent, bucket]))?;
        m.run();
        let Some((plan, why)) = refused.borrow_mut().pop() else { return Ok(r) };
        let c = m.start(Value::text(runner), Some(0), "", v("Challenge-Plan", &[runner, change, intent, &plan, &why]))?;
        m.run();
        if m.state(c) != State::Done {
            return Err(format!("{runner} could not challenge {plan}: {:?}", m.record.last()));
        }
    }
}
