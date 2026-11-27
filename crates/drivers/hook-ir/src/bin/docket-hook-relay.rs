//! docket-hook-relay NATIVE-NAME: a harness's hook, whole. It copies the hook's input to the agent's socket
//! (DOCKET_HOOK_SOCKET), prints the driver's answer and exits with its code, and decides nothing.
//!
//! When the socket cannot be reached it prints nothing, says why on stderr and exits 1, which a harness takes as
//! the hook failing and goes on as it would have.

use std::io::{BufRead, BufReader, Read, Write};

fn main() {
    let hook = std::env::args().nth(1).unwrap_or_default();
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    match relay(&hook, &input) {
        Ok((stdout, code)) => {
            print!("{stdout}");
            let _ = std::io::stdout().flush();
            std::process::exit(code);
        }
        Err(e) => {
            eprintln!("docket-hook-relay: {e}");
            std::process::exit(1);
        }
    }
}

fn relay(hook: &str, input: &str) -> Result<(String, i32), String> {
    let path = std::env::var("DOCKET_HOOK_SOCKET").map_err(|_| "no DOCKET_HOOK_SOCKET".to_string())?;
    let mut conn = std::os::unix::net::UnixStream::connect(&path).map_err(|e| format!("{path}: {e}"))?;
    conn.write_all((docket_hooks::wire::ask(hook, input) + "\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(&conn).read_line(&mut line).map_err(|e| e.to_string())?;
    docket_hooks::wire::read_answer(line.trim_end())
}
