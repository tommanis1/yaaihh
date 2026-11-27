//! `docket check [--clingo-export] FILE...`: a policy's `.docket` modules, loaded as one text, or an `.eflint` file,
//! read as the engine reads it. It says each knot of derived types that reads itself through `Not`, `Forall`, a count,
//! `Violated` or `Enabled`, with where each such read is written, and each instance the well-founded model leaves
//! unknown; with `--clingo-export` it writes the policy as an answer set program for Clingo instead, for checking
//! offline. Exit 1 when a file does not load.

use docket_v2::norm::Core;
use docket_v2::run::norms::Norms;
use eflint::compile::Spec;
use eflint::eval::{State, polarity, undecided};
use eflint::session::Session;
use std::path::Path;

fn usage() -> ! {
    eprintln!("usage: docket check [--clingo-export] FILE.docket... | FILE.eflint");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("check") {
        usage()
    }
    let export = args.iter().any(|a| a == "--clingo-export");
    let files: Vec<&String> = args[1..].iter().filter(|a| *a != "--clingo-export").collect();
    if files.is_empty() || files.iter().any(|f| f.starts_with("--")) {
        usage()
    }
    let said = match files[..] {
        [f] if f.ends_with(".eflint") => eflint_file(Path::new(f), export),
        _ => docket_files(&files, export),
    };
    if let Err(e) = said {
        eprintln!("docket check: {e}");
        std::process::exit(1);
    }
}

/// An `.eflint` file run as the reference's command runs it, its own answers written to standard error.
fn eflint_file(path: &Path, export: bool) -> Result<(), String> {
    let mut answers = Vec::new();
    let (spec, state) = {
        let mut s = Session::new(&mut answers, true);
        if !s.run_file(path) {
            return Err(format!("{} is not read", path.display()));
        }
        (s.cfg.spec.clone(), s.cfg.state.clone())
    };
    eprint!("{}", String::from_utf8_lossy(&answers));
    say(&spec, &state, export);
    Ok(())
}

/// `.docket` modules loaded as one text into the engine's core, from no stored fact.
fn docket_files(files: &[&String], export: bool) -> Result<(), String> {
    let mut modules = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(f).map_err(|e| format!("{f}: {e}"))?;
        modules.push(docket_v2::syntax::read(&text).map_err(|e| format!("{f}:{e}"))?);
    }
    let loaded = docket_v2::check::load(modules).map_err(|e| e.to_string())?;
    let mut core = Core::default();
    core.load(&loaded.norms)?;
    let (spec, state) = core.eflint().ok_or("nothing was loaded")??;
    say(spec, state, export);
    Ok(())
}

/// The program for Clingo, or what the graph and the well-founded model say.
fn say(spec: &Spec, state: &State, export: bool) {
    if export {
        print!("{}", eflint::clingo::export(spec, state));
        return;
    }
    let g = polarity(spec);
    let strata = g.knots.iter().map(|k| k.stratum + 1).max().unwrap_or(0);
    println!("{} derived types in {} knots and {strata} strata", g.types.len(), g.knots.len());
    for k in g.knots.iter().filter(|k| !k.falling.is_empty()) {
        let types: Vec<String> = k.members.iter().map(|&i| g.types[i].to_string()).collect();
        println!("not stratified: [{}], worked out by the well-founded model", types.join(", "));
        for r in &k.falling {
            println!("  {}", r.describe());
        }
    }
    for u in undecided(spec, state) {
        println!("{}", u.message());
    }
}
