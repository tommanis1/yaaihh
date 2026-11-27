//! The bridge's language servers (paper-research N-412 p125, docket-core2's p130): the app sends a JSON-RPC message for
//! LANG as `Request::Lsp`, and the bridge writes it, Content-Length framed, to the coord's shared server on
//! `$COORD/lsp/LANG.sock` (coordd starts it from the project's `[[lsp]]`); each message the server sends goes to the
//! app as `{"lsp":{"lang":LANG,"message":…}}`, and its end as `{"lsp":{"lang":LANG,"closed":WHY}}`. One connection per
//! lang, opened by the first message and kept while the bridge runs; a thread of its own writes to it, so the bridge's
//! stdin loop (the terminals' keys go through it too) never waits on a server that is slow to read (docket-core3,
//! N-412 p140). The app speaks LSP itself (its initialize names
//! the server side's folder); coordd is not in the path, as it is not for the terminals.

use coord_types::Reply;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

/// Where a line for the app goes (the bridge's stdout).
pub type Emit = Arc<dyn Fn(Value) + Send + Sync>;

pub struct Lsp {
    coord: String,
    emit: Emit,
    /// Each lang's writer (the messages queued for its connection), with the open it belongs to.
    conns: Arc<Mutex<HashMap<String, (u64, Sender<String>)>>>,
    next: AtomicU64,
}

impl Lsp {
    pub fn new(coord: String, emit: Emit) -> Lsp {
        Lsp { coord, emit, conns: Arc::default(), next: AtomicU64::new(0) }
    }

    /// The shared server's socket for LANG.
    pub fn socket(coord: &str, lang: &str) -> String {
        format!("{coord}/lsp/{lang}.sock")
    }

    /// Write MESSAGE to LANG's server, connecting first if this is its first.
    pub fn handle(&self, lang: &str, message: &Value) -> Reply {
        let fail = |out: String| Reply { ok: false, out, post: None };
        if lang.is_empty() || !lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return fail(format!("coord bridge: `{lang}` is not a language's name"));
        }
        if self.coord.is_empty() {
            return fail(String::from("coord bridge: COORD is not set, so there is no coord to find a shared server in"));
        }
        let body = message.to_string();
        let mut conns = self.conns.lock().unwrap();
        if !conns.contains_key(lang) {
            let path = Lsp::socket(&self.coord, lang);
            let s = match UnixStream::connect(&path) {
                Ok(s) => s,
                Err(e) => return fail(format!("no shared {lang} server in this coord ({path}: {e})")),
            };
            let Ok(r) = s.try_clone() else { return fail(format!("{path}: the connection cannot be read")) };
            let id = self.next.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = channel::<String>();
            conns.insert(lang.to_string(), (id, tx));
            // the writer: ends when the connection fails (the reader then says closed) or the bridge drops its sender
            let mut w = s;
            std::thread::spawn(move || {
                for body in rx {
                    if write!(w, "Content-Length: {}\r\n\r\n{body}", body.len()).and_then(|_| w.flush()).is_err() {
                        let _ = w.shutdown(std::net::Shutdown::Both);
                        break;
                    }
                }
            });
            let (emit, all, lang) = (self.emit.clone(), self.conns.clone(), lang.to_string());
            std::thread::spawn(move || {
                let mut r = BufReader::new(r);
                let why = loop {
                    match read_message(&mut r) {
                        Ok(Some(m)) => emit(json!({"lsp": {"lang": lang, "message": m}})),
                        Ok(None) => break String::from("the server closed the connection"),
                        Err(e) => break e.to_string(),
                    }
                };
                // forget it, unless a newer connection took the lang
                let mut a = all.lock().unwrap();
                if a.get(&lang).is_some_and(|(i, _)| *i == id) {
                    a.remove(&lang);
                }
                drop(a);
                emit(json!({"lsp": {"lang": lang, "closed": why}}));
            });
        }
        let (_, tx) = &conns[lang];
        if tx.send(body).is_err() {
            conns.remove(lang);
            return fail(format!("{lang}'s server: its connection has ended"));
        }
        Reply { ok: true, out: String::new(), post: None }
    }
}

/// One Content-Length framed message from R: its JSON (a body that is not JSON comes as its text); None at the end.
fn read_message(r: &mut impl BufRead) -> std::io::Result<Option<Value>> {
    let mut len = None;
    let mut line = String::new();
    loop {
        line.clear();
        if r.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let l = line.trim_end();
        if l.is_empty() {
            if len.is_some() {
                break;
            }
            continue;
        }
        if let Some((k, v)) = l.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; len.unwrap_or(0)];
    r.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&body).into_owned()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    /// A stand-in shared server: it answers each request with its id and method, and ends after two.
    fn stand_in(coord: &std::path::Path) -> std::thread::JoinHandle<Vec<Value>> {
        std::fs::create_dir_all(coord.join("lsp")).unwrap();
        let l = UnixListener::bind(coord.join("lsp/docket.sock")).unwrap();
        std::thread::spawn(move || {
            let (s, _) = l.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            let mut r = BufReader::new(s);
            let mut got = Vec::new();
            while got.len() < 2 {
                let Ok(Some(m)) = read_message(&mut r) else { break };
                let body = json!({"jsonrpc": "2.0", "id": m["id"], "result": {"method": m["method"]}}).to_string();
                write!(w, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
                got.push(m);
            }
            got
        })
    }

    /// N-412 p130: the app's messages reach the coord's shared server framed, each answer comes back as an `lsp` line,
    /// and the server's end as `closed`; a coord with no server says so, and a bad lang is refused.
    #[test]
    fn the_bridge_relays_the_apps_lsp_messages_to_the_shared_server() {
        let coord = std::env::temp_dir().join(format!("coordc-lsp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&coord);
        let server = stand_in(&coord);
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let lsp = Lsp::new(coord.display().to_string(), Arc::new(move |v| tx.lock().unwrap().send(v).unwrap()));
        let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"rootUri": "file:///w/é"}});
        assert!(lsp.handle("docket", &init).ok);
        assert_eq!(rx.recv().unwrap(), json!({"lsp": {"lang": "docket", "message": {"jsonrpc": "2.0", "id": 1, "result": {"method": "initialize"}}}}));
        assert!(lsp.handle("docket", &json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover"})).ok, "the same connection");
        assert_eq!(rx.recv().unwrap()["lsp"]["message"]["result"]["method"], "textDocument/hover");
        let got = server.join().unwrap();
        assert_eq!(got[0], init, "the message as sent, a multi-byte character counted in bytes");
        let closed = rx.recv().unwrap();
        assert!(closed["lsp"]["closed"].is_string(), "{closed}");
        let r = lsp.handle("agda", &json!({}));
        assert!(!r.ok && r.out.starts_with("no shared agda server in this coord"), "{}", r.out);
        assert!(!lsp.handle("../x", &json!({})).ok);
        let none = Lsp::new(String::new(), Arc::new(|_| {}));
        assert!(none.handle("docket", &json!({})).out.contains("COORD is not set"));
        let _ = std::fs::remove_dir_all(&coord);
    }

    #[test]
    fn a_message_is_read_by_its_length_whatever_the_headers() {
        let text = "Content-Type: application/vscode-jsonrpc\r\ncontent-length: 7\r\n\r\n{\"a\":1}Content-Length: 2\r\n\r\nno";
        let mut r = BufReader::new(text.as_bytes());
        assert_eq!(read_message(&mut r).unwrap(), Some(json!({"a": 1})));
        assert_eq!(read_message(&mut r).unwrap(), Some(json!("no")));
        assert_eq!(read_message(&mut r).unwrap(), None);
    }
}
