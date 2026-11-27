//! The machine's state for a window: one line of JSON asked on stdin, one answered on stdout, until stdin ends.
//! `--sampler PATH` names the devices' sampler when none is beside this binary.

use std::io::{BufRead, Write};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut given = None;
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--sampler", Some(p)) => given = Some(std::path::PathBuf::from(p)),
            _ => {
                eprintln!("usage: yaaihh-machine-state [--sampler PATH]");
                std::process::exit(2);
            }
        }
    }
    let at = machine_state::Places::home(given);
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines().map_while(Result::ok).filter(|l| !l.trim().is_empty()) {
        if writeln!(out, "{}", machine_state::answer(&line, &at)).and_then(|()| out.flush()).is_err() {
            break;
        }
    }
}
