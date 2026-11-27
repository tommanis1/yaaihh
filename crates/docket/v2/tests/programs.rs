//! Every program of the specification's branch is read whole: the gate's two libraries, the standard library, its
//! bridges, and the two projects' programs.

use std::path::PathBuf;

fn design() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docket")
}

fn programs() -> Vec<PathBuf> {
    // every `.docket` under the two folders, found by walking them: this crate names no library or project
    fn walk(d: &std::path::Path, v: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, v);
            } else if p.extension().is_some_and(|x| x == "docket") {
                v.push(p);
            }
        }
    }
    let mut v = Vec::new();
    walk(&design(), &mut v);
    v.sort();
    v
}

#[test]
fn every_program_is_read_whole() {
    let files = programs();
    assert!(!files.is_empty(), "no program text under {}", design().display());
    let mut failed = Vec::new();
    let mut decls = 0;
    for f in &files {
        match docket_v2::syntax::read(&std::fs::read_to_string(f).unwrap()) {
            Ok(m) => decls += m.decls.len(),
            Err(e) => failed.push(format!("{}:{e}", f.strip_prefix(design()).unwrap().display())),
        }
    }
    assert!(failed.is_empty(), "{} of {} not read:\n{}", failed.len(), files.len(), failed.join("\n"));
    eprintln!("{} files, {decls} declarations", files.len());
}

/// Each of the gate's libraries loads alone, and the programs load as one text, every name resolved (14, check 2).
#[test]
fn every_program_loads_with_each_name_resolved() {
    let read = |p: &PathBuf| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap();
    let files = programs();
    let (gate, rest): (Vec<_>, Vec<_>) = files.iter().partition(|p| p.starts_with(design().join("gate")));
    for f in &gate {
        let l = docket_v2::check::load(vec![read(f)]).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        eprintln!("{}: {} names, {} normative, {} reactions, {} Lets lines", f.file_name().unwrap().to_string_lossy(), l.syms.len(), l.norms.len(), l.ons.len(), l.lets.0.len());
    }
    let l = docket_v2::check::names::load(rest.iter().map(|f| read(f)).collect()).unwrap_or_else(|e| panic!("the programs: {e}"));
    eprintln!("the programs: {} modules, {} names, {} normative, {} reactions, {} Lets lines", l.modules.len(), l.syms.len(), l.norms.len(), l.ons.len(), l.lets.0.len());
}

/// The written form of every declaration reads back to the same form: what a key or a hash reads is the text.
#[test]
fn every_program_reads_back_from_its_written_form() {
    use docket_v2::syntax::form;
    for f in programs() {
        let m = docket_v2::syntax::read(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let once: Vec<String> = m.decls.iter().map(|d| form::decl(&d.what)).collect();
        let again = docket_v2::syntax::read(&once.join("\n")).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let twice: Vec<String> = again.decls.iter().map(|d| form::decl(&d.what)).collect();
        assert_eq!(once, twice, "{}", f.display());
    }
}

/// The instances the text states (5.5), and nothing else: what the early check enumerates a point type from.
struct Stated(Vec<docket_v2::run::value::Instance>);

impl docket_v2::run::norms::Norms for Stated {
    fn load(&mut self, decls: &[docket_v2::run::norms::NormDecl]) -> Result<(), String> {
        use docket_v2::syntax::tree::{DeclKind, E, Expr};
        for d in decls {
            if let DeclKind::Holds(Expr { e: E::Apply(p, xs), .. }) = &d.decl.what {
                let args = xs.iter().map(docket_v2::check::names::literal).collect::<Option<Vec<_>>>().ok_or("a stated instance of literals")?;
                self.0.push(docket_v2::run::value::Instance { ty: p.to_string(), args });
            }
        }
        Ok(())
    }
    fn each(&mut self, c: &docket_v2::syntax::tree::Expr, env: &docket_v2::run::value::Env) -> Result<Vec<docket_v2::run::value::Env>, String> {
        use docket_v2::syntax::tree::E;
        let E::Apply(p, xs) = &c.e else { return Err("one atom".into()) };
        let [docket_v2::syntax::tree::Expr { e: E::Var(v), .. }] = &xs[..] else { return Err("one variable".into()) };
        Ok(self.0.iter().filter(|i| i.ty == p.to_string() && i.args.len() == 1).map(|i| {
            let mut e = env.clone();
            e.insert(v.clone(), i.args[0].clone());
            e
        }).collect())
    }
    fn holds(&mut self, _: &docket_v2::run::value::Instance) -> bool {
        unreachable!("the early check asks no fact")
    }
    fn cond(&mut self, _: &docket_v2::syntax::tree::Expr, _: &docket_v2::run::value::Env) -> Result<bool, String> {
        unreachable!("the early check asks no condition")
    }
    fn step(&mut self, _: &docket_v2::run::value::Instance, _: Option<&docket_v2::run::value::Value>) -> Result<docket_v2::run::norms::Step, docket_v2::run::norms::Why> {
        unreachable!("no run is stepped")
    }
    fn commit(&mut self, _: docket_v2::run::norms::Step) {}
    // what it holds never changes, so every past state is the one now
    fn view(&mut self, _: Option<usize>) {}
    fn reads(&mut self) -> [u8; 32] {
        [0; 32]
    }
}

/// A driver that names itself and carries nothing.
struct Named(Vec<docket_v2::run::value::Value>);

impl docket_v2::run::driver::Driver for Named {
    fn name(&self) -> Vec<docket_v2::run::value::Value> {
        self.0.clone()
    }
    fn effects(&self) -> Vec<(String, Vec<String>)> {
        Vec::new()
    }
    fn events(&self) -> Vec<String> {
        Vec::new()
    }
    fn carry(&mut self, _: &str, _: &[docket_v2::run::value::Value], _: u64) -> docket_v2::run::driver::Reply {
        unreachable!("no run is stepped")
    }
    fn dropped(&mut self, _: u64) {}
}

/// The project's `Serve` passes the early capability check on each harness its bridges read, and on one no bridge
/// reads: every power its arms write stands under a test of `Lets` (12, scenario 3 step 5).
#[test]
fn every_program_serve_passes_the_early_check_on_each_bridge() {
    use docket_v2::run::value::{Instance, Value};
    let read = |p: &PathBuf| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap();
    let rest: Vec<_> = programs().into_iter().filter(|p| !p.starts_with(design().join("gate"))).collect();
    let l = docket_v2::check::names::load(rest.iter().map(read).collect()).unwrap();
    let mut seen = Vec::new();
    for line in &l.lets.0 {
        let d = line[..line.len() - 2].to_vec();
        if !seen.contains(&d) {
            seen.push(d);
        }
    }
    seen.push(vec![Value::text("unread"), Value::text("0")]);
    let serves: Vec<String> = l.syms.keys().filter(|n| n.ends_with(".Serve")).cloned().collect();
    assert!(!serves.is_empty());
    for (d, act) in seen.iter().flat_map(|d| serves.iter().map(move |a| (d, a))) {
        let mut m = docket_v2::run::machine::Machine::new(l.clone(), Stated(Vec::new()), vec![Box::new(Named(d.clone()))]).unwrap();
        let r = m.start(Value::text("b1"), Some(0), "", Instance { ty: act.clone(), args: vec![Value::text("b1")] });
        assert!(r.is_ok(), "{act} on {d:?}: {r:?}");
    }
}

/// Every program of the gate's libraries and of the projects is typed, rows found and no `fn` on a cycle (14,
/// checks 3, 6, 8).
#[test]
fn every_program_is_typed() {
    let read = |p: &PathBuf| docket_v2::syntax::read(&std::fs::read_to_string(p).unwrap()).unwrap();
    let files = programs();
    let (gate, rest): (Vec<_>, Vec<_>) = files.iter().partition(|p| p.starts_with(design().join("gate")));
    let mut texts: Vec<Vec<docket_v2::syntax::tree::Module>> = gate.iter().map(|f| vec![read(f)]).collect();
    texts.push(rest.iter().map(|f| read(f)).collect());
    let mut errors = Vec::new();
    for t in texts {
        let l = docket_v2::check::names::load(t).unwrap();
        if let Err(es) = docket_v2::check::typing::check(&l) {
            errors.extend(es.iter().map(|e| e.to_string()));
        }
    }
    assert!(errors.is_empty(), "{} errors:\n{}", errors.len(), errors.join("\n"));
}
