//! std.verify's verifier and E2E runner as a Docket driver, linked into no daemon yet (only verify-run runs it): a plan
//! asked of the verifier's model, each step done on the built window live on a scratch project, `Attest` only on a pass;
//! the model's tokens and the window's time charged to the run's budget.

mod ledger;
mod plan;
mod rounds;
mod seal;

pub use ledger::{limits, load, Owner};
pub use rounds::verify;
pub use seal::Run;

use docket_v2::run::driver::{Driver, Reply};
use docket_v2::run::value::{Instance, Value};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yaaihh_e2e::{digest, digest_text, expect, Look, Release, Session, Step, Window};

pub const MODULE: &str = "std.verify";

/// What a run's steps are done on: the built window live on a scratch project, or a test's stand-in.
pub trait Screen {
    fn step(&mut self, step: &Step, said: &str) -> Result<Look, String>;
    fn server_log(&self) -> String;
}

impl Screen for Session {
    fn step(&mut self, step: &Step, said: &str) -> Result<Look, String> {
        Session::step(self, step, said).cloned()
    }
    fn server_log(&self) -> String {
        self.server_log.clone()
    }
}

/// Opens a screen for one run, its files under the run's folder.
pub type Open = Box<dyn FnMut(&Path) -> Result<Box<dyn Screen>, String>>;

/// The built WINDOW run live on scratch coordds of RELEASE.
pub fn live(release: Release, window: PathBuf) -> Open {
    Box::new(move |folder| Ok(Box::new(Session::new(release.clone(), Window { exe: window.clone(), size: (1440, 900), fps: 30 }, &folder.join("looks"))?)))
}

pub struct Config {
    /// The repository whose commits the changes are.
    pub repo: PathBuf,
    /// The verifier's model: a command that reads a prompt on stdin and prints `{"answer", "tokens"}`, as `coordd call`
    /// does; the caller names it.
    pub planner: Vec<String>,
    /// The built artifact each run runs, hashed into its evidence.
    pub window: PathBuf,
    /// Where runs and sealed manifests are kept.
    pub evidence: PathBuf,
    pub open: Open,
    /// Each Delivered whose Seal attested nothing, as (plan_ref, why), for the caller to challenge the plan with.
    pub refused: Refused,
}

/// The plans a Seal refused to attest, shared with the caller.
pub type Refused = std::rc::Rc<std::cell::RefCell<Vec<(String, String)>>>;

pub struct Verify {
    cfg: Config,
    plans: HashMap<String, String>,
    current: Option<String>,
    runs: HashMap<String, (Box<dyn Screen>, Run)>,
    made: u64,
}

impl Verify {
    pub fn new(cfg: Config) -> Verify {
        Verify { cfg, plans: HashMap::new(), current: None, runs: HashMap::new(), made: 0 }
    }

    /// The model asked for a plan, having been shown what the built window shows at its start and along the paths it
    /// asked to look along.
    fn synthesize(&mut self, change: &str, intent: &str, diff: &str, weak: &[String]) -> Result<(String, i64), String> {
        let (first, folder) = self.run(change).map(|(_, r)| (r.before.clone(), r.folder.clone()))?;
        let (asked, mut tokens) = plan::run(&self.cfg.planner, &plan::explore(intent, diff, &first))?;
        let paths = plan::paths(&asked);
        let mut explored = String::new();
        for (i, path) in paths.iter().enumerate() {
            explored.push_str(&format!("After {}:\n{}\n\n", path.join(" ; "), self.look_along(&folder.join(format!("path{i}")), path)?));
        }
        let explored = if explored.is_empty() { "none was asked for".to_string() } else { explored };
        let (answer, t) = plan::run(&self.cfg.planner, &plan::ask(change, intent, diff, weak, &first, &explored))?;
        tokens += t;
        let block = plan::block(&answer).ok_or_else(|| format!("the model answered no block: {}", answer.chars().take(300).collect::<String>()))?;
        let hex = digest_text(&block);
        let dir = self.cfg.evidence.join("plans");
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        std::fs::write(dir.join(format!("{hex}.docket")), &block).map_err(|e| e.to_string())?;
        let r = format!("plan:{}", &hex[..16]);
        self.plans.insert(r.clone(), block);
        self.current = Some(r.clone());
        Ok((r, tokens))
    }

    /// The texts at the end of PATH, on a screen of its own in FOLDER; a step that cannot be done ends it, saying why.
    fn look_along(&mut self, folder: &Path, path: &[String]) -> Result<String, String> {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let mut screen = (self.cfg.open)(folder)?;
        let mut seen = String::new();
        for said in path {
            match said.parse::<Step>().and_then(|s| screen.step(&s, said)) {
                Ok(look) => seen = look.seen(),
                Err(e) => return Ok(format!("{seen}\n(step {said:?} could not be done: {e})")),
            }
        }
        Ok(seen)
    }

    /// A new run of CHANGE: a folder of its own, and the window's digest taken.
    fn fresh(&mut self, change: &str) -> Result<Run, String> {
        self.made += 1;
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let slug: String = change.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(40).collect();
        let folder = self.cfg.evidence.join("runs").join(format!("{at}-{}-{}-{slug}", std::process::id(), self.made));
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        Ok(Run { folder, window_sha256: digest(&self.cfg.window)?, ..Default::default() })
    }

    /// CHANGE's run, its screen opened and looked at before any step.
    fn run(&mut self, change: &str) -> Result<&mut (Box<dyn Screen>, Run), String> {
        if !self.runs.contains_key(change) {
            let mut run = self.fresh(change)?;
            let mut screen = (self.cfg.open)(&run.folder)?;
            let look = screen.step(&Step::Look, "look before any step")?;
            run.before = look.seen();
            run.looks.push(look);
            self.runs.insert(change.into(), (screen, run));
        }
        Ok(self.runs.get_mut(change).expect("inserted"))
    }

    fn interact(&mut self, change: &str, said: &str) -> Reply {
        let (screen, run) = match self.run(change) {
            Ok(r) => r,
            Err(e) => return Reply::Failed(format!("no screen for {change}: {e}")),
        };
        let at = std::time::Instant::now();
        let done = said.parse::<Step>().and_then(|s| screen.step(&s, said));
        match done {
            Ok(look) => {
                let seen = look.seen();
                run.looks.push(look);
                Reply::Answer { value: Value::Text(seen), costs: vec![("wall_ms".into(), ms(at))], events: Vec::new() }
            }
            Err(e) => {
                run.failed_steps.push(format!("{said}: {e}"));
                Reply::Answer { value: Value::Text(format!("step failed: {e}")), costs: vec![("wall_ms".into(), ms(at))], events: Vec::new() }
            }
        }
    }

    /// The run sealed and its screen ended; a Delivered that passed is said to have run clean, one that did not refused.
    fn seal(&mut self, args: &[Value]) -> Reply {
        let [verifier, change, intent, plan_ref, verdict] = args else { return Reply::Failed("Seal takes five fields".into()) };
        let (ch, pr) = (text(change), text(plan_ref));
        let run = match self.runs.remove(&ch) {
            Some((screen, mut run)) => {
                run.server_log = screen.server_log();
                run
            }
            None => match self.fresh(&ch) {
                Ok(run) => run,
                Err(e) => return Reply::Failed(e),
            },
        };
        let delivered = matches!(verdict, Value::Ctor { name, .. } if name.rsplit('.').next() == Some("Delivered"));
        let plan = self.plans.get(&pr).map(|b| digest_text(b)).unwrap_or_default();
        let header = json!({ "verifier": text(verifier), "change": ch, "intent": text(intent), "plan_ref": pr.clone(), "plan_sha256": plan, "verdict": verdict.to_string() });
        let evidence = match seal::seal(&run, header, &self.cfg.evidence.join("sealed")) {
            Ok(e) => e,
            Err(e) => return Reply::Failed(format!("the run could not be sealed: {e}")),
        };
        match (delivered, run.short_of_a_pass()) {
            (true, Some(why)) => {
                self.cfg.refused.borrow_mut().push((pr, why.clone()));
                Reply::Failed(format!("{evidence} attests nothing: {why}"))
            }
            (true, None) => {
                let clean = Instance { ty: format!("{MODULE}.Ran-Clean-On"), args: vec![verifier.clone(), change.clone(), intent.clone(), plan_ref.clone(), Value::Text(evidence.clone())] };
                answer(Value::Text(evidence), vec![clean])
            }
            (false, _) => answer(Value::Text(evidence), Vec::new()),
        }
    }
}

impl Driver for Verify {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("yaaihh-e2e"), Value::text(env!("CARGO_PKG_VERSION"))]
    }

    fn effects(&self) -> Vec<(String, Vec<String>)> {
        let dims = |e: &str| match e {
            "Synthesize" => vec!["tokens".to_string(), "wall_ms".into()],
            "Interact" => vec!["wall_ms".to_string()],
            _ => Vec::new(),
        };
        let mut e: Vec<(String, Vec<String>)> = ["Diff", "Synthesize", "Interact", "Expect", "Seal", "Confirm", "Await-Fix"].iter().map(|e| (format!("{MODULE}.{e}"), dims(e))).collect();
        e.push(("Gap".into(), Vec::new()));
        e
    }

    fn events(&self) -> Vec<String> {
        vec![format!("{MODULE}.Ran-Clean-On"), format!("{MODULE}.Attest")]
    }

    fn carry(&mut self, op: &str, args: &[Value], _line: u64) -> Reply {
        let a = |i: usize| args.get(i).map(text).unwrap_or_default();
        match op.strip_prefix(MODULE).and_then(|o| o.strip_prefix('.')).unwrap_or(op) {
            "Diff" => plan::diff(&self.cfg.repo, &a(0)).map_or_else(Reply::Failed, |d| answer(Value::Text(d), Vec::new())),
            "Synthesize" => {
                let weak: Vec<String> = match args.get(4) {
                    Some(Value::List(ws)) => ws.iter().map(text).collect(),
                    _ => Vec::new(),
                };
                let at = std::time::Instant::now();
                match self.synthesize(&a(1), &a(2), &a(3), &weak) {
                    Ok((r, tokens)) => Reply::Answer { value: Value::Text(r), costs: vec![("tokens".into(), tokens), ("wall_ms".into(), ms(at))], events: Vec::new() },
                    Err(e) => Reply::Failed(e),
                }
            }
            "Gap" => match self.current.as_ref().and_then(|r| self.plans.get(r)) {
                Some(block) => answer(Value::Text(block.clone()), Vec::new()),
                None => Reply::Failed("no plan was synthesized in this run to fill the scenario".into()),
            },
            "Interact" => self.interact(&a(0), &a(1)),
            "Expect" => {
                let failed = expect(&a(1), &a(2)).err().unwrap_or_default();
                let held = failed.is_empty();
                if let Some((_, run)) = self.runs.get_mut(&a(0)) {
                    run.expects.push((a(2), failed));
                } else {
                    return Reply::Failed(format!("Expect on {} before any step of it", a(0)));
                }
                answer(Value::Bool(held), Vec::new())
            }
            "Seal" => self.seal(args),
            "Confirm" => {
                let attest = Instance { ty: format!("{MODULE}.Attest"), args: args.to_vec() };
                answer(Value::Unit, vec![attest])
            }
            "Await-Fix" => Reply::Later,
            other => Reply::Failed(format!("{other}: not an effect this driver carries")),
        }
    }

    fn dropped(&mut self, _line: u64) {}
}

/// The build's line: `Built-From` is its Event, performed with the digest of what it built; it carries no Effect.
pub struct Builder;

impl Driver for Builder {
    fn name(&self) -> Vec<Value> {
        vec![Value::text("build")]
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        Vec::new()
    }
    fn events(&self) -> Vec<String> {
        vec![format!("{MODULE}.Built-From")]
    }
    fn carry(&mut self, op: &str, _: &[Value], _: u64) -> Reply {
        Reply::Failed(format!("{op}: the build carries no effect"))
    }
    fn dropped(&mut self, _: u64) {}
}

/// The milliseconds since AT, as a cost.
fn ms(at: std::time::Instant) -> i64 {
    i64::try_from(at.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn answer(value: Value, events: Vec<Instance>) -> Reply {
    Reply::Answer { value, costs: Vec::new(), events }
}

/// A field as text: a string, or the one string an identified fact is made of.
fn text(v: &Value) -> String {
    match v {
        Value::Text(s) => s.clone(),
        Value::Inst(Instance { args, .. }) if matches!(args.as_slice(), [Value::Text(_)]) => text(&args[0]),
        v => v.to_string(),
    }
}

#[cfg(test)]
mod tests;
