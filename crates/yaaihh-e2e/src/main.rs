//! `yaaihh-e2e --release DIR --window EXE --out DIR [--expect TEXT]... STEP...`: the steps on a scratch project, each look's texts said.

use std::path::PathBuf;
use yaaihh_e2e::{expect, Release, Session, Step, Window};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("yaaihh-e2e: {e}");
            std::process::exit(2)
        }
    }
}

/// Whether every `--expect` held of the last look.
fn run(args: &[String]) -> Result<bool, String> {
    let (mut release, mut window, mut out, mut wanted, mut steps) = (None, None, None, Vec::new(), Vec::new());
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--release" => release = Some(PathBuf::from(val()?)),
            "--window" => window = Some(PathBuf::from(val()?)),
            "--out" => out = Some(PathBuf::from(val()?)),
            "--expect" => wanted.push(val()?),
            s => steps.push((s.parse::<Step>()?, s.to_string())),
        }
    }
    let (Some(release), Some(exe), Some(out)) = (release, window, out) else { return Err("--release, --window and --out are needed".into()) };
    let mut s = Session::new(Release::folder(&release), Window { exe, size: (1440, 900), fps: 30 }, &out.join("looks"))?;
    if steps.is_empty() {
        steps.push((Step::Look, "look".into()));
    }
    for (step, said) in &steps {
        let l = s.step(step, said)?;
        println!("== {said} -> {}\n{}", l.png.display(), l.seen());
    }
    let seen = s.looks.last().map(|l| l.seen()).unwrap_or_default();
    let failed: Vec<String> = wanted.iter().filter_map(|w| expect(&seen, w).err()).flatten().collect();
    for f in &failed {
        println!("not as expected: {f}");
    }
    Ok(failed.is_empty())
}
