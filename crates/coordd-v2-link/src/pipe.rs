//! A connection to the engine: a socket on this machine, or a command whose stdin and stdout reach one (a bridge).

use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

/// One open connection: where replies are read, where requests are written, how to end it, and its last error line.
pub struct Pipe {
    pub rd: Box<dyn Read + Send>,
    pub wr: Box<dyn Write + Send>,
    pub end: Box<dyn FnMut() + Send>,
    /// Its last error line, asked once it has been ended.
    pub said: Box<dyn FnMut() -> Option<String> + Send>,
}

/// Opens a new connection each time it is called.
pub type Connect = Box<dyn FnMut() -> io::Result<Pipe> + Send>;

/// The engine's socket at `path`.
#[cfg(unix)]
pub fn socket(path: std::path::PathBuf) -> Connect {
    Box::new(move || {
        let s = std::os::unix::net::UnixStream::connect(&path)?;
        let (rd, wr, end) = (s.try_clone()?, s.try_clone()?, s);
        Ok(Pipe { rd: Box::new(rd), wr: Box::new(wr), end: Box::new(move || drop(end.shutdown(std::net::Shutdown::Both))), said: Box::new(|| None) })
    })
}

/// Command `cmd`, its stdin and stdout joined to the engine's socket where it runs (`socat - UNIX-CONNECT:PATH`).
pub fn command(cmd: Vec<String>) -> Connect {
    Box::new(move || {
        let (prog, args) = cmd.split_first().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no command"))?;
        let mut cmd = Command::new(prog);
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // a group of its own, so what it started is ended with it and lets go of its error output
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
        let mut child = cmd.spawn()?;
        let (wr, rd, err) = (child.stdin.take().expect("piped"), child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));
        // its last non-empty error line, once every holder of its error output has closed it
        let mut reader = Some(std::thread::spawn(move || BufReader::new(err).lines().map_while(Result::ok).map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).last()));
        let said = Box::new(move || reader.take().and_then(|r| r.join().ok()).flatten());
        let end = Box::new(move || {
            #[cfg(unix)]
            // SAFETY: a signal to the group this child leads, whose id it was given at its start
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            drop((child.kill(), child.wait()));
        });
        Ok(Pipe { rd: Box::new(rd), wr: Box::new(wr), end, said })
    })
}
