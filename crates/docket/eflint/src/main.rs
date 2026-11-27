//! The `eflint-rs` command: run a file of phrases, then phrases typed one per line, printing what each did.

use eflint::session::Session;
use std::io::{BufRead, Write};
use std::path::Path;

fn usage() -> ! {
    eprintln!("usage: eflint-rs <NAME>.eflint [--test-mode] [--guided]");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(file) = args.first() else { usage() };
    if args[1..].iter().any(|a| a != "--test-mode" && a != "--guided") {
        usage()
    }
    let test_mode = args.iter().any(|a| a == "--test-mode");
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut session = Session::new(&mut out, test_mode);
    // every combination of a rule's fields tried, as the reference does, unless asked to start from the facts it names
    session.cfg.spec.reading.guided = args.iter().any(|a| a == "--guided");
    session.run_file(Path::new(file));
    if test_mode {
        return;
    }
    // the rest of the session is read from standard input: phrases, `:display` (`:d`) and `:quit` (`:q`)
    let stdin = std::io::stdin();
    loop {
        let id = session.state_id;
        let mut line = String::new();
        print_prompt(id);
        match stdin.lock().read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let line = line.trim_end();
        match line.split_whitespace().next() {
            Some(":quit") | Some(":q") => return,
            Some(":display") | Some(":d") => {
                for (te, i) in session.cfg.state.contents() {
                    println!("{} = {}", te, if i.value { "True" } else { "False" });
                }
                for te in session.cfg.state.unknown() {
                    println!("{} = Unknown", te);
                }
                println!();
            }
            _ => session.run_text(line),
        }
    }
}

fn print_prompt(id: usize) {
    // written through the same handle the session uses, so the order of output is kept
    let stdout = std::io::stdout();
    let mut o = stdout.lock();
    let _ = write!(o, "#{} > ", id);
    let _ = o.flush();
}
